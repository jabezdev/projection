import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { BIBLE_BOOKS } from "./bibleBooks";
import { formatReference, parseReference, scriptureTitle, VERSE_END } from "./bible";

const spans = (text: string) => {
  const parsed = parseReference(text);
  if (!parsed.ok) throw new Error(parsed.error);
  return parsed.spans;
};

describe("bible references", () => {
  test("the book table matches the desktop fixture", () => {
    const fixture = JSON.parse(readFileSync(new URL("../../../desktop/fixtures/bible-books.json", import.meta.url), "utf8"));
    expect(BIBLE_BOOKS).toEqual(fixture);
  });

  test("read the usual ways of writing them, like the desktop", () => {
    expect(spans("John 3:16")).toEqual([{ book: 42, from: [3, 16], to: [3, 16] }]);
    expect(spans("jn 3:16-18, 20")).toEqual([{ book: 42, from: [3, 16], to: [3, 18] }, { book: 42, from: [3, 20], to: [3, 20] }]);
    expect(spans("Juan 3.16–4:2")).toEqual([{ book: 42, from: [3, 16], to: [4, 2] }]);
    expect(spans("Ps 23")).toEqual([{ book: 18, from: [23, 1], to: [23, VERSE_END] }]);
    expect(spans("1 Cor 13:4-7; Rom 8:28")).toEqual([{ book: 45, from: [13, 4], to: [13, 7] }, { book: 44, from: [8, 28], to: [8, 28] }]);
    expect(spans("1 Mga Taga-Corinto 13:4")[0].book).toBe(45);
    expect(spans("Jude 3")).toEqual([{ book: 64, from: [1, 3], to: [1, 3] }]);
    expect(spans("Pahayag 21:4")[0].book).toBe(65);
  });

  test("explain what is wrong", () => {
    expect(parseReference("Hezekiah 1:1")).toEqual({ ok: false, error: "No book called “Hezekiah”" });
    expect(parseReference("Ju 1:1").ok).toBe(false);
    expect(parseReference("John 30:1")).toEqual({ ok: false, error: "John has 21 chapters" });
  });

  test("are written back in the Bible's language", () => {
    expect(formatReference(spans("jn 3:16-18, 20; 4:1"))).toBe("John 3:16–18, 20; 4:1");
    expect(formatReference(spans("jn 3:16"), "tl")).toBe("Juan 3:16");
    expect(scriptureTitle("awit 23", "adb1905", "kjv")).toBe("Mga Awit 23 · ADB1905 · KJV");
  });
});
