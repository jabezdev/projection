import { json, same, stored } from "./json";
import { diffList, insert, moveTo, predecessor, remove } from "./list";
import type { ItemField, ItemRecord, Json, ServiceField, ServiceOp, ServiceRecord } from "./types";

const FIELDS: ServiceField[] = ["scheduledAtMs", "title", "leader", "notes", "endedAtMs"];
const SONG_ITEM_FIELDS: ItemField[] = ["title", "notes", "background", "key", "arrangement", "versionId"];
const ITEM_FIELDS: ItemField[] = ["title", "notes", "background", "content"];
const SONG_FIELDS = new Set<ItemField>(["key", "arrangement", "versionId"]);

export function diffService(before: ServiceRecord, after: ServiceRecord): ServiceOp[] {
  const ops: ServiceOp[] = [];
  for (const field of FIELDS) {
    const [b, a] = [json(before[field]), json(after[field])];
    if (!same(b, a)) ops.push({ op: "set", field, before: b, after: a });
  }
  for (const change of diffList(before.items, after.items)) {
    if (change.kind === "insert") ops.push({ op: "insert_item", item: change.value, prev: change.prev });
    else if (change.kind === "remove") ops.push({ op: "remove_item", item: change.value, prev: change.prev });
    else ops.push({ op: "move_item", id: change.id, from: change.from, to: change.to });
  }
  for (const next of after.items) {
    const old = before.items.find((i) => i.id === next.id);
    if (!old) continue;
    for (const field of itemFields(old, next)) {
      const [b, a] = [getItem(old, field) ?? null, getItem(next, field) ?? null];
      if (!same(b, a)) ops.push({ op: "set_item", id: next.id, field, before: b, after: a });
    }
  }
  return ops;
}

const itemFields = (old: ItemRecord, next: ItemRecord) =>
  old.content.kind === "song" && next.content.kind === "song" && old.content.songId === next.content.songId ? SONG_ITEM_FIELDS : ITEM_FIELDS;

export function invertServiceOp(op: ServiceOp): ServiceOp {
  switch (op.op) {
    case "set": return { ...op, before: op.after, after: op.before };
    case "insert_item": return { op: "remove_item", item: op.item, prev: op.prev };
    case "remove_item": return { op: "insert_item", item: op.item, prev: op.prev };
    case "move_item": return { ...op, from: op.to, to: op.from };
    case "set_item": return { ...op, before: op.after, after: op.before };
  }
}

export function applyServiceOp(op: ServiceOp, service: ServiceRecord): boolean {
  switch (op.op) {
    case "set": {
      const clean = same(service[op.field], op.before);
      setField(service, op.field, op.after);
      return clean;
    }
    case "insert_item": return insert(service.items, structuredClone(op.item), op.prev);
    case "remove_item": return remove(service.items, op.item);
    case "move_item": {
      const clean = predecessor(service.items, op.id) === op.from;
      return moveTo(service.items, op.id, op.to) && clean;
    }
    case "set_item": {
      const item = service.items.find((i) => i.id === op.id);
      if (!item) return false;
      const current = getItem(item, op.field);
      if (current === undefined) return false;
      const clean = same(current, op.before);
      setItem(item, op.field, op.after);
      return clean;
    }
  }
}

export function serviceValue(op: ServiceOp, service: ServiceRecord): Json | undefined {
  if (op.op === "set") return json(service[op.field]);
  if (op.op !== "set_item") return undefined;
  const item = service.items.find((i) => i.id === op.id);
  return item ? getItem(item, op.field) : undefined;
}

function setField(service: ServiceRecord, field: ServiceField, value: Json) {
  const record = service as Record<string, unknown>;
  if (value === null) delete record[field]; else record[field] = structuredClone(value);
}

function getItem(item: ItemRecord, field: ItemField): Json | undefined {
  if (SONG_FIELDS.has(field)) return item.content.kind === "song" ? json(item.content[field]) : undefined;
  if (field === "content") return json(item.content);
  return json(item[field as "title" | "notes" | "background"]);
}

function setItem(item: ItemRecord, field: ItemField, value: Json) {
  const target = (SONG_FIELDS.has(field) ? item.content : item) as Record<string, unknown>;
  if (field === "content") { item.content = stored(value) ?? item.content; return; }
  const next = stored(value);
  if (next === undefined) delete target[field]; else target[field] = next;
}

export function mergeServiceOps(earlier: ServiceOp[], later: ServiceOp[]): ServiceOp[] | null {
  if (earlier.length !== later.length) return null;
  const merged: ServiceOp[] = [];
  for (const old of earlier) {
    if (old.op === "set") {
      const next = later.find((op) => op.op === "set" && op.field === old.field);
      if (!next || next.op !== "set") return null;
      if (!same(old.before, next.after)) merged.push({ ...old, after: next.after });
    } else if (old.op === "set_item") {
      const next = later.find((op) => op.op === "set_item" && op.id === old.id && op.field === old.field);
      if (!next || next.op !== "set_item") return null;
      if (!same(old.before, next.after)) merged.push({ ...old, after: next.after });
    } else return null;
  }
  return merged;
}
