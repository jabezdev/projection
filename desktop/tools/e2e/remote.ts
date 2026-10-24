// Remote access check: the phone remote page, pairing, tokens, refusing
// unpaired devices and pages from other sites, the command rate limit, OSC
// and the palto.local name. Runs against a scratch data folder with LAN
// access on, using this computer's own network address as "another device".
//
//   bun tools/e2e/remote.ts [--app <exe>] [--port 28500]

import { createHash } from 'node:crypto';
import dgram from 'node:dgram';
import { writeFileSync, readFileSync, rmSync } from 'node:fs';
import { networkInterfaces } from 'node:os';
import { join } from 'node:path';
import { App, Remote, defaultAppPath, parseArgs, program, scratchDir } from './harness';

const args = parseArgs();
const port = Number(args.port ?? 28500);
const oscPort = port + 1;
const lan = Object.values(networkInterfaces()).flat().find(i => i && i.family === 'IPv4' && !i.internal)?.address;
if (!lan) throw new Error('No network address on this computer; connect to a network first.');

const dataDir = scratchDir('remote');
const token = createHash('sha256').update(String(Math.random())).digest('hex');
writeFileSync(join(dataDir, 'remote-devices.json'), JSON.stringify({ devices: [{ id: 'test', name: 'Test phone', token_sha256: createHash('sha256').update(token).digest('hex'), paired_ms: 0, last_seen_ms: 0 }] }));
writeFileSync(join(dataDir, 'app-config.json'), JSON.stringify({ lan_enabled: true, osc_enabled: true, osc_port: oscPort }));
const app = await App.start(typeof args.app === 'string' ? args.app : defaultAppPath(), dataDir, port);
console.log(`PALTO remote check (this computer as a LAN device: ${lan})`);
const failures: string[] = [];
const check = (name: string, pass: boolean, detail = '') => {
  console.log(`  ${pass ? 'ok  ' : 'FAIL'}  ${name}${detail ? ` (${detail})` : ''}`);
  if (!pass) failures.push(name);
};
const opens = (url: string, headers?: Record<string, string>) => new Promise<WebSocket | null>(resolve => {
  const socket = new WebSocket(url, headers ? { headers } as never : undefined);
  socket.onopen = () => resolve(socket);
  socket.onerror = () => resolve(null);
});
const oscMessage = (address: string) => {
  const pad = (s: string) => { const b = Buffer.alloc(Math.ceil((s.length + 1) / 4) * 4); b.write(s); return b; };
  return Buffer.concat([pad(address), pad(',')]);
};

try {
  const page = await fetch(`http://${lan}:${port}/`);
  check('the phone remote page is served on the network', page.status === 200 && (await page.text()).includes('PALTO remote'));
  check('a paired token is recognized', (await fetch(`http://${lan}:${port}/api/check?token=${token}`)).status === 200);
  check('an unknown token is not', (await fetch(`http://${lan}:${port}/api/check?token=nope`)).status === 401);
  const pairing = await fetch(`http://${lan}:${port}/api/pair`, { method: 'POST', body: JSON.stringify({ code: '000000', name: 'x' }) });
  check('pairing is refused while no code is shown', pairing.status === 403);
  const local = await opens(`ws://127.0.0.1:${port}/`);
  check('programs on this computer connect without pairing', local !== null);
  local?.close();
  check('pages from other sites are refused', (await opens(`ws://127.0.0.1:${port}/ws`, { Origin: 'https://example.com' })) === null);
  check('unpaired devices are refused', (await opens(`ws://${lan}:${port}/ws`)) === null);
  const paired = await opens(`ws://${lan}:${port}/ws?token=${token}`);
  check('paired devices connect', paired !== null);
  if (paired) {
    const events: { type: string; payload: { message?: string } }[] = [];
    paired.onmessage = m => events.push(JSON.parse(String(m.data)));
    for (let i = 0; i < 45; i++) paired.send(JSON.stringify({ type: 'set_display', payload: { display: i % 2 ? 'black' : 'live' } }));
    await Bun.sleep(500);
    const refused = events.filter(e => e.type === 'error' && /Too many/.test(e.payload.message ?? '')).length;
    check('a burst of commands is rate limited', refused > 0 && refused < 45, `${refused} of 45 refused`);
    paired.close();
  }
  const watcher = await Remote.connect(port);
  await Bun.sleep(200);
  const before = (watcher.events.map(program).filter(Boolean).at(-1) as { display?: string } | undefined)?.display;
  const target = before === 'black' ? 'live' : 'black';
  const changed = watcher.next(e => program(e)?.display === target, 'the OSC display change', 5000).then(() => true, () => false);
  const socket = dgram.createSocket('udp4');
  socket.send(oscMessage(`/palto/${target}`), oscPort, '127.0.0.1');
  check('OSC from this computer acts', await changed);
  socket.send(oscMessage('/palto/next'), oscPort, lan);
  await Bun.sleep(300);
  socket.close();
  watcher.close();
  check('OSC from a computer not on the allowed list is ignored', readFileSync(app.logPath, 'utf8').includes('Ignored OSC /palto/next'));
  const resolver = process.platform === 'linux' ? Bun.spawnSync(['avahi-resolve', '-4', '-n', 'palto.local']) : null;
  if (resolver?.success) check('palto.local resolves', resolver.stdout.toString().includes('palto.local'), resolver.stdout.toString().trim());
  else console.log('  skip  palto.local (needs avahi-resolve; on Windows open http://palto.local:' + port + '/ in a browser)');
} finally {
  await app.stop();
  if (!failures.length) rmSync(dataDir, { recursive: true, force: true });
}
if (failures.length) {
  console.error(`${failures.length} check(s) failed; data kept in ${dataDir}`);
  process.exit(1);
}
console.log('All remote checks passed.');
