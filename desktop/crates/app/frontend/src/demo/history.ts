import * as edits from '../../../../../../web/shared/src/edits';
import { newId, type Service, type Song } from '../api/models';
import type { History } from '../api/history';

const DEPTH = 100;
const MERGE_WINDOW_MS = 1000;
type Stacks = { undo: edits.EditStep[]; redo: edits.EditStep[] };
const stacks = new Map<string, Stacks>();

const stacksOf = (id: string) => { let s = stacks.get(id); if (!s) stacks.set(id, s = { undo: [], redo: [] }); return s; };

export function historyOf(id: string): History {
  const s = stacks.get(id);
  return { undo: s?.undo.at(-1)?.label, redo: s?.redo.at(-1)?.label };
}

export const dropHistory = (id: string) => void stacks.delete(id);

export function recordService(before: Service, after: Service, itemName: (id: string) => string) {
  record({ kind: 'service', id: after.id, ops: edits.diffService(before, after) }, change => edits.describe(change, itemName));
}

export function recordSong(before: Song, after: Song) {
  record({ kind: 'song', id: after.id, ops: edits.diffSong(before, after) }, change => edits.describe(change, id => id));
}

function record(change: edits.Change, describe: (change: edits.Change) => string) {
  if (edits.isEmpty(change)) return;
  const now = Date.now();
  const s = stacksOf(change.id);
  const top = s.undo.at(-1);
  const merged = top && !s.redo.length && now - top.atMs <= MERGE_WINDOW_MS ? edits.merge(top.change, change) : null;
  s.redo = [];
  if (top && merged) {
    if (edits.isEmpty(merged)) s.undo.pop(); else Object.assign(top, { change: merged, atMs: now, label: describe(merged) });
    return;
  }
  s.undo.push({ id: newId(), atMs: now, label: describe(change), change });
  if (s.undo.length > DEPTH) s.undo.shift();
}

export function stepService(service: Service, redo: boolean): Service | null {
  const next = structuredClone(service);
  const change = take(service.id, redo);
  if (!change) return null;
  edits.applyToService(change, next as unknown as edits.ServiceRecord);
  return next;
}

export function stepSong(song: Song, redo: boolean): Song | null {
  const next = structuredClone(song);
  const change = take(song.id, redo);
  if (!change) return null;
  edits.applyToSong(change, next as unknown as edits.SongRecord);
  return next;
}

function take(id: string, redo: boolean): edits.Change | null {
  const s = stacksOf(id);
  const step = (redo ? s.redo : s.undo).pop();
  if (!step) return null;
  (redo ? s.undo : s.redo).push(step);
  return redo ? step.change : edits.inverted(step.change);
}
