// Profiling run: plays a video (looping, confidence on) and reports how long
// each stage takes per frame (decode, copy from the GPU decoder, convert,
// upload, draw per output, GPU completion per output), the frame rate of
// each output, CPU use and memory. Run it on the church PC with the videos
// you plan to use, once per decoder setting.
//
//   bun tools/e2e/profile.ts --video <file> [--seconds 60] [--decoder software|auto|<name>]
//                            [--compare] [--no-confidence] [--app <exe>] [--port 28490] [--manual-show]
//
// --compare runs software and auto decoding one after the other. Each run
// starts its own copy of the app on a scratch data folder, so the library is
// untouched, and writes a Markdown report to the current folder.

import { cpus, totalmem } from 'node:os';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { App, Remote, cpuSeconds, defaultAppPath, parseArgs, playback, residentBytes, scratchDir, videoOptions } from './harness';

const args = parseArgs();
if (typeof args.video !== 'string') throw new Error('Pass --video <file> (the video to play).');
const video = resolve(args.video);
const seconds = Number(args.seconds ?? 60);
const port = Number(args.port ?? 28490);
const confidence = args['no-confidence'] !== true;
const decoders = args.compare ? ['software', 'auto'] : [typeof args.decoder === 'string' ? args.decoder : 'software'];
const appPath = typeof args.app === 'string' ? args.app : defaultAppPath();
/** Windows before this are ignored while caches and the GPU warm up. */
const WARMUP_MS = 10_000;

type Stage = { count: number; meanMs: number; maxMs: number };
const STAGES = ['decode', 'transfer', 'convert', 'upload', 'projection draw', 'projection gpu', 'confidence draw', 'confidence gpu'] as const;
type Window = Record<(typeof STAGES)[number], Stage>;

/** Every profile window in `text`. */
function profileWindows(text: string): Window[] {
  const windows: Window[] = [];
  const stage = /(\d+) × ([\d.]+) ms \(max ([\d.]+)\)/;
  for (const line of text.split('\n')) {
    if (!line.includes('profile: ')) continue;
    const body = line.slice(line.indexOf('profile: ') + 9);
    // "decode A; transfer B; convert C; upload D; projection draw E gpu F; confidence draw G gpu H"
    const parts = body.split(/; | gpu /).map(p => p.match(stage));
    if (parts.length !== STAGES.length || parts.some(p => !p)) continue;
    windows.push(Object.fromEntries(STAGES.map((name, i) => [name, { count: +parts[i]![1], meanMs: +parts[i]![2], maxMs: +parts[i]![3] }])) as Window);
  }
  return windows;
}

function fpsLines(text: string, role: string): number[] {
  return [...text.matchAll(new RegExp(`\\b${role}: ([\\d.]+) fps while active, worst interval ([\\d.]+) ms`, 'g'))].map(m => Number(m[1]));
}

const combine = (stages: Stage[]): Stage => {
  const count = stages.reduce((n, s) => n + s.count, 0);
  return { count, meanMs: count ? stages.reduce((t, s) => t + s.meanMs * s.count, 0) / count : 0, maxMs: Math.max(0, ...stages.map(s => s.maxMs)) };
};

