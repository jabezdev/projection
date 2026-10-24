// Shared helpers for the end-to-end and soak scripts: start PALTO against a
// scratch data folder, seed services into its database, and drive the engine
// over the same WebSocket protocol remote clients use.
//
// Needs Bun (https://bun.sh); no packages. The app must already be built.

import { Database } from 'bun:sqlite';
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

export type Json = Record<string, unknown>;
export type EngineEvent = { type: string; payload: Json };

const desktop = resolve(import.meta.dir, '../..');

/** The newest local build of the app: release first, then debug. */
export function defaultAppPath(): string {
  const exe = process.platform === 'win32' ? 'app.exe' : 'app';
  for (const profile of ['release', 'debug']) {
    const path = join(desktop, 'target', profile, exe);
    if (existsSync(path)) return path;
  }
  throw new Error('No built app found. Build it first (cargo build -p app), or pass --app <path>.');
}

/** Minimal `--name value` / `--flag` parsing. */
export function parseArgs(argv = process.argv.slice(2)): Record<string, string | true> {
  const args: Record<string, string | true> = {};
  for (let i = 0; i < argv.length; i++) {
    const key = argv[i];
    if (!key.startsWith('--')) throw new Error(`Unexpected argument ${key}`);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) args[key.slice(2)] = true;
    else { args[key.slice(2)] = next; i++; }
  }
  return args;
}

export function scratchDir(prefix: string): string {
  return mkdtempSync(join(tmpdir(), `palto-${prefix}-`));
}

export class App {
  private constructor(readonly dataDir: string, readonly port: number, readonly process: ReturnType<typeof Bun.spawn>) {}

  /** Starts the app with `dataDir` as its data folder and the WebSocket on `port`. */
  static async start(appPath: string, dataDir: string, port: number, env: Record<string, string> = {}): Promise<App> {
    mkdirSync(dataDir, { recursive: true });
    const config = join(dataDir, 'app-config.json');
    const saved = existsSync(config) ? JSON.parse(readFileSync(config, 'utf8')) : {};
    writeFileSync(config, JSON.stringify({ projection_size: null, ...saved, ws_bind_host: '127.0.0.1', ws_port: port }));
    const child = Bun.spawn([appPath], {
      env: { ...process.env, ...env, PALTO_DATA_DIR: dataDir },
      stdout: 'ignore',
      stderr: 'ignore',
    });
    const app = new App(dataDir, port, child);
    try {
      await app.waitForSocket();
    } catch (e) {
      child.kill();
      throw e;
    }
    return app;
  }

  get pid(): number { return this.process.pid; }
  get logPath(): string { return join(this.dataDir, 'logs', 'palto.log'); }

  private async waitForSocket(timeoutMs = 60_000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (this.process.exitCode !== null) throw new Error(`The app exited during startup (code ${this.process.exitCode}); see ${this.logPath}`);
      if (existsSync(this.logPath) && readFileSync(this.logPath, 'utf8').includes('Network server failed')) throw new Error(`The app could not open its WebSocket on port ${this.port}; pass another --port`);
      try { (await Remote.connect(this.port)).close(); return; } catch { await Bun.sleep(500); }
    }
    throw new Error(`The app's WebSocket did not open on port ${this.port}`);
  }

  /** Adds a service to the app's library and returns its id. */
  seedService(name: string, items: Json[]): number {
    const db = new Database(join(this.dataDir, 'app.db'));
    try {
      const now = Date.now();
      const id = Number(db.query('INSERT INTO projects(name, created_at_ms, updated_at_ms, model_json) VALUES (?, ?, ?, NULL)').run(name, now, now).lastInsertRowid);
      db.query('UPDATE projects SET model_json = ? WHERE id = ?').run(JSON.stringify({ id, name, session: { items } }), id);
      return id;
    } finally {
      db.close();
    }
  }

  async stop() {
    this.process.kill();
    await this.process.exited;
  }
}

/** A WebSocket remote: sends engine commands, records every event. */
export class Remote {
  readonly events: EngineEvent[] = [];
  private waiters: { test: (e: EngineEvent) => boolean; done: (e: EngineEvent) => void }[] = [];

