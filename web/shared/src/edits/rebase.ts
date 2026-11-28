import { same } from "./json";
import { applyServiceOp, serviceValue } from "./service";
import { applySongOp, songValue } from "./song";
import type { Change, Json, ServiceOp, ServiceRecord, SongOp, SongRecord } from "./types";

export type Rebased = { applied: number; conflicts: { op: ServiceOp | SongOp; theirs: Json }[] };

const isValueOp = (op: ServiceOp | SongOp) => op.op === "set" || op.op === "set_item" || op.op === "set_section";

export function rebase(change: Change, record: ServiceRecord | SongRecord): Rebased {
  const result: Rebased = { applied: 0, conflicts: [] };
  for (const op of change.ops) {
    const apply = (target: ServiceRecord | SongRecord) =>
      change.kind === "service" ? applyServiceOp(op as ServiceOp, target as ServiceRecord) : applySongOp(op as SongOp, target as SongRecord);
    if (!isValueOp(op)) {
      apply(record);
      result.applied++;
      continue;
    }
    const theirs = change.kind === "service" ? serviceValue(op as ServiceOp, record as ServiceRecord) : songValue(op as SongOp, record as SongRecord);
    if (theirs === undefined) continue;
    const after = (op as { after: Json }).after;
    if (same(theirs, after)) continue;
    if (!same(theirs, (op as { before: Json }).before)) {
      result.conflicts.push({ op, theirs });
      continue;
    }
    apply(record);
    result.applied++;
  }
  return result;
}
