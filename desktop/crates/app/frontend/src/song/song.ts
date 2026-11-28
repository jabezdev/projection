import type { Entry, Section, SectionKind, SectionPatch, SessionItem, Song, SongItem, SongVersion } from '../api/models';

export const splitSlides = (lyrics: string) => lyrics.replace(/\r\n?/g, '\n').split(/\n[ \t]*\n(?:[ \t]*\n)*/).map(slide => slide.split('\n').map(line => line.trim()).join('\n').trim()).filter(Boolean);
export const sectionKinds: { kind: SectionKind; label: string }[] = [
  { kind: 'verse', label: 'Verse' }, { kind: 'pre_chorus', label: 'Pre-chorus' }, { kind: 'chorus', label: 'Chorus' }, { kind: 'bridge', label: 'Bridge' },
  { kind: 'intro', label: 'Intro' }, { kind: 'outro', label: 'Outro' }, { kind: 'tag', label: 'Tag' }, { kind: 'other', label: 'Other' },
];
const colors: Record<SectionKind, string> = { verse: '#4f91e8', chorus: '#e76774', pre_chorus: '#efa34a', bridge: '#aa84df', intro: '#50c6d6', outro: '#ddbd53', tag: '#79c56b', other: '#a3a8b2' };
const prefixes: Record<SectionKind, string> = { verse: 'V', pre_chorus: 'PC', chorus: 'C', bridge: 'BR', intro: 'I', outro: 'O', tag: 'T', other: '' };
export const kindColor = (kind: SectionKind) => colors[kind];
export const sectionColor = (section: Section) => section.color || colors[section.kind];
export const verseCode = (caption?: string) => caption?.match(/(\d+(?::\d+)?) · /)?.[1];
export const sectionCode = (section: Section) => `${section.kind === 'other' ? (verseCode(section.name) || section.name?.trim()[0]?.toUpperCase() || 'P') : prefixes[section.kind]}${section.number ?? ''}`;
export const sectionLabel = (section: Section) => section.kind === 'other' ? (section.name?.trim() || 'Part') : `${sectionKinds.find(k => k.kind === section.kind)?.label}${section.number != null ? ` ${section.number}` : ''}${section.name ? ` · ${section.name}` : ''}`;
export const slides = (section: Section) => splitSlides(section.lyrics);

export type SongDraft = { sections: Section[]; arrangement: string[]; background?: string };
export const emptyDraft = (): SongDraft => ({ sections: [], arrangement: [] });
export const emptySong = (): Song => ({ id: '', title: '', archived: false, credits: {}, tags: [], current: { id: '', number: 1, sections: [], arrangement: [] } });

export function arranged(draft: SongDraft): Section[] {
  return draft.arrangement.flatMap(id => draft.sections.filter(section => section.id === id).slice(0, 1));
}
export const draftSteps = (draft: SongDraft) => arranged(draft).flatMap(slides);

export function moveArrangement(arrangement: string[], from: number, destination: number) {
  if (from < 0 || from >= arrangement.length) return arrangement;
  const next = [...arrangement];
  const [id] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(next.length, destination - (from < destination ? 1 : 0))), 0, id);
  return next;
}

export function slideAtCaret(lyrics: string, caret: number) {
  const separator = /\n[ \t]*\n(?:[ \t]*\n)*/g;
  let index = 0;
  for (let match = separator.exec(lyrics); match; match = separator.exec(lyrics)) {
    const end = match.index + match[0].length;
    if (end > caret) break;
    if (/\S/.test(lyrics.slice(0, match.index)) && /\S/.test(lyrics.slice(end))) index++;
  }
  return Math.max(0, Math.min(index, splitSlides(lyrics).length - 1));
}

const isEmpty = (patch?: SectionPatch) => !patch || Object.values(patch).every(v => v === undefined);
export function applyPatch(id: string, patch: SectionPatch | undefined, base?: Section): Section {
  return { id, kind: patch?.kind ?? base?.kind ?? 'other', number: patch?.number ?? base?.number, name: patch?.name ?? base?.name, color: patch?.color ?? base?.color, lyrics: patch?.lyrics ?? base?.lyrics ?? '' };
}

