// End-to-end check over the WebSocket: cueing and navigation, display modes,
// media transport, saving, and loading after a restart (which also exercises
// crash recovery, since the app is killed rather than closed).
//
//   bun tools/e2e/e2e.ts [--app <exe>] [--port 28481] [--video <file>] [--keep]
//
// Runs its own copy of the app on a scratch data folder. A test video is
// generated with the `ffmpeg` command when it is installed and no --video is
// given; without either, the media transport checks are skipped.

import { rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { App, Remote, defaultAppPath, parseArgs, playback, program, project, scratchDir, videoOptions, type Json } from './harness';

const args = parseArgs();
const appPath = typeof args.app === 'string' ? args.app : defaultAppPath();
const port = Number(args.port ?? 28481);
const dataDir = scratchDir('e2e');
const results: { name: string; error?: string }[] = [];

async function step(name: string, body: () => Promise<void>) {
  try { await body(); results.push({ name }); console.log(`  ok    ${name}`); }
  catch (e) { results.push({ name, error: String(e) }); console.log(`  FAIL  ${name}: ${e}`); }
}
function expect(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

/** A 2×2 PNG, so an image cue has a real file to check. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGM4oaEBRAwQCgAhLgRhHuBtwgAAAABJRU5ErkJggg==';

async function testVideo(): Promise<string | null> {
  if (typeof args.video === 'string') return resolve(args.video);
  const path = join(dataDir, 'clip.mp4');
  try {
    const ffmpeg = Bun.spawn(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'testsrc=duration=6:size=320x180:rate=30', '-f', 'lavfi', '-i', 'sine=frequency=440:duration=6', '-shortest', '-pix_fmt', 'yuv420p', path], { stdout: 'ignore', stderr: 'inherit' });
    return (await ffmpeg.exited) === 0 ? path : null;
  } catch {
    return null;
  }
}

const image = join(dataDir, 'still.png');
writeFileSync(image, Buffer.from(PNG, 'base64'));
const video = await testVideo();
const items: Json[] = [
  { id: 1, title: 'Song', kind: { kind: 'text', data: { steps: ['Line one', 'Line two'] } } },
  { id: 2, title: 'Still', kind: { kind: 'image', data: { path: image } } },
  ...(video ? [{ id: 3, title: 'Clip', kind: { kind: 'video', data: { path: video, options: videoOptions() } } }] : []),
];

console.log(`PAKIT end-to-end (${appPath}, data in ${dataDir})`);
let app = await App.start(appPath, dataDir, port);
const serviceId = app.seedService('E2E service', items);
let remote = await Remote.connect(port);

await step('load a service', async () => {
  const loaded = remote.next(e => project(e)?.id === serviceId, 'project_changed');
  const selected = remote.next(e => program(e)?.active_item_id === 1, 'the first item to be selected');
  remote.send('load_project', { project_id: serviceId });
  await Promise.all([loaded, selected]);
});

await step('navigate lyrics with next and previous', async () => {
  let state = program(await remote.command('next', undefined, e => program(e)?.active_text === 'Line two', 'the second line'))!;
  expect(state.active_step_index === 1, `step ${state.active_step_index}`);
  state = program(await remote.command('prev', undefined, e => program(e)?.active_text === 'Line one', 'the first line'))!;
  expect(state.active_step_index === 0, `step ${state.active_step_index}`);
});

await step('cue an image', async () => {
  const state = program(await remote.command('cue', { item_id: 2, index: 0 }, e => program(e)?.active_item_id === 2, 'the image cue'))!;
  const background = state.background as Json | null;
  expect(background?.kind === 'image' && background.path === image, `background ${JSON.stringify(background)}`);
});

await step('display modes', async () => {
  for (const display of ['black', 'logo', 'clear', 'live']) {
    await remote.command('set_display', { display }, e => program(e)?.display === display, `display ${display}`);
  }
});

if (video) {
  await step('video transport', async () => {
    await remote.command('cue', { item_id: 3, index: 0 }, e => playback(e)?.phase === 'playing', 'the video to play', 20_000);
    await remote.command('pause', undefined, e => playback(e)?.paused === true, 'pause');
    const seeked = playback(await remote.command('seek', { time_ms: 2_000 }, e => (playback(e)?.position_ms as number) >= 2_000, 'the seek'))!;
    expect((seeked.generation as number) > 0, 'seek starts a new generation');
    await remote.command('stop', undefined, e => playback(e)?.position_ms === 0 && playback(e)?.paused === true, 'stop to rewind');
    await remote.command('play', undefined, e => playback(e)?.phase === 'playing', 'play again', 10_000);
    await remote.command('set_display', { display: 'logo' }, e => playback(e)?.suspended === true, 'logo to suspend video');
    await remote.command('set_display', { display: 'live' }, e => playback(e)?.suspended === false, 'live to resume video');
    await remote.command('cue', { item_id: 1, index: 1 }, e => playback(e) === null, 'leaving the video to close its session');
  });
} else {
  console.log('  skip  video transport (no --video and no ffmpeg command)');
}

await step('stale media commands are refused', async () => {
  const refused = remote.next(e => e.type === 'error', 'an error');
  remote.send('media', { session_id: 1, action: { type: 'play' } });
  await refused;
});

await step('save a service', async () => {
  const last = [...remote.events].reverse().map(program).find(p => p);
  if (last?.active_item_id !== 1 || last?.active_step_index !== 1) {
    await remote.command('cue', { item_id: 1, index: 1 }, e => program(e)?.active_step_index === 1, 'the second line');
  }
  const current = [...remote.events].reverse().map(project).find(p => p) as Json;
  const renamed = { ...current, name: 'E2E service (renamed)' };
  await remote.command('save_project', { project: renamed }, e => project(e)?.name === 'E2E service (renamed)', 'the saved service');
});

await step('restart: saved changes load and the last cue is restored', async () => {
  // Let the storage worker write the save, then kill the app (no clean exit).
  await Bun.sleep(1_000);
  remote.close();
  await app.stop();
  app = await App.start(appPath, dataDir, port);
  remote = await Remote.connect(port);
  const deadline = Date.now() + 10_000;
  let restored: { project?: Json | null; program?: Json | null } = {};
  while (Date.now() < deadline && !(restored.program?.active_item_id === 1 && restored.program?.active_step_index === 1)) {
    restored = { project: [...remote.events].reverse().map(project).find(p => p !== undefined), program: [...remote.events].reverse().map(program).find(p => p) };
    await Bun.sleep(200);
  }
  expect(restored.project?.name === 'E2E service (renamed)', `restored service ${JSON.stringify(restored.project?.name)}`);
  expect(restored.program?.active_item_id === 1 && restored.program?.active_step_index === 1, `restored cue ${JSON.stringify(restored.program)}`);
});

remote.close();
await app.stop();
const failed = results.filter(r => r.error);
if (args.keep !== true && !failed.length) rmSync(dataDir, { recursive: true, force: true });
console.log(failed.length ? `${failed.length} of ${results.length} checks failed (data kept in ${dataDir})` : `All ${results.length} checks passed.`);
process.exit(failed.length ? 1 : 0);
