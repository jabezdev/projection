import { json, same, stored } from "./json";
import { diffList, insert, moveTo, predecessor, remove } from "./list";
import type { Json, SongField, SongOp, SongRecord } from "./types";

const FIELDS: SongField[] = ["title", "archived", "credits", "key", "tags", "background"];

export function diffSong(before: SongRecord, after: SongRecord): SongOp[] {
  const ops: SongOp[] = FIELDS.flatMap((field) => setOp(before, after, field));
  for (const change of diffList(before.current.sections, after.current.sections)) {
    if (change.kind === "insert") ops.push({ op: "insert_section", section: change.value, prev: change.prev });
    else if (change.kind === "remove") ops.push({ op: "remove_section", section: change.value, prev: change.prev });
    else ops.push({ op: "move_section", id: change.id, from: change.from, to: change.to });
  }
  for (const next of after.current.sections) {
    const old = before.current.sections.find((s) => s.id === next.id);
    if (old && !same(old, next)) ops.push({ op: "set_section", id: next.id, before: old, after: next });
  }
  ops.push(...setOp(before, after, "arrangement"));
  return ops;
}

function setOp(before: SongRecord, after: SongRecord, field: SongField): SongOp[] {
  const [b, a] = [get(before, field), get(after, field)];
  return same(b, a) ? [] : [{ op: "set", field, before: b, after: a }];
}

function get(song: SongRecord, field: SongField): Json {
  switch (field) {
    case "archived": return song.archived ?? false;
    case "credits": return json(song.credits ?? {});
    case "tags": return json(song.tags ?? []);
    case "background": return json(song.current.background);
    case "arrangement": return json(song.current.arrangement);
    default: return json(song[field]);
  }
}

function set(song: SongRecord, field: SongField, value: Json) {
  const target = (field === "background" || field === "arrangement" ? song.current : song) as Record<string, unknown>;
  const next = stored(value);
  if (next === undefined) delete target[field]; else target[field] = next;
}

export function songValue(op: SongOp, song: SongRecord): Json | undefined {
  if (op.op === "set") return get(song, op.field);
  if (op.op !== "set_section") return undefined;
  const section = song.current.sections.find((s) => s.id === op.id);
  return section ? json(section) : undefined;
}

export function invertSongOp(op: SongOp): SongOp {
  switch (op.op) {
    case "set": return { ...op, before: op.after, after: op.before };
    case "insert_section": return { op: "remove_section", section: op.section, prev: op.prev };
    case "remove_section": return { op: "insert_section", section: op.section, prev: op.prev };
    case "move_section": return { ...op, from: op.to, to: op.from };
    case "set_section": return { ...op, before: op.after, after: op.before };
  }
}

export function applySongOp(op: SongOp, song: SongRecord): boolean {
  const sections = song.current.sections;
  switch (op.op) {
    case "set": {
      const clean = same(get(song, op.field), op.before);
      set(song, op.field, op.after);
      return clean;
    }
    case "insert_section": return insert(sections, structuredClone(op.section), op.prev);
    case "remove_section": return remove(sections, op.section);
    case "move_section": {
      const clean = predecessor(sections, op.id) === op.from;
      return moveTo(sections, op.id, op.to) && clean;
    }
    case "set_section": {
      const i = sections.findIndex((s) => s.id === op.id);
      if (i < 0) return false;
      const clean = same(sections[i], op.before);
      sections[i] = structuredClone(op.after);
      return clean;
    }
  }
}

export function mergeSongOps(earlier: SongOp[], later: SongOp[]): SongOp[] | null {
  if (earlier.length !== later.length) return null;
  const merged: SongOp[] = [];
  for (const old of earlier) {
    if (old.op === "set") {
      const next = later.find((op) => op.op === "set" && op.field === old.field);
      if (!next || next.op !== "set") return null;
      if (!same(old.before, next.after)) merged.push({ ...old, after: next.after });
    } else if (old.op === "set_section") {
      const next = later.find((op) => op.op === "set_section" && op.id === old.id);
      if (!next || next.op !== "set_section") return null;
      if (!same(old.before, next.after)) merged.push({ ...old, after: next.after });
    } else return null;
  }
  return merged;
}
