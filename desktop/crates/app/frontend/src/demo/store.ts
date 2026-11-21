import type { UiOnlyStore, UiOnlyProgramState, UiOnlyProjectModel } from './types';
import { isTauri } from '../api/tauri';
import { DEMO_LOGO, DEMO_PROJECT_ID, demoMedia, demoProject, demoSongs } from './service';

export const UI_ONLY_STORAGE_KEY = 'magi.projection.ui_only.v1';

export function defaultProgramState(): UiOnlyProgramState {
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
export function demoStore(): UiOnlyStore {
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

export function loadUiOnlyStore(): UiOnlyStore {
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

export function saveUiOnlyStore(store: UiOnlyStore): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(UI_ONLY_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // ignore storage failures (private mode/quota)
  }
}