export function resolveSong(item: SongItem, version: SongVersion): Section[] {
  const find = (id?: string) => id ? version.sections.find(s => s.id === id) : undefined;
  if (!item.arrangement) return version.arrangement.flatMap(id => { const s = find(id); return s ? [s] : []; });
  return item.arrangement.flatMap(entry => {
    const base = find(entry.sectionId);
    if (entry.sectionId && !base && isEmpty(entry.variation)) return [];
    return [applyPatch(entry.id, entry.variation, base)];
  });
}

export function entriesOf(item: SongItem, version: SongVersion): Entry[] {
  return item.arrangement ?? version.arrangement.map((sectionId, index) => ({ id: `e${index}-${sectionId}`, sectionId }));
}

export function serviceDraft(item: SongItem, version: SongVersion): SongDraft {
  const entries = entriesOf(item, version);
  const sections = version.sections.map(base => {
    const varied = entries.find(e => e.sectionId === base.id && !isEmpty(e.variation));
    return varied ? applyPatch(base.id, varied.variation, base) : base;
  });
  const own = entries.filter(e => !e.sectionId).map(e => applyPatch(e.id, e.variation));
  return { sections: [...sections, ...own], arrangement: entries.map(e => e.sectionId ?? e.id) };
}

function diff(section: Section, base: Section): SectionPatch {
  const patch: SectionPatch = {};
  for (const key of ['kind', 'number', 'name', 'color', 'lyrics'] as const) if (section[key] !== base[key]) (patch as Record<string, unknown>)[key] = section[key];
  return patch;
}

export function fromServiceDraft(draft: SongDraft, version: SongVersion): Entry[] | undefined {
  const entries = draft.arrangement.flatMap((id, index): Entry[] => {
    const section = draft.sections.find(s => s.id === id);
    if (!section) return [];
    const base = version.sections.find(s => s.id === id);
    if (!base) return [{ id: draft.arrangement.indexOf(id) === index ? id : `${id}-${index}`, variation: { kind: section.kind, number: section.number, name: section.name, color: section.color, lyrics: section.lyrics } }];
    const patch = diff(section, base);
    return [{ id: `e${index}-${id}`, sectionId: id, ...(isEmpty(patch) ? {} : { variation: patch }) }];
  });
  const plain = entries.every(e => e.sectionId && !e.variation) && entries.length === version.arrangement.length && entries.every((e, i) => e.sectionId === version.arrangement[i]);
  return plain ? undefined : entries;
}

export type Occurrence = { key: string; section: Section; lines: string[]; start: number };
export const SCRIPTURE_COLOR = '#c9a24a';
export function occurrences(item: SessionItem): Occurrence[] {
  if (item.kind.kind !== 'text') return [];
  const captions = item.kind.data.captions;
  if (captions?.length) {
    const steps = item.kind.data.steps;
    const runs: Occurrence[] = [];
    steps.forEach((line, index) => {
      const caption = captions[index] ?? '';
      const last = runs.at(-1);
      if (last && last.section.name === caption) last.lines.push(line);
      else runs.push({ key: `${item.id}-${index}`, section: { id: `${item.id}-${index}`, kind: 'other', name: caption, color: SCRIPTURE_COLOR, lyrics: '' }, lines: [line], start: index });
    });
    return runs;
  }
  const sections: Section[] = item.song?.sections ?? [{ id: 'text', kind: 'other', name: 'Text', lyrics: item.kind.data.steps.join('\n\n') }];
  let offset = 0;
  return sections.map((section, index) => {
    const lines = item.song ? slides(section) : (item.kind.kind === 'text' ? item.kind.data.steps : []);
    const start = offset;
    offset += lines.length;
    return { key: `${section.id}-${index}`, section, lines, start };
  });
}
