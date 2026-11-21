import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { labelToSection, newId, resolveEntries, sectionCode, sectionColor, sectionLabel, type SongContent } from "./sections";
import { toSlides } from "./lyrics";

const version: SongContent = {
  sections: [
    { id: "v", kind: "verse", number: 1, lyrics: "a" },
    { id: "c", kind: "chorus", lyrics: "b" },
  ],
  arrangement: ["v", "c", "c"],
};

describe("sections", () => {
  test("match the desktop's shared fixtures", () => {
    const fixtures = JSON.parse(readFileSync(new URL("../../../desktop/fixtures/sections.json", import.meta.url), "utf8"));
    for (const c of fixtures.codes) {
      const section = { id: "x", lyrics: "", ...c.section };
      expect([sectionCode(section), sectionColor(section)]).toEqual([c.code, c.color]);
    }
    for (const c of fixtures.slides) expect(toSlides(c.lyrics, 99)).toEqual(c.slides);
  });

  test("pasted labels become kinds and numbers", () => {
    expect(labelToSection("Verse 2")).toEqual({ kind: "verse", number: 2 });
    expect(labelToSection("Pre-Chorus")).toEqual({ kind: "pre_chorus" });
    expect(labelToSection("Interlude")).toEqual({ kind: "other", name: "Interlude" });
    expect(sectionLabel({ kind: "verse", number: 2 })).toBe("Verse 2");
  });

  test("an item without its own arrangement follows the library; variations mark the entry", () => {
    expect(resolveEntries(undefined, version).map((e) => e.lyrics)).toEqual(["a", "b", "b"]);
    const resolved = resolveEntries(
      [
        { id: "1", sectionId: "c", variation: { lyrics: "B!" } },
        { id: "2", variation: { kind: "tag", lyrics: "amen" } },
        { id: "3", sectionId: "gone" },
      ],
      version,
    );
    expect(resolved.map((e) => [e.kind, e.lyrics, e.isVariation, e.sectionId])).toEqual([
      ["chorus", "B!", true, "c"],
      ["tag", "amen", false, null],
    ]);
  });

  test("ids are unique and time-ordered", () => {
    const a = newId();
    const b = newId();
    expect(a).toHaveLength(23);
    expect(a).not.toBe(b);
    expect(a.slice(0, 10) <= b.slice(0, 10)).toBe(true);
  });
});
