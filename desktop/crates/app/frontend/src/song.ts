export type Part = { id: string; type: string; number: number; lyrics: string; slideTexts?: string[]; color?: string };
export type SongDocument = { format: 'magi.song.v1'; slideSeparator?: 'blank-line'; parts: Part[]; arrangement: string[]; background: string; authorSinger?: string };
export const emptySong = (): SongDocument => ({ format: 'magi.song.v1', slideSeparator: 'blank-line', parts: [], arrangement: [], background: '' });
export const splitSlides = (lyrics: string) => lyrics.replace(/\r\n?/g, '\n').split(/\n[ \t]*\n(?:[ \t]*\n)*/).map(slide => slide.trim()).filter(Boolean);
export const partColors: Record<string, string> = { Verse: '#4f91e8', Chorus: '#e76774', 'Pre-chorus': '#efa34a', Bridge: '#aa84df', Intro: '#50c6d6', Outro: '#ddbd53', Tag: '#79c56b' };
export const partColor = (part: Part) => part.color || partColors[part.type] || '#a3a8b2';
export const slides = (part: Part) => part.slideTexts || splitSlides(part.lyrics);
export const partCode = (part: Part) => `${part.type === 'Bridge' ? 'BR' : part.type === 'Pre-chorus' ? 'PC' : part.type.charAt(0).toUpperCase()}${part.number}`;
export function decodeSong(lyrics: string): SongDocument {
  try {
    const value = JSON.parse(lyrics);
    if (value.format === 'magi.song.v1' && Array.isArray(value.parts) && Array.isArray(value.arrangement)) {
      // Preserve slides saved by the former single-line editor when opening old songs.
      if (value.slideSeparator !== 'blank-line') {
        value.parts = value.parts.map((part: Part) => ({ ...part, lyrics: (part.slideTexts || part.lyrics.split('\n').map(line => line.trim()).filter(Boolean)).join('\n\n'), slideTexts: undefined }));
        value.slideSeparator = 'blank-line';
      }
      return value;
    }
  } catch { /* Older library songs contain plain lyrics. */ }
  const part = { id: 'legacy', type: 'Verse', number: 1, lyrics, slideTexts: splitSlides(lyrics) };
  return { ...emptySong(), parts: [part], arrangement: [part.id] };
}
export function arrangedParts(song: SongDocument) {
  return song.arrangement.flatMap(id => { const part = song.parts.find(part => part.id === id); return part ? [part] : []; });
}
export const songSteps = (song: SongDocument) => arrangedParts(song).flatMap(slides);

// Destination is a gap between occurrences, including the gap after the last one.
export function moveArrangement(arrangement: string[], from: number, destination: number) {
  if (from < 0 || from >= arrangement.length) return arrangement;
  const next = [...arrangement];
  const [id] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(next.length, destination - (from < destination ? 1 : 0))), 0, id);
  return next;
}
