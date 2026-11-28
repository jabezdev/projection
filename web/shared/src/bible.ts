import { BIBLE_BOOKS, type BibleBook } from "./bibleBooks";

export const VERSE_END = 65535;
export type Span = { book: number; from: [number, number]; to: [number, number] };
export type BibleRef = { ok: true; spans: Span[] } | { ok: false; error: string };

export const BUNDLED_BIBLES = [
  { id: "kjv", abbreviation: "KJV", name: "King James Version", language: "en" },
  { id: "adb1905", abbreviation: "ADB1905", name: "Ang Dating Biblia (1905)", language: "tl" },
];

const FILLER = new Set(["mga", "taga", "kay", "ang", "ng", "of", "the", "book", "sulat", "ni", "san", "st", "saint", "sa", "aklat"]);
const ORDINALS: Record<string, string> = { "1": "1", i: "1", "1st": "1", first: "1", una: "1", unang: "1", "2": "2", ii: "2", "2nd": "2", second: "2", ikalawa: "2", ikalawang: "2", "3": "3", iii: "3", "3rd": "3", third: "3", ikatlo: "3", ikatlong: "3" };

const fold = (word: string) => word.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/['’‘,.;:!?"“”]/g, "").toLowerCase();

export function normalizeBook(text: string) {
  const spaced = text.toLowerCase().replace(/[.\-_–]/g, " ").replace(/(\d)(\p{L})/gu, "$1 $2");
  const words = spaced.split(/\s+/).filter(Boolean);
  return words.flatMap((word, i) => {
    if (i === 0 && words.length > 1 && ORDINALS[word]) return [ORDINALS[word]];
    return FILLER.has(word) ? [] : [fold(word)];
  }).join(" ");
}

let keys: [string, number][] | null = null;
function bookKeys() {
  if (keys) return keys;
  const out: [string, number][] = [];
  BIBLE_BOOKS.forEach((b, i) => {
    for (const name of [b.en, b.tl, b.code, b.osis, ...b.aliases]) {
      const key = normalizeBook(name);
      if (key && !out.some(([k, n]) => k === key && n === i)) out.push([key, i]);
    }
  });
  return (keys = out);
}

function findBook(text: string): number | string {
  const query = normalizeBook(text);
  if (!query) return "Type a book name, like John 3:16";
  const exact = bookKeys().find(([k]) => k === query);
  if (exact) return exact[1];
  if ([...query].filter((c) => /\p{L}/u.test(c)).length < 2) return `“${text.trim()}” is too short for a book name`;
  const found = [...new Set(bookKeys().filter(([k]) => k.startsWith(query)).map(([, b]) => b))].sort((a, b) => a - b);
  if (found.length === 1) return found[0];
  if (!found.length) return `No book called “${text.trim()}”`;
  return `“${text.trim()}” could be ${found.slice(0, 4).map((b) => BIBLE_BOOKS[b].en).join(", ")}`;
}

function splitBook(segment: string): [string, string] {
  let seenLetter = false;
  for (let i = 0; i < segment.length; i++) {
    const c = segment[i];
    if (/\p{L}/u.test(c)) seenLetter = true;
    else if (/\d/.test(c) && seenLetter) return [segment.slice(0, i), segment.slice(i)];
  }
  return seenLetter ? [segment, ""] : ["", segment];
}

function number(text: string): number {
  const match = text.match(/^(\d+)[abc]?$/);
  if (!match) throw new Error(`“${text}” is not a chapter or verse number`);
  const n = Number(match[1]);
  if (n === 0) throw new Error("Chapters and verses start at 1");
  if (n > VERSE_END) throw new Error(`“${text}” is too large`);
  return n;
}

function point(text: string): [number | null, number] {
  const [c, v] = text.split(":");
  return v === undefined ? [null, number(c)] : [number(c), number(v)];
}

export function parseReference(input: string): BibleRef {
  try {
    const spans: Span[] = [];
    let current: number | null = null;
    for (const segment of input.split(";").map((s) => s.trim()).filter(Boolean)) {
      const [name, rest] = splitBook(segment);
      let index: number;
      if (!name.trim()) {
        if (current === null) throw new Error("Start with a book name, like John 3:16");
        index = current;
      } else {
        const found = findBook(name);
        if (typeof found === "string") throw new Error(found);
        index = found;
      }
      current = index;
      const info = BIBLE_BOOKS[index];
      const single = info.chapters === 1;
      const numbers = rest.replace(/\s+/g, "").replace(/[.：]/g, ":").replace(/[–—‑‒]/g, "-").toLowerCase();
      if (!numbers) { spans.push({ book: index, from: [1, 1], to: [info.chapters, VERSE_END] }); continue; }
      let chapter: number | null = null;
      for (const part of numbers.split(",").filter(Boolean)) {
        const [start, end] = part.split("-") as [string, string | undefined];
        const [startChapter, startValue] = point(start);
        const close = (c: number): [number, number] => {
          if (end === undefined) return [c, startValue];
          const [c2, v2] = point(end);
          return c2 === null ? [c, v2] : [c2, v2];
        };
        let span: Span;
        if (startChapter !== null) {
          const to = close(startChapter);
          chapter = to[0];
          span = { book: index, from: [startChapter, startValue], to };
        } else if (chapter !== null || single) {
          const c = single ? 1 : chapter!;
          const to = close(c);
          chapter = to[0];
          span = { book: index, from: [c, startValue], to };
        } else if (end !== undefined && end.includes(":")) {
          const [c2, v2] = point(end);
          chapter = c2;
          span = { book: index, from: [startValue, 1], to: [c2!, v2] };
        } else {
          span = { book: index, from: [startValue, 1], to: [end === undefined ? startValue : point(end)[1], VERSE_END] };
        }
        if (span.from[0] > info.chapters || span.to[0] > info.chapters) throw new Error(`${info.en} has ${info.chapters} chapters`);
        if (span.to[0] < span.from[0] || (span.to[0] === span.from[0] && span.to[1] < span.from[1])) throw new Error(`“${part}” ends before it starts`);
        spans.push(span);
      }
    }
    if (!spans.length) throw new Error("Type a reference, like John 3:16");
    return { ok: true, spans };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

const bookName = (book: BibleBook, language: string) => (language.startsWith("tl") || language.startsWith("fil") ? book.tl : book.en);

export function formatReference(spans: Span[], language = "en") {
  let out = "";
  let previous: [number, number] | null = null;
  for (const span of spans) {
    const info = BIBLE_BOOKS[span.book];
    const wholeChapters = span.from[1] === 1 && span.to[1] === VERSE_END;
    const wholeBook = wholeChapters && span.from[0] === 1 && span.to[0] === info.chapters && info.chapters > 1;
    const sameBook = previous?.[0] === span.book;
    const sameChapter = sameBook && previous?.[1] === span.from[0] && !wholeChapters;
    if (out) out += sameChapter ? ", " : "; ";
    if (!sameBook) out += bookName(info, language) + (wholeBook ? "" : " ");
    const verse = (v: number) => (v === VERSE_END ? "" : String(v));
    if (wholeChapters) {
      if (!wholeBook) out += span.to[0] !== span.from[0] ? `${span.from[0]}–${span.to[0]}` : String(span.from[0]);
    } else if (info.chapters === 1) {
      out += verse(span.from[1]) + (span.to[1] !== span.from[1] ? `–${verse(span.to[1])}` : "");
    } else {
      if (!sameChapter) out += `${span.from[0]}:`;
      out += verse(span.from[1]);
      if (span.to[0] !== span.from[0]) out += `–${span.to[0]}:${verse(span.to[1])}`;
      else if (span.to[1] !== span.from[1]) out += `–${verse(span.to[1])}`;
    }
    previous = [span.book, span.to[0]];
  }
  return out.trim();
}

export function scriptureTitle(reference: string, translation: string, parallel?: string) {
  const parsed = parseReference(reference);
  const language = BUNDLED_BIBLES.find((b) => b.id === translation)?.language ?? "en";
  const shown = parsed.ok ? formatReference(parsed.spans, language) : reference.trim() || "Bible passage";
  return [shown, translation.toUpperCase(), parallel?.toUpperCase()].filter(Boolean).join(" · ");
}
