import type { UiOnlyStore, UiOnlyProgramState } from './types';
import { isTauri } from '../api/tauri';
import { DEMO_LOGO, demoMedia, demoService, demoSongs } from './service';

export const UI_ONLY_STORAGE_KEY = 'magi.projection.ui_only.v2';

export function defaultProgramState(): UiOnlyProgramState {
  return { mode: 'manual', active_item_id: null, next_item_id: null, active_step_index: null, active_text: '', background: null };
}

function empty(): UiOnlyStore {
  return { version: 1, active_service_id: null, program_state: defaultProgramState(), media: [], songs: {}, versions: {}, services: {}, frozen: [] };
}

export function demoStore(): UiOnlyStore {
  const service = demoService();
  const store: UiOnlyStore = {
    ...empty(),
    active_service_id: service.id,
    media: structuredClone(demoMedia),
    songs: Object.fromEntries(demoSongs.map(song => [song.id, structuredClone(song)])),
    services: { [service.id]: service },
  };
  saveUiOnlyStore(store);
  return store;
}

export function startDemo(): void {
  if (isTauri() || typeof window === 'undefined') return;
  try {
    window.localStorage.clear();
    window.localStorage.setItem('magi.output.config', JSON.stringify({ logoPath: DEMO_LOGO }));
  } catch {  }
  demoStore();
}

export function loadUiOnlyStore(): UiOnlyStore {
  if (typeof window === 'undefined') return empty();
  try {
    const raw = window.localStorage.getItem(UI_ONLY_STORAGE_KEY);
    if (!raw) return demoStore();
    return { ...empty(), ...(JSON.parse(raw) as Partial<UiOnlyStore>) };
  } catch {
    return empty();
  }
}

export function saveUiOnlyStore(store: UiOnlyStore): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(UI_ONLY_STORAGE_KEY, JSON.stringify(store));
  } catch {
  }
}
