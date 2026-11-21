import type { Media } from './models';
import { invoke as nativeInvoke } from '@tauri-apps/api/core';
import { DEMO_LOGO, DEMO_PROJECT_ID, demoMedia, demoProject, demoSongs } from './demoService';
export type UnlistenFn = () => void;

// Minimal event shape used by our UI.
export type TauriEvent<T> = {
  payload: T;
};

type UiOnlyBackgroundKind = 'image' | 'video' | 'shader';
type UiOnlyBackground = { kind: UiOnlyBackgroundKind; path: string };

type UiOnlyProgramState = {
  display?: 'logo' | 'black' | 'clear' | 'live';
  mode: 'manual' | 'automated' | 'external';
  active_item_id: number | null;
  next_item_id: number | null;
  active_step_index: number | null;
  active_text: string;
  background: UiOnlyBackground | null;
};

type UiOnlySessionItemKind =
  | { kind: 'text'; data: { steps: string[] } }
  | { kind: 'image'; data: { path: string } }
  | { kind: 'video'; data: { path: string; options?: Record<string, unknown> } }
  | { kind: 'audio'; data: { path: string; options?: Record<string, unknown> } }
  | { kind: 'presentation'; data: { path: string; slides?: string[] } }
  | { kind: 'capture'; data: Record<string, unknown> }
  | { kind: 'section'; data: Record<string, never> };

type UiOnlySessionItem = {
  id: number;
  title: string | null;
  kind: UiOnlySessionItemKind;
  song?: string;
  background?: string;
};

type UiOnlyProjectModel = {
  id: number;
  name: string;
  session: { items: UiOnlySessionItem[] };
};

type UiOnlyProjectHeader = { id: number; name: string; updated_at_ms: number };

type UiOnlyEngineSnapshot = {
  program_state: UiOnlyProgramState;
  version: number;
  project: UiOnlyProjectModel | null;
};

