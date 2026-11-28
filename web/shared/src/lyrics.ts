export type ParsedSection = { label: string; lyrics: string };

const LABEL_WORDS =
  "verse|chorus|pre-?chorus|bridge|tag|intro|outro|ending|refrain|interlude|vamp|coda|instrumental";
const LABEL_LINE = new RegExp(`^\\s*[\\[(]?\\s*((?:${LABEL_WORDS})(?:\\s*\\d+)?)\\s*[\\])]?\\s*:?\\s*$`, "i");
const BRACKETED = /^\s*\[([^[\]]{1,40})\]\s*$/;

export function parseLabelLine(line: string): string | null {
  const match = LABEL_LINE.exec(line);
  if (!match) return BRACKETED.exec(line)?.[1].trim() || null;
  return match[1]
    .toLowerCase()
    .replace(/\s+/g, " ")
    .replace(/(^|[\s-])\w/g, (c) => c.toUpperCase());
}

export function parseLyrics(text: string): ParsedSection[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const hasLabels = lines.some((line) => parseLabelLine(line) !== null);

  if (!hasLabels) {
    return splitSlides(text).map((lyrics, i) => ({ label: `Part ${i + 1}`, lyrics }));
  }

  const sections: ParsedSection[] = [];
  let current: { label: string; lines: string[] } | null = null;
  const flush = () => {
    if (!current) return;
    const lyrics = trimBlankLines(current.lines).join("\n");
    if (lyrics) sections.push({ label: current.label, lyrics });
  };
  for (const line of lines) {
    const label = parseLabelLine(line);
    if (label) {
      flush();
      current = { label, lines: [] };
    } else {
      current ??= { label: "Intro", lines: [] };
      current.lines.push(line.trimEnd());
    }
  }
  flush();
  return sections;
}

export function toSlides(lyrics: string, maxLines = 2): string[] {
  return splitSlides(lyrics).flatMap((block) => {
    const lines = block.split("\n");
    const slides: string[] = [];
    for (let i = 0; i < lines.length; i += maxLines) {
      slides.push(lines.slice(i, i + maxLines).join("\n"));
    }
    return slides;
  });
}

export function shortLabel(label: string): string {
  const [word, num] = label.trim().split(/\s+/);
  const lower = (word ?? "").toLowerCase();
  const short =
    lower.startsWith("pre") ? "PC" : lower === "interlude" ? "IL" : (word?.[0] ?? "?").toUpperCase();
  return num ? `${short}${num}` : short;
}

export function splitSlides(text: string): string[] {
  return text
    .replace(/\r\n?/g, "\n")
    .split(/\n\s*\n/)
    .map((block) => block.split("\n").map((l) => l.trimEnd()).join("\n").trim())
    .filter(Boolean);
}

function trimBlankLines(lines: string[]): string[] {
  let start = 0;
  let end = lines.length;
  while (start < end && !lines[start].trim()) start++;
  while (end > start && !lines[end - 1].trim()) end--;
  return lines.slice(start, end);
}

export function splitSlidesText(text: string): string[] {
  return text.replace(/\r\n?/g, "\n").split(/\n[ \t]*\n(?:[ \t]*\n)*/).map((slide) => slide.split("\n").map((line) => line.trim()).join("\n").trim()).filter(Boolean);
}
