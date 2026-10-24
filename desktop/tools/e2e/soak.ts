// Soak test: plays a looping video with confidence on for 30 minutes and fails
// if memory, decoders or audio outputs grow, or if frames stop arriving.
//
//   bun tools/e2e/soak.ts --video <file> [--minutes 30] [--app <exe>] [--port 28480]
//                         [--max-growth-mb 64] [--manual-show] [--keep]
//
// Starts its own copy of the app on a scratch data folder, so the operator's
// library is untouched, and shows both outputs on their saved (or default)
// displays. With --manual-show, show them yourself (Screens → Show) when
// asked: hidden outputs present nothing, so the frame checks need them.
// A CSV of every sample is written to the current folder.

import { appendFileSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { App, Remote, defaultAppPath, parseArgs, playback, residentBytes, scratchDir, videoOptions } from './harness';

const args = parseArgs();
if (typeof args.video !== 'string') throw new Error('Pass --video <file> (the video to loop).');
const minutes = Number(args.minutes ?? 30);
const port = Number(args.port ?? 28480);
const maxGrowthMb = Number(args['max-growth-mb'] ?? 64);
const SAMPLE_MS = 10_000;
/** Ignore the first samples while decoders, caches and the GPU settle. */
const WARMUP_MS = 60_000;

type Media = { decoders: number; audioOutputs: number; reopened: number; presented: number; late: number; underruns: number };
type Sample = { at: number; rss: number | null; media: Media | null; fps: Record<string, number> };

/** The latest media telemetry and per-output frame rates in the log. */
function readLog(path: string): Pick<Sample, 'media' | 'fps'> {
  let text = '';
  try { text = readFileSync(path, 'utf8'); } catch { return { media: null, fps: {} }; }
  const lines = text.split('\n');
  let media: Media | null = null;
  const fps: Record<string, number> = {};
  for (let i = lines.length - 1; i >= 0 && (!media || Object.keys(fps).length < 2); i--) {
    const line = lines[i];
    const m = !media && line.match(/decoders (\d+), audio outputs (\d+) \(reopened (\d+)\), frames presented (\d+) late (\d+), audio underruns (\d+)/);
    if (m) media = { decoders: +m[1], audioOutputs: +m[2], reopened: +m[3], presented: +m[4], late: +m[5], underruns: +m[6] };
    const f = line.match(/\b(projection|confidence): ([\d.]+) fps while active/);
    if (f && !(f[1] in fps)) fps[f[1]] = Number(f[2]);
  }
  return { media, fps };
}

const median = (values: number[]) => { const sorted = [...values].sort((a, b) => a - b); return sorted[Math.floor(sorted.length / 2)] ?? 0; };

const dataDir = scratchDir('soak');
const csv = join(process.cwd(), `soak-${new Date().toISOString().replace(/[:.]/g, '-')}.csv`);
const app = await App.start(typeof args.app === 'string' ? args.app : defaultAppPath(), dataDir, port, args['manual-show'] ? {} : { PALTO_SHOW_OUTPUTS: '1' });
const failures: string[] = [];
try {
  const serviceId = app.seedService('Soak test', [
    { id: 1, title: 'Soak video', kind: { kind: 'video', data: { path: resolve(args.video), options: videoOptions({ confidence: true, looping: true }) } } },
  ]);
  const remote = await Remote.connect(port);
  await remote.command('load_project', { project_id: serviceId }, e => e.type === 'project_changed', 'the service to load');
  await remote.command('cue', { item_id: 1, index: 0 }, e => playback(e)?.phase === 'playing', 'the video to play', 30_000);

  console.log(args['manual-show'] ? 'Video is playing. Show the outputs now (Screens → Show); waiting for both outputs to present frames…' : 'Video is playing; waiting for both outputs to present frames…');
  const showDeadline = Date.now() + (args['manual-show'] ? 10 * 60_000 : 60_000);
  while (Object.keys(readLog(app.logPath).fps).length < 2) {
    if (Date.now() > showDeadline) throw new Error('Both outputs never presented frames; are they shown?');
    await Bun.sleep(2_000);
  }

  console.log(`Soaking for ${minutes} min; samples every ${SAMPLE_MS / 1000} s in ${csv}`);
  writeFileSync(csv, 'seconds,rss_mb,decoders,audio_outputs,reopened,frames_presented,frames_late,underruns,projection_fps,confidence_fps\n');
  const started = Date.now();
  const samples: Sample[] = [];
  let lastProgress = { presented: -1, at: Date.now() };
  while (Date.now() - started < minutes * 60_000) {
    await Bun.sleep(SAMPLE_MS);
    if (app.process.exitCode !== null) { failures.push(`The app exited during the soak (code ${app.process.exitCode})`); break; }
    const sample: Sample = { at: Date.now() - started, rss: await residentBytes(app.pid), ...readLog(app.logPath) };
    samples.push(sample);
    const m = sample.media;
    appendFileSync(csv, [Math.round(sample.at / 1000), sample.rss === null ? '' : (sample.rss / 2 ** 20).toFixed(1), m?.decoders, m?.audioOutputs, m?.reopened, m?.presented, m?.late, m?.underruns, sample.fps.projection, sample.fps.confidence].map(v => v ?? '').join(',') + '\n');
    if (m && m.presented !== lastProgress.presented) lastProgress = { presented: m.presented, at: Date.now() };
    if (Date.now() - lastProgress.at > 30_000) { failures.push(`No new video frames for 30 s (at ${Math.round(sample.at / 1000)} s)`); break; }
    process.stdout.write(`\r${Math.round(sample.at / 60_000)}/${minutes} min · ${sample.rss ? (sample.rss / 2 ** 20).toFixed(0) : '?'} MB · decoders ${m?.decoders ?? '?'} · ${sample.fps.projection ?? '?'}/${sample.fps.confidence ?? '?'} fps   `);
  }
  console.log();
  remote.close();

  const settled = samples.filter(s => s.at >= WARMUP_MS);
  if (settled.length >= 6) {
    const window = Math.max(3, Math.floor(settled.length / 6));
    const memory = (part: Sample[]) => median(part.map(s => s.rss ?? 0)) / 2 ** 20;
    const growth = memory(settled.slice(-window)) - memory(settled.slice(0, window));
    console.log(`Memory: ${memory(settled.slice(0, window)).toFixed(0)} MB → ${memory(settled.slice(-window)).toFixed(0)} MB (${growth >= 0 ? '+' : ''}${growth.toFixed(1)} MB)`);
    if (settled.some(s => s.rss === null)) console.log('Memory could not be read on this platform; skipped the memory check.');
    else if (growth > maxGrowthMb) failures.push(`Memory grew by ${growth.toFixed(1)} MB (limit ${maxGrowthMb} MB)`);
    const first = settled.find(s => s.media)?.media, last = settled.findLast(s => s.media)?.media;
    if (first && last) {
      if (last.decoders > first.decoders) failures.push(`Decoders grew from ${first.decoders} to ${last.decoders}`);
      if (last.audioOutputs > first.audioOutputs) failures.push(`Audio outputs grew from ${first.audioOutputs} to ${last.audioOutputs}`);
      console.log(`Frames: ${last.presented} presented, ${last.late} late; audio underruns ${last.underruns}, device reopens ${last.reopened}`);
    } else failures.push('No media telemetry in the log');
  } else if (!failures.length) failures.push('Too few samples after warm-up to judge growth; run for at least 3 minutes');
} finally {
  await app.stop();
  if (args.keep !== true && !failures.length) rmSync(dataDir, { recursive: true, force: true });
}

if (failures.length) {
  console.error(`SOAK FAILED (data kept in ${dataDir}):\n- ${failures.join('\n- ')}`);
  process.exit(1);
}
console.log('Soak passed.');
