import type { MediaFile, Song } from '../api/models';

export type SearchResult = { kind: 'song'; song: Song } | { kind: 'media'; media: MediaFile };

export const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

export function searchLibrary(query: string, songs: Song[], media: MediaFile[], limit = 30): SearchResult[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const all: { result: SearchResult; text: string; title: string }[] = [
    ...songs.map(song => ({ result: { kind: 'song', song } as const, text: song.title.toLowerCase(), title: song.title.toLowerCase() })),
    ...media.map(item => ({ result: { kind: 'media', media: item } as const, text: `${item.title} ${fileName(item.path ?? '')}`.toLowerCase(), title: item.title.toLowerCase() })),
  ];
  const matches = all.filter(entry => words.every(word => entry.text.includes(word)));
  const rank = (entry: typeof all[number]) => !words.length ? 0 : entry.title.startsWith(words.join(' ')) ? 0 : entry.title.includes(words[0]) ? 1 : 2;
  return matches.sort((a, b) => rank(a) - rank(b) || a.title.localeCompare(b.title)).slice(0, limit).map(entry => entry.result);
}

