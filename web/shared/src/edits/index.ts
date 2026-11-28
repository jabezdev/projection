import { applyServiceOp, diffService, invertServiceOp, mergeServiceOps } from "./service";
import { applySongOp, diffSong, invertSongOp, mergeSongOps } from "./song";
import type { Change, ServiceRecord, SongRecord } from "./types";

export * from "./types";
export { describe } from "./label";
export { same } from "./json";
export { rebase, type Rebased } from "./rebase";
export { diffService, diffSong };

export const isEmpty = (change: Change) => change.ops.length === 0;

export function inverted(change: Change): Change {
  return change.kind === "service"
    ? { ...change, ops: [...change.ops].reverse().map(invertServiceOp) }
    : { ...change, ops: [...change.ops].reverse().map(invertSongOp) };
}

export function applyToService(change: Change, service: ServiceRecord): number {
  if (change.kind !== "service") return change.ops.length;
  return change.ops.filter((op) => !applyServiceOp(op, service)).length;
}

export function applyToSong(change: Change, song: SongRecord): number {
  if (change.kind !== "song") return change.ops.length;
  return change.ops.filter((op) => !applySongOp(op, song)).length;
}

export function merge(earlier: Change, later: Change): Change | null {
  if (earlier.id !== later.id) return null;
  if (earlier.kind === "service" && later.kind === "service") {
    const ops = mergeServiceOps(earlier.ops, later.ops);
    return ops && { ...earlier, ops };
  }
  if (earlier.kind === "song" && later.kind === "song") {
    const ops = mergeSongOps(earlier.ops, later.ops);
    return ops && { ...earlier, ops };
  }
  return null;
}
