import type { UiOnlyStore, UiOnlyEngineEvent, UiOnlyEngineSnapshot, UiOnlyListener, UiOnlyOutput, UiOnlyPlayback, UiOnlyProjectModel, UiOnlySessionItem } from './types';
import { UI_ONLY_STORAGE_KEY, loadUiOnlyStore, saveUiOnlyStore } from './store';
import { runDemoCommand } from './commands';
import type { UnlistenFn } from '../api/tauri';

/** Lengths of the demo clips; anything else plays as three minutes. */
export const UI_ONLY_DURATIONS: Record<string, number> = { '/demo/countdown.mp4': 10_000, '/demo/worship-loop.mp4': 8_000, '/demo/offering-music.mp3': 20_000 };

/** Demo displays for the Screens panel: the operator's laptop, a projector and a stage monitor. */
export const UI_ONLY_MONITORS = [
  { key: '0|demo-laptop', name: 'Built-in display', connector: 'Built-in', width: 1920, height: 1080, aspect: '16:9', operator: true },
  { key: '1|demo-projector', name: 'EPSON PJ', connector: 'HDMI', width: 1920, height: 1080, aspect: '16:9', operator: false },
  { key: '2|demo-monitor', name: 'Dell U2419H', connector: 'DisplayPort', width: 1920, height: 1200, aspect: '16:10', operator: false },
];


export class UiOnlyEngine {
  store: UiOnlyStore;
  listeners: Set<UiOnlyListener> = new Set();
  screens: Record<string, UiOnlyOutput> = {
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

  bumpVersion(): number {
    this.store.version += 1;
    return this.store.version;
  }

  activeProject(): UiOnlyProjectModel | null {
    if (this.store.active_project_id == null) return null;
    return this.store.projects[String(this.store.active_project_id)] || null;
  }

  snapshot(): UiOnlyEngineSnapshot & { playback: UiOnlyPlayback | null; playback_revision: number } {
    return structuredClone({
      program_state: this.store.program_state,
      version: this.store.version,
      project: this.activeProject(),
      playback: this.playback,
      playback_revision: this.playbackRevision,
    });
  }

  playback: UiOnlyPlayback | null = null;
  playbackRevision = 0;
  autoplay = true;
  nextSession = 1;
  publishPlayback(): void {
    this.playbackRevision++;
    this.emit({ type: 'playback_changed', payload: { playback: structuredClone(this.playback), revision: this.playbackRevision } });
  }
  /** Where the clip is now, from its last position and the clock. */
  position(p: UiOnlyPlayback) {
    return p.paused ? p.position_ms : p.position_ms + (Date.now() - p.position_at_ms) * p.rate_percent / 100;
  }
  /** Opens a session for a cued video or audio item, as the engine does. */
  syncPlayback(item: UiOnlySessionItem | null): void {
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
  mediaAction(action: { type: string; payload?: Record<string, number | boolean> }): void {
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

  emit(event: UiOnlyEngineEvent): void {
    for (const listener of this.listeners) {
      try {
        listener({ payload: event });
      } catch {
        // ignore listener errors
      }
    }
  }

  syncEvents(): UiOnlyEngineEvent[] {
    const snapshot = this.snapshot();
    return [
      { type: 'project_changed', payload: { project: snapshot.project, version: snapshot.version } },
      { type: 'program_changed', payload: { program_state: snapshot.program_state, version: snapshot.version } },
    ];
  }

  persistAndBroadcast(): void {
    saveUiOnlyStore(this.store);
    for (const event of this.syncEvents()) this.emit(event);
  }

  listen(handler: UiOnlyListener): UnlistenFn {
    this.listeners.add(handler);
    // Immediately sync so the UI can render reliably.
    for (const event of this.syncEvents()) handler({ payload: event });
    return () => this.listeners.delete(handler);
  }

  recomputeNextItem(): void {
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

  setActiveItem(itemId: number | null): void {
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

  displays() {
    return { monitors: UI_ONLY_MONITORS, shown: Object.values(this.screens).some(screen => screen.shown), outputs: structuredClone(this.screens) };
  }
  invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
    return runDemoCommand<T>(this, command, args);
  }
}

export let uiOnlyEngineSingleton: UiOnlyEngine | null = null;

export function uiOnlyEngine(): UiOnlyEngine {
  if (!uiOnlyEngineSingleton) uiOnlyEngineSingleton = new UiOnlyEngine();
  return uiOnlyEngineSingleton;
}