type UiOnlyEngineEvent =
  | { type: 'project_changed'; payload: { project: UiOnlyProjectModel | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: UiOnlyProgramState; version: number } }
  | { type: 'playback_changed'; payload: { playback: UiOnlyPlayback | null; revision: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

/** Browser-only mode: a simulated media session, so the transport can be laid out. */
type UiOnlyPlayback = {
  session_id: number; generation: number; item_id: number; path: string; kind: 'video' | 'audio';
  paused: boolean; suspended: boolean; phase: 'playing' | 'paused' | 'ended';
  position_ms: number; duration_ms: number | null; position_at_ms: number;
  volume: number; muted: boolean; rate_percent: number; looping: boolean;
  start_ms: number; end_ms: number | null; confidence: boolean; error: string | null;
};
/** Lengths of the demo clips; anything else plays as three minutes. */
const UI_ONLY_DURATIONS: Record<string, number> = { '/demo/countdown.mp4': 10_000, '/demo/worship-loop.mp4': 8_000, '/demo/offering-music.mp3': 20_000 };

type UiOnlyStore = {
  media?: Media[];
  songs?: Record<string, { id: number; title: string; lyrics: string }>;
  version: number;
  active_project_id: number | null;
  program_state: UiOnlyProgramState;
  projects: Record<string, UiOnlyProjectModel>;
  project_updated_at: Record<string, number>;
};

const UI_ONLY_STORAGE_KEY = 'magi.projection.ui_only.v1';

function defaultProgramState(): UiOnlyProgramState {
  return {
    mode: 'manual',
    active_item_id: null,
    next_item_id: null,
    active_step_index: null,
    active_text: '',
    background: null,
  };
}

/** The demo service with every item type, its media and songs in the library. */
function demoStore(): UiOnlyStore {
  const store: UiOnlyStore = {
    version: 1,
    active_project_id: DEMO_PROJECT_ID,
    program_state: defaultProgramState(),
    media: demoMedia,
    songs: demoSongs,
    projects: { [String(DEMO_PROJECT_ID)]: structuredClone(demoProject) as UiOnlyProjectModel },
    project_updated_at: { [String(DEMO_PROJECT_ID)]: Date.now() },
  };
  saveUiOnlyStore(store);
  return store;
}

/**
 * Browser-only mode starts every page load from the demo: anything saved by
 * an earlier visit (services, songs, output settings, column sizes) is
 * dropped, then the demo service and the demo logo are written. Call before
 * the UI first renders.
 */
export function startDemo(): void {
  if (isTauri() || typeof window === 'undefined') return;
  try {
    window.localStorage.clear();
    window.localStorage.setItem('magi.output.config', JSON.stringify({ logoPath: DEMO_LOGO }));
  } catch { /* Storage blocked: the demo still loads, unsaved. */ }
  demoStore();
}

function loadUiOnlyStore(): UiOnlyStore {
  if (typeof window === 'undefined') {
    return {
      version: 1,
      active_project_id: null,
      program_state: defaultProgramState(),
      projects: {},
      project_updated_at: {},
    };
  }

  try {
    const raw = window.localStorage.getItem(UI_ONLY_STORAGE_KEY);
    if (!raw) return demoStore();
    const parsed = JSON.parse(raw) as Partial<UiOnlyStore>;
    return {
      version: typeof parsed.version === 'number' ? parsed.version : 1,
      active_project_id: typeof parsed.active_project_id === 'number' ? parsed.active_project_id : null,
      program_state: (parsed.program_state as UiOnlyProgramState) || defaultProgramState(),
      media: parsed.media || [],
      songs: parsed.songs || {},
      projects: (parsed.projects as UiOnlyStore['projects']) || {},
      project_updated_at: (parsed.project_updated_at as UiOnlyStore['project_updated_at']) || {},
    };
  } catch {
    return {
      version: 1,
      active_project_id: null,
      program_state: defaultProgramState(),
      projects: {},
      project_updated_at: {},
    };
  }
}

function saveUiOnlyStore(store: UiOnlyStore): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(UI_ONLY_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // ignore storage failures (private mode/quota)
  }
}

type UiOnlyListener = (evt: TauriEvent<UiOnlyEngineEvent>) => void;

// Desktop-only events (output:shown and the like), raised by the fake commands.
const uiOnlyEvents = new EventTarget();
function emitUiOnly(event: string, payload: unknown): void {
  uiOnlyEvents.dispatchEvent(new CustomEvent(event, { detail: payload }));
}

type UiOnlyPair = [number, number];
type UiOnlyOutput = { monitor: number | null; shown: boolean; fullscreen: boolean; size: UiOnlyPair | null; aspect: UiOnlyPair | null };

/** Demo displays for the Screens panel: the operator's laptop, a projector and a stage monitor. */
const UI_ONLY_MONITORS = [
  { key: '0|demo-laptop', name: 'Built-in display', connector: 'Built-in', width: 1920, height: 1080, aspect: '16:9', operator: true },
  { key: '1|demo-projector', name: 'EPSON PJ', connector: 'HDMI', width: 1920, height: 1080, aspect: '16:9', operator: false },
  { key: '2|demo-monitor', name: 'Dell U2419H', connector: 'DisplayPort', width: 1920, height: 1200, aspect: '16:10', operator: false },
];

class UiOnlyEngine {
  private store: UiOnlyStore;
  private listeners: Set<UiOnlyListener> = new Set();
  private screens: Record<string, UiOnlyOutput> = {
    projection: { monitor: 1, shown: false, fullscreen: true, size: null, aspect: null },
    confidence: { monitor: 2, shown: false, fullscreen: true, size: null, aspect: null },
  };

  constructor() {
    this.store = loadUiOnlyStore();
    window.addEventListener('storage', event => {
      if (event.key === UI_ONLY_STORAGE_KEY) {
        this.store = loadUiOnlyStore();
        for (const update of this.syncEvents()) this.emit(update);
      }
    });
  }

  private bumpVersion(): number {
    this.store.version += 1;
    return this.store.version;
  }

  private activeProject(): UiOnlyProjectModel | null {
    if (this.store.active_project_id == null) return null;
    return this.store.projects[String(this.store.active_project_id)] || null;
  }

  private snapshot(): UiOnlyEngineSnapshot & { playback: UiOnlyPlayback | null; playback_revision: number } {
    return structuredClone({
      program_state: this.store.program_state,
      version: this.store.version,
      project: this.activeProject(),
      playback: this.playback,
      playback_revision: this.playbackRevision,
    });
  }

  private playback: UiOnlyPlayback | null = null;
  private playbackRevision = 0;
  private autoplay = true;
  private nextSession = 1;
  private publishPlayback(): void {
    this.playbackRevision++;
    this.emit({ type: 'playback_changed', payload: { playback: structuredClone(this.playback), revision: this.playbackRevision } });
  }
  /** Where the clip is now, from its last position and the clock. */
  private position(p: UiOnlyPlayback) {
    return p.paused ? p.position_ms : p.position_ms + (Date.now() - p.position_at_ms) * p.rate_percent / 100;
  }
  /** Opens a session for a cued video or audio item, as the engine does. */
  private syncPlayback(item: UiOnlySessionItem | null): void {
    const media = item && (item.kind.kind === 'video' || item.kind.kind === 'audio') ? item : null;
    if (this.playback && media && this.playback.item_id === media.id) return;
    this.playback = null;
    if (media && (media.kind.kind === 'video' || media.kind.kind === 'audio')) {
      const options = (media.kind.data as { options?: Record<string, unknown> }).options ?? {};
      const start = Number(options.start_ms ?? 0);
      this.playback = {
        session_id: this.nextSession++, generation: 0, item_id: media.id, path: media.kind.data.path, kind: media.kind.kind,
        paused: !this.autoplay, suspended: false, phase: this.autoplay ? 'playing' : 'paused',
        position_ms: start, duration_ms: UI_ONLY_DURATIONS[media.kind.data.path] ?? 180_000, position_at_ms: Date.now(),
        volume: Number(options.volume ?? 100), muted: false, rate_percent: Number(options.rate_percent ?? 100), looping: Boolean(options.looping),
        start_ms: start, end_ms: (options.end_ms as number | null) ?? null, confidence: Boolean(options.confidence), error: null,
      };
    }
    this.publishPlayback();
  }
  private mediaAction(action: { type: string; payload?: Record<string, number | boolean> }): void {
    const p = this.playback;
    if (!p) return;
    const now = Date.now();
    switch (action.type) {
      case 'play': p.position_ms = this.position(p); p.paused = false; p.phase = 'playing'; break;
      case 'pause': p.position_ms = this.position(p); p.paused = true; p.phase = 'paused'; break;
      case 'stop': p.position_ms = p.start_ms; p.paused = true; p.phase = 'paused'; break;
      case 'seek': p.position_ms = Number(action.payload?.position_ms ?? 0); break;
      case 'set_volume': p.volume = Number(action.payload?.volume ?? 100); break;
      case 'set_muted': p.muted = Boolean(action.payload?.muted); break;
    }
    p.position_at_ms = now;
    this.publishPlayback();
  }

  private emit(event: UiOnlyEngineEvent): void {
    for (const listener of this.listeners) {
      try {
        listener({ payload: event });
      } catch {
        // ignore listener errors
      }
    }
  }

  private syncEvents(): UiOnlyEngineEvent[] {
    const snapshot = this.snapshot();
    return [
      { type: 'project_changed', payload: { project: snapshot.project, version: snapshot.version } },
      { type: 'program_changed', payload: { program_state: snapshot.program_state, version: snapshot.version } },
    ];
  }

  private persistAndBroadcast(): void {
    saveUiOnlyStore(this.store);
    for (const event of this.syncEvents()) this.emit(event);
  }

  listen(handler: UiOnlyListener): UnlistenFn {
    this.listeners.add(handler);
    // Immediately sync so the UI can render reliably.
    for (const event of this.syncEvents()) handler({ payload: event });
    return () => this.listeners.delete(handler);
  }

  private recomputeNextItem(): void {
    const project = this.activeProject();
    if (!project) {
      this.store.program_state.next_item_id = null;
      return;
    }
    const items = project.session.items || [];
    const activeId = this.store.program_state.active_item_id;
    const idx = activeId == null ? -1 : items.findIndex((it) => it.id === activeId);
    const next = idx >= 0 ? items.slice(idx + 1).find(it => it.kind.kind !== 'section') ?? null : null;
    this.store.program_state.next_item_id = next ? next.id : null;
  }

  private setActiveItem(itemId: number | null): void {
    // Section headings never go on air: a cue on one lands on the item after it.
    const listed = this.activeProject()?.session.items ?? [];
    let index = itemId == null ? -1 : listed.findIndex(it => it.id === itemId);
    while (index >= 0 && index < listed.length && listed[index].kind.kind === 'section') index++;
    if (index >= listed.length) index = -1;
    if (index >= 0) itemId = listed[index].id;
    this.store.program_state.active_item_id = itemId;
    this.syncPlayback(index >= 0 ? listed[index] : null);
    this.recomputeNextItem();

    const project = this.activeProject();
    const item =
      project && itemId != null ? project.session.items.find((it) => it.id === itemId) || null : null;

    if (item?.kind.kind === 'text') {
      if (item.background) this.store.program_state.background = { kind: /\.(mp4|mov|webm|mkv)$/i.test(item.background) ? 'video' : 'image', path: item.background };
      const steps = item.kind.data.steps || [];
      const idx = steps.length > 0 ? 0 : null;
      this.store.program_state.active_step_index = idx;
      this.store.program_state.active_text = idx != null ? steps[idx] || '' : '';
    } else {
      if (item?.kind.kind === 'image' || item?.kind.kind === 'video') this.store.program_state.background = { kind: item.kind.kind, path: item.kind.data.path };
      if (item?.kind.kind === 'presentation') this.store.program_state.background = { kind: 'image', path: item.kind.data.slides?.[0] || item.kind.data.path };
      this.store.program_state.active_step_index = item?.kind.kind === 'presentation' ? 0 : null;
      this.store.program_state.active_text = '';
    }
  }

  private displays() {
    return { monitors: UI_ONLY_MONITORS, shown: Object.values(this.screens).some(screen => screen.shown), outputs: structuredClone(this.screens) };
  }

  async invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
    const now = Date.now();

    try {
      switch (command) {
        case 'list_media': return (this.store.media || []) as T;
        case 'create_media': {
          const media = { id: now, title: String(args?.title || ''), kind: args?.kind, path: String(args?.path || '') } as Media;
          this.store.media = [...(this.store.media || []), media]; saveUiOnlyStore(this.store); return media as T;
        }
        case 'list_songs': return Object.values(this.store.songs || {}).map(({ id, title }) => ({ id, title })) as T;
        case 'get_song': {
          const song = this.store.songs?.[String(args?.id)];
          if (!song) throw new Error('Song not found');
          return song as T;
        }
        case 'update_song': {
          const song = args?.song as { id: number; title: string; lyrics: string };
          if (!song || !this.store.songs?.[song.id]) throw new Error('Song not found');
          this.store.songs[song.id] = song; saveUiOnlyStore(this.store); return undefined as T;
        }
        case 'create_song': {
          const song = { id: now, title: String(args?.title || ''), lyrics: String(args?.lyrics || '') };
          this.store.songs = { ...this.store.songs, [song.id]: song };
          saveUiOnlyStore(this.store);
          return song as T;
        }
        case 'engine_command': {
          const cmd = args?.command as { type: string; payload?: { action?: { type: string; payload?: Record<string, number | boolean> } } } | undefined;
          if (cmd?.type === 'media' && cmd.payload?.action) this.mediaAction(cmd.payload.action);
          return undefined as T;
        }
        case 'get_media_autoplay': return this.autoplay as T;
        case 'set_media_autoplay': this.autoplay = Boolean(args?.enabled); return undefined as T;
        case 'set_display': {
          const display = args?.display as UiOnlyProgramState['display'];
          if (!display || !['logo', 'black', 'clear', 'live'].includes(display)) throw new Error('Invalid display mode');
          this.store.program_state.display = display;
          this.bumpVersion(); this.persistAndBroadcast(); return undefined as T;
        }

        case 'get_engine_snapshot': {
          return this.snapshot() as T;
        }

        case 'list_projects': {
          const headers: UiOnlyProjectHeader[] = Object.values(this.store.projects).map((p) => ({
            id: p.id,
            name: p.name,
            updated_at_ms: this.store.project_updated_at[String(p.id)] || now,
          }));
          headers.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
          return headers as T;
        }

        case 'get_project': {
          const project = this.store.projects[String(args?.projectId)];
          if (!project) throw new Error('Service not found');
          return structuredClone(project) as T;
        }
        case 'delete_project': {
          const id = Number(args?.projectId);
          if (this.store.active_project_id === id) throw new Error('This service is loaded. Load another service before deleting it.');
          delete this.store.projects[String(id)]; delete this.store.project_updated_at[String(id)];
          saveUiOnlyStore(this.store); return undefined as T;
        }
        case 'delete_song': {
          if (this.store.songs) delete this.store.songs[String(args?.id)];
          saveUiOnlyStore(this.store); return undefined as T;
        }
        case 'delete_media': {
          this.store.media = (this.store.media || []).filter(m => m.id !== Number(args?.id));
          saveUiOnlyStore(this.store); return undefined as T;
        }
        case 'rename_media': {
          const title = String(args?.title ?? '').trim();
          if (!title) throw new Error('Enter a title.');
          this.store.media = (this.store.media || []).map(m => m.id === Number(args?.id) ? { ...m, title } : m);
          saveUiOnlyStore(this.store); return undefined as T;
        }
        // Browser-only mode cannot read files from disk.
        case 'import_media_files': return [] as T;

        case 'create_project': {
          const name = String(args?.name ?? 'Untitled');
          const id = now + Math.floor(Math.random() * 1000);
          const project: UiOnlyProjectModel = { id, name, session: { items: [] } };
          this.store.projects[String(id)] = project;
          this.store.project_updated_at[String(id)] = now;
          this.bumpVersion();
          saveUiOnlyStore(this.store);
          return project as T;
        }

        case 'save_project': {
          const project = args?.project as UiOnlyProjectModel | undefined;
          if (!project || typeof project.id !== 'number') {
            throw new Error('save_project requires { project }');
          }
          this.store.projects[String(project.id)] = project;
          this.store.project_updated_at[String(project.id)] = now;
          this.bumpVersion();
          saveUiOnlyStore(this.store);
          if (this.store.active_project_id === project.id) {
            const step = this.store.program_state.active_step_index;
            this.setActiveItem(this.store.program_state.active_item_id);
            const item = project.session.items.find(item => item.id === this.store.program_state.active_item_id);
            if (item?.kind.kind === 'presentation' && step != null) { const paths = item.kind.data.slides || [item.kind.data.path]; const index = Math.min(step, paths.length - 1); this.store.program_state.active_step_index = index; this.store.program_state.background = { kind: 'image', path: paths[index] }; }
            if (item?.kind.kind === 'text' && step != null && item.kind.data.steps.length) {
              const index = Math.min(step, item.kind.data.steps.length - 1);
              this.store.program_state.active_step_index = index;
              this.store.program_state.active_text = item.kind.data.steps[index];
            }
            this.persistAndBroadcast();
          }
          return undefined as T;
        }

        case 'load_project': {
          const projectId = Number(args?.projectId);
          if (!Number.isFinite(projectId)) throw new Error('load_project requires { projectId }');
          this.store.active_project_id = projectId;
          const project = this.activeProject();
          if (project && project.session.items.length > 0) {
            this.setActiveItem(project.session.items[0].id);
          } else {
            this.setActiveItem(null);
          }
          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
        }

        case 'cue': {
          this.setActiveItem(Number(args?.itemId));
          const item = this.activeProject()?.session.items.find(item => item.id === Number(args?.itemId));
          const index = Number(args?.index ?? 0);
          if (item?.kind.kind === 'text' && index < item.kind.data.steps.length) { this.store.program_state.active_step_index = index; this.store.program_state.active_text = item.kind.data.steps[index]; }
          if (item?.kind.kind === 'presentation') { const paths = item.kind.data.slides || [item.kind.data.path]; if (paths[index]) { this.store.program_state.active_step_index = index; this.store.program_state.background = { kind: 'image', path: paths[index] }; } }
          this.bumpVersion(); this.persistAndBroadcast(); return undefined as T;
        }
        case 'go_to_item': {
          const itemId = Number(args?.itemId);
          if (!Number.isFinite(itemId)) throw new Error('go_to_item requires { itemId }');
          this.setActiveItem(itemId);
          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
        }

        case 'go_to_step': {
          const index = Number(args?.index);
          if (!Number.isFinite(index)) throw new Error('go_to_step requires { index }');
          const project = this.activeProject();
          const activeId = this.store.program_state.active_item_id;
          const item =
            project && activeId != null ? project.session.items.find((it) => it.id === activeId) || null : null;

          if (item?.kind.kind === 'presentation') {
            const images = item.kind.data.slides || [item.kind.data.path];
            const indexClamped = Math.max(0, Math.min(index, images.length - 1));
            this.store.program_state.active_step_index = indexClamped;
            this.store.program_state.background = { kind: 'image', path: images[indexClamped] };
            this.bumpVersion(); this.persistAndBroadcast();
          }
          if (item?.kind.kind === 'text') {
            const steps = item.kind.data.steps || [];
            const clamped = Math.max(0, Math.min(index, Math.max(0, steps.length - 1)));
            this.store.program_state.active_step_index = steps.length > 0 ? clamped : null;
            this.store.program_state.active_text = steps.length > 0 ? steps[clamped] || '' : '';
            this.bumpVersion();
            this.persistAndBroadcast();
          }
          return undefined as T;
        }

        case 'nav_next':
        case 'nav_prev': {
          const dir = command === 'nav_next' ? 1 : -1;
          const project = this.activeProject();
          if (!project) return undefined as T;

          const items = project.session.items || [];
          if (items.length === 0) return undefined as T;

          const activeId = this.store.program_state.active_item_id;
          if (activeId == null) {
            this.setActiveItem(items[0].id);
          } else {
            const activeIndex = items.findIndex((it) => it.id === activeId);
            const item = activeIndex >= 0 ? items[activeIndex] : null;
            if (item?.kind.kind === 'text') {
              const steps = item.kind.data.steps || [];
              if (steps.length > 0) {
                const current = this.store.program_state.active_step_index ?? 0;
                const nextStep = Math.max(0, Math.min(current + dir, steps.length - 1));
                this.store.program_state.active_step_index = nextStep;
                this.store.program_state.active_text = steps[nextStep] || '';
              }
            } else if (activeIndex >= 0) {
              // Neighbouring items skip section headings.
              let target = activeIndex + dir;
              while (target >= 0 && target < items.length && items[target].kind.kind === 'section') target += dir;
              if (target >= 0 && target < items.length) this.setActiveItem(items[target].id);
            }
          }

          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
        }

        case 'get_output_displays': return this.displays() as T;
        case 'set_output_display': {
          const output = String(args?.output);
          if (!(output in this.screens)) throw new Error('Unknown output');
          const monitor = Number(args?.monitor);
          if (UI_ONLY_MONITORS[monitor]?.key !== args?.key) throw new Error('The connected displays changed. Choose the display again.');
          this.screens[output] = {
            monitor,
            shown: this.screens[output].shown,
            fullscreen: Boolean(args?.fullscreen),
            size: (args?.size as UiOnlyPair | null) ?? null,
            aspect: (args?.aspect as UiOnlyPair | null) ?? null,
          };
          return this.displays() as T;
        }
        case 'set_output_shown': {
          const output = String(args?.output);
          if (!(output in this.screens)) throw new Error('Unknown output');
          this.screens[output] = { ...this.screens[output], shown: Boolean(args?.shown) };
          emitUiOnly('output:shown', Object.values(this.screens).some(screen => screen.shown));
          return this.displays() as T;
        }

        // Media playback needs the desktop engine; the browser-only mode has none.

        default: {
          console.debug('[ui-only] unhandled invoke', command, args);
          return undefined as T;
        }
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.warn('[ui-only] invoke error', command, message);
      this.emit({
        type: 'error',
        payload: { source: command, message, recoverable: true },
      });
      throw e;
    }
  }
}

let uiOnlyEngineSingleton: UiOnlyEngine | null = null;

function uiOnlyEngine(): UiOnlyEngine {
  if (!uiOnlyEngineSingleton) uiOnlyEngineSingleton = new UiOnlyEngine();
  return uiOnlyEngineSingleton;
}

export function isTauri(): boolean {
  if (typeof window === 'undefined') return false;
  const w = window as unknown as Record<string, unknown>;
  // Tauri injects globals; exact shape differs a bit between versions.
  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauri()) {
    return uiOnlyEngine().invoke<T>(command, args);
  }

  return nativeInvoke<T>(command, args);
}

/** Sends `bytes` as the raw request body (desktop app only), with string headers. */
export async function invokeBytes<T = unknown>(command: string, bytes: Uint8Array, headers: Record<string, string> = {}): Promise<T> {
  return nativeInvoke<T>(command, bytes, { headers });
}

export async function listen<T>(
  event: string,
  handler: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  if (!isTauri()) {
    if (event !== 'engine:event') {
      const relay = (e: Event) => handler({ payload: (e as CustomEvent<T>).detail });
      uiOnlyEvents.addEventListener(event, relay);
      return () => uiOnlyEvents.removeEventListener(event, relay);
    }

    return uiOnlyEngine().listen(handler as unknown as UiOnlyListener);
  }

  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(event, handler);
}
