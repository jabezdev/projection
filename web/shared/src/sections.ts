export type SectionKind = "verse" | "pre_chorus" | "chorus" | "bridge" | "intro" | "outro" | "tag" | "other";
export type Section = { id: string; kind: SectionKind; number?: number; name?: string; color?: string; lyrics: string };
export type SectionPatch = Partial<Omit<Section, "id">>;
export type Entry = { id: string; sectionId?: string; variation?: SectionPatch };
export type SongContent = { sections: Section[]; arrangement: string[]; background?: string };

export const SECTION_KINDS: { kind: SectionKind; label: string; code: string; color: string }[] = [
  { kind: "verse", label: "Verse", code: "V", color: "#4f91e8" },
  { kind: "pre_chorus", label: "Pre-chorus", code: "PC", color: "#efa34a" },
  { kind: "chorus", label: "Chorus", code: "C", color: "#e76774" },
  { kind: "bridge", label: "Bridge", code: "BR", color: "#aa84df" },
  { kind: "intro", label: "Intro", code: "I", color: "#50c6d6" },
  { kind: "outro", label: "Outro", code: "O", color: "#ddbd53" },
  { kind: "tag", label: "Tag", code: "T", color: "#79c56b" },
  { kind: "other", label: "Other", code: "", color: "#a3a8b2" },
];
const info = (kind: SectionKind) => SECTION_KINDS.find((k) => k.kind === kind)!;

export function sectionCode(section: Pick<Section, "kind" | "number" | "name">): string {
  const prefix = section.kind === "other" ? section.name?.trim()[0]?.toUpperCase() || "P" : info(section.kind).code;
  return `${prefix}${section.number ?? ""}`;
}

export function sectionLabel(section: Pick<Section, "kind" | "number" | "name">): string {
  if (section.kind === "other") return section.name?.trim() || "Part";
  const base = `${info(section.kind).label}${section.number != null ? ` ${section.number}` : ""}`;
  return section.name ? `${base} · ${section.name}` : base;
}

export const sectionColor = (section: Pick<Section, "kind" | "color">) => section.color || info(section.kind).color;

const WORDS: [RegExp, SectionKind][] = [
  [/^pre-?\s?chorus/i, "pre_chorus"],
  [/^chorus|^refrain/i, "chorus"],
  [/^verse/i, "verse"],
  [/^bridge/i, "bridge"],
  [/^intro/i, "intro"],
  [/^outro|^ending|^coda/i, "outro"],
  [/^tag/i, "tag"],
];

export function labelToSection(label: string): Pick<Section, "kind" | "number" | "name"> {
  const text = label.trim();
  const number = /(\d+)\s*$/.exec(text)?.[1];
  const kind = WORDS.find(([pattern]) => pattern.test(text))?.[1];
  if (!kind) return { kind: "other", name: text || undefined };
  return { kind, ...(number ? { number: Number(number) } : {}) };
}

const empty = (patch?: SectionPatch) => !patch || Object.values(patch).every((v) => v === undefined);

export function applyPatch(id: string, patch: SectionPatch | undefined, base?: Section): Section {
  return {
    id,
    kind: patch?.kind ?? base?.kind ?? "other",
    number: patch?.number ?? base?.number,
    name: patch?.name ?? base?.name,
    color: patch?.color ?? base?.color,
    lyrics: patch?.lyrics ?? base?.lyrics ?? "",
  };
}

export function entriesOf(arrangement: Entry[] | undefined, version: SongContent): Entry[] {
  return arrangement ?? version.arrangement.map((sectionId, index) => ({ id: `e${index}-${sectionId}`, sectionId }));
}

export type Resolved = Section & { entryId: string; sectionId: string | null; isVariation: boolean };

export function resolveEntries(arrangement: Entry[] | undefined, version: SongContent): Resolved[] {
  return entriesOf(arrangement, version).flatMap((entry) => {
    const base = entry.sectionId ? version.sections.find((s) => s.id === entry.sectionId) : undefined;
    if (entry.sectionId && !base && empty(entry.variation)) return [];
    return [{ ...applyPatch(entry.id, entry.variation, base), entryId: entry.id, sectionId: base ? base.id : null, isVariation: !!base && !empty(entry.variation) }];
  });
}

const ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz";
export function newId(): string {
  let time = Date.now();
  let out = "";
  for (let i = 0; i < 10; i++) {
    out = ALPHABET[time % 32] + out;
    time = Math.floor(time / 32);
  }
  for (const byte of crypto.getRandomValues(new Uint8Array(13))) out += ALPHABET[byte % 32];
  return out;
}