const reports: string[] = [];
for (const decoder of decoders) {
  const dataDir = scratchDir('profile');
  writeFileSync(join(dataDir, 'app-config.json'), JSON.stringify({ video_decoder: decoder }));
  const app = await App.start(appPath, dataDir, port, args['manual-show'] ? {} : { PALTO_SHOW_OUTPUTS: '1' });
  try {
    const serviceId = app.seedService('Profile', [
      { id: 1, title: basename(video), kind: { kind: 'video', data: { path: video, options: videoOptions({ confidence, looping: true }) } } },
    ]);
    const remote = await Remote.connect(port);
    await remote.command('load_project', { project_id: serviceId }, e => e.type === 'project_changed', 'the service to load');
    await remote.command('cue', { item_id: 1, index: 0 }, e => playback(e)?.phase === 'playing', 'the video to play', 30_000);
    if (args['manual-show']) console.log('Show the outputs now (Screens → Show).');
    console.log(`[${decoder}] warming up for ${WARMUP_MS / 1000} s, then measuring for ${seconds} s…`);
    await Bun.sleep(WARMUP_MS);
    const since = Date.now();
    const logStart = readFileSync(app.logPath).length;
    const cpuStart = await cpuSeconds(app.pid);
    const memory: number[] = [];
    while (Date.now() - since < seconds * 1000) {
      await Bun.sleep(5_000);
      if (app.process.exitCode !== null) throw new Error(`The app exited during the run (code ${app.process.exitCode})`);
      const rss = await residentBytes(app.pid);
      if (rss) memory.push(rss);
      process.stdout.write(`\r  ${Math.round((Date.now() - since) / 1000)}/${seconds} s   `);
    }
    const cpuEnd = await cpuSeconds(app.pid);
    const elapsed = (Date.now() - since) / 1000;
    console.log();
    remote.close();
    const log = readFileSync(app.logPath, 'utf8');
    // Only what was logged while measuring.
    const recent = readFileSync(app.logPath).subarray(logStart).toString('utf8');
    const windows = profileWindows(recent);
    const adapter = log.match(/Native output adapter: (.+)/)?.[1] ?? 'unknown';
    const decoding = log.match(/Decoding (\d+x\d+ .+)/)?.[1] ?? 'unknown';
    const cores = cpus().length;
    const cpu = cpuStart !== null && cpuEnd !== null ? (cpuEnd - cpuStart) / elapsed * 100 : null;
    const mean = (values: number[]) => values.length ? values.reduce((a, b) => a + b, 0) / values.length : 0;
    const rows = STAGES.map(name => {
      const stage = combine(windows.map(w => w[name]));
      return `| ${name} | ${(stage.count / elapsed).toFixed(1)} | ${stage.meanMs.toFixed(2)} | ${stage.maxMs.toFixed(2)} |`;
    });
    const report = [
      `## ${basename(video)} · decoder setting: ${decoder}`,
      '',
      `- Machine: ${cpus()[0]?.model ?? 'unknown CPU'} (${cores} threads), ${(totalmem() / 2 ** 30).toFixed(1)} GB, ${process.platform}`,
      `- GPU: ${adapter}`,
      `- Video: ${decoding}; confidence ${confidence ? 'on' : 'off'}; ${elapsed.toFixed(0)} s measured`,
      `- CPU: ${cpu === null ? 'unknown' : `${cpu.toFixed(0)}% of one thread (${(cpu / cores).toFixed(1)}% of the whole CPU)`}`,
      `- Memory: ${memory.length ? `${(mean(memory) / 2 ** 20).toFixed(0)} MB (max ${(Math.max(...memory) / 2 ** 20).toFixed(0)} MB)` : 'unknown'}`,
      `- Frame rate: projection ${mean(fpsLines(recent, 'projection')).toFixed(1)} fps, confidence ${mean(fpsLines(recent, 'confidence')).toFixed(1)} fps`,
      '',
      '| Stage | per second | mean ms | worst ms |',
      '|---|---|---|---|',
      ...rows,
      '',
      'GPU is the time from submitting a frame until the GPU finished it, including work queued ahead of it; its resolution is about 4 ms.',
      '',
    ].join('\n');
    console.log(report);
    reports.push(report);
    if (!windows.length) console.error('No profile lines were logged; were the outputs shown?');
  } finally {
    await app.stop();
    if (args.keep !== true) rmSync(dataDir, { recursive: true, force: true });
  }
}

const file = join(process.cwd(), `profile-${basename(video).replace(/\W+/g, '_')}-${new Date().toISOString().replace(/[:.]/g, '-')}.md`);
writeFileSync(file, `# PALTO profile\n\n${reports.join('\n')}`);
console.log(`Report written to ${file}`);
