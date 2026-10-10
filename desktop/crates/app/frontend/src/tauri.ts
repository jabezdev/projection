import type { Media } from './models';
import { invoke as nativeInvoke } from '@tauri-apps/api/core';
export type UnlistenFn = () => void;

// Minimal event shape used by our UI.
export type TauriEvent<T> = {
  payload: T;
};

type UiOnlyBackgroundKind = 'image' | 'video';
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
  | { kind: 'video'; data: { path: string } }
  | { kind: 'audio'; data: { path: string; options?: Record<string, unknown> } }
  | { kind: 'presentation'; data: { path: string; slides?: string[] } };

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
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

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
    if (!raw) throw new Error('missing');
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

class UiOnlyEngine {
  private store: UiOnlyStore;
  private listeners: Set<UiOnlyListener> = new Set();

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

  private snapshot(): UiOnlyEngineSnapshot {
    return structuredClone({
      program_state: this.store.program_state,
      version: this.store.version,
      project: this.activeProject(),
    });
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
    const next = idx >= 0 && idx + 1 < items.length ? items[idx + 1] : null;
    this.store.program_state.next_item_id = next ? next.id : null;
  }

  private setActiveItem(itemId: number | null): void {
    this.store.program_state.active_item_id = itemId;
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
              const nextItemIndex = Math.max(0, Math.min(activeIndex + dir, items.length - 1));
              this.setActiveItem(items[nextItemIndex].id);
            }
          }

          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
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

export async function listen<T>(
  event: string,
  handler: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  if (!isTauri()) {
    if (event !== 'engine:event') {
      console.debug('[ui-only] listen (ignored)', event);
      return () => {
        // no-op
      };
    }

    return uiOnlyEngine().listen(handler as unknown as UiOnlyListener);
  }

  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(event, handler);
}
