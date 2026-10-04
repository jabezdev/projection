export type UnlistenFn = () => void;

// Minimal event shape used by our UI.
export type TauriEvent<T> = {
  payload: T;
};

type UiOnlyTransport = 'stopped' | 'playing' | 'paused';
type UiOnlyBackgroundKind = 'image' | 'video';
type UiOnlyBackground = { kind: UiOnlyBackgroundKind; path: string };

type UiOnlyProgramState = {
  mode: 'manual' | 'automated' | 'external';
  transport: UiOnlyTransport;
  time_ms: number;
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
  | { kind: 'presentation'; data: { path: string } };

type UiOnlySessionItem = {
  id: number;
  title: string | null;
  kind: UiOnlySessionItemKind;
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
    transport: 'stopped',
    time_ms: 0,
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
    return {
      program_state: this.store.program_state,
      version: this.store.version,
      project: this.activeProject(),
    };
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
      const steps = item.kind.data.steps || [];
      const idx = steps.length > 0 ? 0 : null;
      this.store.program_state.active_step_index = idx;
      this.store.program_state.active_text = idx != null ? steps[idx] || '' : '';
    } else {
      this.store.program_state.active_step_index = null;
      this.store.program_state.active_text = '';
    }
  }

  async invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
    const now = Date.now();

    try {
      switch (command) {
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
            this.persistAndBroadcast();
          }
          return undefined as T;
        }

        case 'load_project': {
          const projectId = Number(args?.project_id);
          if (!Number.isFinite(projectId)) throw new Error('load_project requires { project_id }');
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

        case 'go_to_item': {
          const itemId = Number(args?.item_id);
          if (!Number.isFinite(itemId)) throw new Error('go_to_item requires { item_id }');
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

        case 'play': {
          this.store.program_state.transport = 'playing';
          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
        }

        case 'pause': {
          this.store.program_state.transport = 'paused';
          this.bumpVersion();
          this.persistAndBroadcast();
          return undefined as T;
        }

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
      return undefined as T;
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

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
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
