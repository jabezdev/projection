import { describe, expect, test } from "bun:test";
import type { Section } from "./sections";
import { songToText, textToSong } from "./songText";

const verse: Section = { id: "v1", kind: "verse", number: 1, lyrics: "a\nb\n\nc" };
const chorus: Section = { id: "c", kind: "chorus", color: "#123456", lyrics: "d" };
const bridge: Section = { id: "x", kind: "other", name: "Spontaneous", lyrics: "e" };
const current = { sections: [verse, chorus, bridge], arrangement: ["v1", "c", "v1", "c", "x"] };

describe("song text", () => {
  test("round-trips sections, ids, colours and the order", () => {
    expect(textToSong(songToText(current.sections), current)).toEqual(current);
  });

  test("keeps ids of renamed lyrics and adds new parts to the end of the order", () => {
    const next = textToSong("[Verse 1]\nnew words\n\n[Chorus]\nd\n\n[Pre-chorus]\nf", current);
    expect(next.sections.map((s) => [s.id === "v1" || s.id === "c" ? s.id : "new", s.kind, s.lyrics])).toEqual([
      ["v1", "verse", "new words"],
      ["c", "chorus", "d"],
      ["new", "pre_chorus", "f"],
    ]);
    expect(next.arrangement.slice(0, 4)).toEqual(["v1", "c", "v1", "c"]);
    expect(next.arrangement).toHaveLength(5);
  });

  test("a chorus written out twice stays one part", () => {
    const next = textToSong("Verse 1\na\n\nChorus\nd\n\nChorus\nd", current);
    expect(next.sections.map((s) => s.id)).toEqual(["v1", "c"]);
  });
});
