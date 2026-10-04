import { describe, expect, test } from "bun:test";
import { parseLabelLine, parseLyrics, shortLabel, toSlides } from "./lyrics";

describe("parseLabelLine", () => {
  test("recognizes common label styles", () => {
    expect(parseLabelLine("Verse 1")).toBe("Verse 1");
    expect(parseLabelLine("[CHORUS]")).toBe("Chorus");
    expect(parseLabelLine("pre-chorus:")).toBe("Pre-Chorus");
    expect(parseLabelLine("(Bridge)")).toBe("Bridge");
  });

  test("ignores lyric lines", () => {
    expect(parseLabelLine("Amazing grace how sweet the sound")).toBeNull();
    expect(parseLabelLine("Chorus of angels sing")).toBeNull();
  });
});

describe("parseLyrics", () => {
  test("splits on labels and keeps inner blank lines as slide breaks", () => {
    const sections = parseLyrics(
      "Verse 1\nline a\nline b\n\nline c\n\nChorus\nline d\n\n[Verse 2]\nline e\n",
    );
    expect(sections).toEqual([
      { label: "Verse 1", lyrics: "line a\nline b\n\nline c" },
      { label: "Chorus", lyrics: "line d" },
      { label: "Verse 2", lyrics: "line e" },
    ]);
  });

  test("falls back to blank-line blocks when there are no labels", () => {
    expect(parseLyrics("a\nb\n\n\nc\r\n")).toEqual([
      { label: "Part 1", lyrics: "a\nb" },
      { label: "Part 2", lyrics: "c" },
    ]);
  });

  test("text before the first label becomes an intro", () => {
    expect(parseLyrics("hum\nVerse 1\nx")).toEqual([
      { label: "Intro", lyrics: "hum" },
      { label: "Verse 1", lyrics: "x" },
    ]);
  });
});

describe("toSlides", () => {
  test("respects explicit breaks, then chunks by line count", () => {
    expect(toSlides("1\n2\n3\n\n4", 2)).toEqual(["1\n2", "3", "4"]);
  });
});

test("shortLabel", () => {
  expect(shortLabel("Verse 2")).toBe("V2");
  expect(shortLabel("Chorus")).toBe("C");
  expect(shortLabel("Pre-Chorus")).toBe("PC");
});
