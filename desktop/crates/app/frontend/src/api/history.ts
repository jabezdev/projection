import { invoke } from './tauri';
import type { Service, Song } from './models';

export type History = { undo?: string; redo?: string };
export type Stepped = { service?: Service; song?: Song; history: History };
export type RecordKind = 'service' | 'song';

export const editHistory = (id: string) => invoke<History>('edit_history', { id });
export const stepEdit = (record: RecordKind, id: string, redo: boolean) => invoke<Stepped>(redo ? 'redo_edit' : 'undo_edit', { record, id });

export function historyKey(e: { key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; target: EventTarget | null }): 'undo' | 'redo' | null {
  if (!(e.ctrlKey || e.metaKey) || (e.target as HTMLElement | null)?.closest?.('input, textarea, [contenteditable="true"]')) return null;
  const key = e.key.toLowerCase();
  if (key === 'y' || (key === 'z' && e.shiftKey)) return 'redo';
  return key === 'z' ? 'undo' : null;
}
