import { parseLyrics } from "./lyrics";
import { labelToSection, newId, sectionLabel, type Section } from "./sections";

export function songToText(sections: Section[]): string {
  return sections.map((s) => `[${sectionLabel(s)}]\n${s.lyrics.trim()}`).join("\n\n");
}

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

export function textToSong(text: string, current: { sections: Section[]; arrangement: string[] }): { sections: Section[]; arrangement: string[] } {
  const sections: Section[] = [];
  const added: string[] = [];
  for (const parsed of parseLyrics(text)) {
    const repeat = sections.find((s) => same(sectionLabel(s), parsed.label));
    if (repeat) {
      if (repeat.lyrics.trim() !== parsed.lyrics.trim()) repeat.lyrics = `${repeat.lyrics}\n\n${parsed.lyrics}`;
      continue;
    }
    const existing = current.sections.find((s) => same(sectionLabel(s), parsed.label));
    if (existing) {
      sections.push({ ...existing, lyrics: parsed.lyrics });
    } else {
      const section: Section = { id: newId(), ...labelToSection(parsed.label), lyrics: parsed.lyrics };
      sections.push(section);
      added.push(section.id);
    }
  }
  const kept = new Set(sections.map((s) => s.id));
  return { sections, arrangement: [...current.arrangement.filter((id) => kept.has(id)), ...added] };
}
