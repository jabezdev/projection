import type { BibleHit, BibleInfo, Passage, SessionItem } from '../api/models';

const BIBLES: BibleInfo[] = [
  { id: 'kjv', name: 'King James Version', abbreviation: 'KJV', language: 'en', bundled: true },
  { id: 'adb1905', name: 'Ang Dating Biblia (1905)', abbreviation: 'ADB1905', language: 'tl', bundled: true },
];

type Verse = [string, string, number, number, string];
const VERSES: Verse[] = [
  ['kjv', 'John', 3, 16, 'For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.'],
  ['kjv', 'John', 3, 17, 'For God sent not his Son into the world to condemn the world; but that the world through him might be saved.'],
  ['adb1905', 'Juan', 3, 16, "Sapagka't gayon na lamang ang pagsinta ng Dios sa sanglibutan, na ibinigay niya ang kaniyang bugtong na Anak, upang ang sinomang sa kaniya'y sumampalataya ay huwag mapahamak, kundi magkaroon ng buhay na walang hanggan."],
  ['adb1905', 'Juan', 3, 17, "Sapagka't hindi sinugo ng Dios ang Anak sa sanglibutan upang hatulan ang sanglibutan; kundi upang ang sanglibutan ay maligtas sa pamamagitan niya."],
  ['kjv', 'Psalms', 23, 1, 'The LORD is my shepherd; I shall not want.'],
  ['kjv', 'Psalms', 23, 2, 'He maketh me to lie down in green pastures: he leadeth me beside the still waters.'],
  ['adb1905', 'Mga Awit', 23, 1, 'Ang Panginoon ay aking pastor; hindi ako mangangailangan.'],
  ['adb1905', 'Mga Awit', 23, 2, 'Kaniyang pinahihiga ako sa sariwang pastulan: inaakay niya ako sa siping ng mga tubig na pahingahan.'],
];
const BOOKS: Record<string, [string, string]> = { john: ['John', 'Juan'], jn: ['John', 'Juan'], juan: ['John', 'Juan'], ps: ['Psalms', 'Mga Awit'], psalm: ['Psalms', 'Mga Awit'], psalms: ['Psalms', 'Mga Awit'], awit: ['Psalms', 'Mga Awit'] };

function demoPassage(reference: string, translation: string, parallel?: string | null): Passage {
  const match = reference.trim().toLowerCase().match(/^([a-z ]+?)\s*(\d+)(?::(\d+)(?:-(\d+))?)?$/);
  const book = match ? BOOKS[match[1].replace(/^mga /, '').trim()] : undefined;
  if (!match || !book) return { title: reference, warning: 'The browser demo only knows John 3:16-17 and Psalm 23:1-2.', slides: [] };
  const chapter = Number(match[2]), from = Number(match[3] ?? 1), to = Number(match[4] ?? match[3] ?? 99);
  const pick = (id: string) => VERSES.filter(v => v[0] === id && (v[1] === book[0] || v[1] === book[1]) && v[2] === chapter && v[3] >= from && v[3] <= to);
  const primary = pick(translation);
  const second = parallel ? pick(parallel) : [];
  const names = [translation, parallel].filter(Boolean).map(id => BIBLES.find(b => b.id === id)?.abbreviation ?? String(id).toUpperCase()).join(' · ');
  const slides = primary.map(v => ({ text: v[4], parallel: second.find(s => s[3] === v[3])?.[4] ?? '', caption: `${v[1]} ${v[2]}:${v[3]} · ${names}` }));
  const span = primary.length > 1 ? `${primary[0][3]}–${primary.at(-1)![3]}` : String(primary[0]?.[3] ?? from);
  return { title: `${primary[0]?.[1] ?? book[0]} ${chapter}:${span} · ${names}`, warning: slides.length ? null : 'The browser demo only knows John 3:16-17 and Psalm 23:1-2.', slides };
}

export function demoScripture(item: SessionItem, content: { reference: string; translation: string; parallel?: string }): SessionItem {
  const found = demoPassage(content.reference, content.translation, content.parallel);
  const parallel = found.slides.map(s => s.parallel);
  return { ...item, title: item.title && !item.title.includes(' · ') ? item.title : found.title, kind: { kind: 'text', data: { steps: found.slides.map(s => s.text), captions: found.slides.map(s => s.caption), ...(parallel.some(Boolean) ? { parallel } : {}) } } };
}

export function runBibleCommand(command: string, args: Record<string, unknown> = {}): { done: boolean; value?: unknown } {
  switch (command) {
    case 'list_bibles': return { done: true, value: BIBLES };
    case 'bible_passage': return { done: true, value: demoPassage(String(args.reference ?? ''), String(args.translation ?? 'kjv'), args.parallel as string | null) };
    case 'search_bible': {
      const words = String(args.query ?? '').toLowerCase().split(/\s+/).filter(Boolean);
      const hits: BibleHit[] = VERSES.filter(v => v[0] === args.translation && words.every(w => v[4].toLowerCase().includes(w))).map(v => ({ reference: `${v[1]} ${v[2]}:${v[3]}`, text: v[4] }));
      return { done: true, value: hits };
    }
    case 'import_bible': return { done: true, value: null };
    case 'remove_bible': return { done: true };
    default: return { done: false };
  }
}
