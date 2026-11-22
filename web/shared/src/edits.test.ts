import { describe as group, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { applyToService, applyToSong, describe, diffService, diffSong, inverted, merge, type Change, type ServiceRecord, type SongRecord } from "./edits";

const fixtures = JSON.parse(readFileSync(new URL("../../../desktop/fixtures/edits.json", import.meta.url), "utf8"));
const service = (ids: string): ServiceRecord => ({ id: "s", scheduledAtMs: 1, title: "Sunday", items: [...ids].map((id) => ({ id, content: { kind: "text", steps: [id] } })) });

function roundTrip(before: ServiceRecord, after: ServiceRecord) {
  const change: Change = { kind: "service", id: before.id, ops: diffService(before, after) };
  const forward = structuredClone(before);
  expect(applyToService(change, forward)).toBe(0);
  expect(forward).toEqual(after);
  const back = structuredClone(after);
  expect(applyToService(inverted(change), back)).toBe(0);
  expect(back).toEqual(before);
  return change;
}

group("edits", () => {
  test("match the desktop's shared fixtures", () => {
    for (const c of fixtures.services) {
      const change = roundTrip(c.before, c.after);
      expect(change.ops).toEqual(c.ops);
      expect(describe(change, (id) => id)).toBe(c.label);
    }
    for (const c of fixtures.songs) {
      const before: SongRecord = c.before, after: SongRecord = c.after;
      const change: Change = { kind: "song", id: before.id, ops: diffSong(before, after) };
      expect(change.ops).toEqual(c.ops);
      expect(describe(change, (id) => id)).toBe(c.label);
      const forward = structuredClone(before);
      expect(applyToSong(change, forward)).toBe(0);
      expect(forward.current.sections).toEqual(after.current.sections);
      const back = structuredClone(after);
      expect(applyToSong(inverted(change), back)).toBe(0);
      expect(back.current).toEqual(before.current);
    }
  });

  test("every reordering round-trips", () => {
    for (const [a, b] of [["abcd", "dabc"], ["abcd", "bcda"], ["abcd", "dcba"], ["abcd", "axcd"], ["abcd", ""], ["", "xyz"], ["abcdef", "fxbdaey"]]) roundTrip(service(a), service(b));
    expect(roundTrip(service("abcd"), service("bcda")).ops).toEqual([{ op: "move_item", id: "a", from: null, to: "d" }]);
  });

  test("typing in one field merges", () => {
    const a = service(""), b = { ...a, title: "Sun" }, c = { ...a, title: "Sunday night" };
    const merged = merge({ kind: "service", id: "s", ops: diffService(a, b) }, { kind: "service", id: "s", ops: diffService(b, c) });
    expect(merged?.ops).toEqual([{ op: "set", field: "title", before: "Sunday", after: "Sunday night" }]);
  });
});