  private constructor(private socket: WebSocket) {
    socket.addEventListener('message', message => {
      const event = JSON.parse(String(message.data)) as EngineEvent;
      this.events.push(event);
      this.waiters = this.waiters.filter(waiter => { if (!waiter.test(event)) return true; waiter.done(event); return false; });
    });
  }

  static connect(port: number): Promise<Remote> {
    return new Promise((resolve, reject) => {
      const socket = new WebSocket(`ws://127.0.0.1:${port}`);
      const remote = new Remote(socket);
      socket.addEventListener('open', () => resolve(remote), { once: true });
      socket.addEventListener('error', () => reject(new Error('WebSocket connection failed')), { once: true });
    });
  }

  send(type: string, payload?: Json) {
    this.socket.send(JSON.stringify(payload === undefined ? { type } : { type, payload }));
  }

  /** Waits for the first event from now on that passes `test`. */
  next(test: (event: EngineEvent) => boolean, what: string, timeoutMs = 10_000): Promise<EngineEvent> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Timed out waiting for ${what}`)), timeoutMs);
      this.waiters.push({ test, done: event => { clearTimeout(timer); resolve(event); } });
    });
  }

  /** Sends a command, then waits for an event that passes `test`. */
  async command(type: string, payload: Json | undefined, test: (event: EngineEvent) => boolean, what: string, timeoutMs?: number) {
    const waiting = this.next(test, what, timeoutMs);
    this.send(type, payload);
    return waiting;
  }

  close() { this.socket.close(); }
}

export const program = (event: EngineEvent) => event.type === 'program_changed' ? event.payload.program_state as Json : null;
export const playback = (event: EngineEvent) => event.type === 'playback_changed' ? event.payload.playback as Json | null : undefined;
export const project = (event: EngineEvent) => event.type === 'project_changed' ? event.payload.project as Json | null : undefined;

export const videoOptions = (overrides: Json = {}) => ({ confidence: false, looping: false, start_ms: 0, end_ms: null, volume: 100, rate_percent: 100, ...overrides });

/** Resident memory of `pid` in bytes, or null when it cannot be read. */
export async function residentBytes(pid: number): Promise<number | null> {
  try {
    if (process.platform === 'linux') {
      const match = readFileSync(`/proc/${pid}/status`, 'utf8').match(/VmRSS:\s+(\d+) kB/);
      return match ? Number(match[1]) * 1024 : null;
    }
    const command = process.platform === 'win32'
      ? ['powershell', '-NoProfile', '-Command', `(Get-Process -Id ${pid}).WorkingSet64`]
      : ['ps', '-o', 'rss=', '-p', String(pid)];
    const text = (await new Response(Bun.spawn(command, { stdout: 'pipe' }).stdout).text()).trim();
    const value = Number(text);
    return Number.isFinite(value) && text ? (process.platform === 'win32' ? value : value * 1024) : null;
  } catch {
    return null;
  }
}

/** CPU time `pid` has used so far (all threads), in seconds, or null. */
export async function cpuSeconds(pid: number): Promise<number | null> {
  try {
    if (process.platform === 'linux') {
      // Fields after the parenthesized name: utime and stime are 14th and 15th overall.
      const fields = readFileSync(`/proc/${pid}/stat`, 'utf8').split(') ')[1].split(' ');
      return (Number(fields[11]) + Number(fields[12])) / 100;
    }
    const command = process.platform === 'win32'
      ? ['powershell', '-NoProfile', '-Command', `(Get-Process -Id ${pid}).TotalProcessorTime.TotalSeconds`]
      : ['ps', '-o', 'time=', '-p', String(pid)];
    const text = (await new Response(Bun.spawn(command, { stdout: 'pipe' }).stdout).text()).trim();
    if (process.platform === 'win32') return Number.isFinite(Number(text)) && text ? Number(text) : null;
    const parts = text.split(/[:-]/).map(Number);
    return parts.reduce((total, part) => total * 60 + part, 0);
  } catch {
    return null;
  }
}
