import { defaultCaptureOptions, defaultVideoOptions, newId, type CaptureSource, type MediaFile, type ServiceItem, type Song, type SongItem, type SongVersion } from '../api/models';
import { entriesOf } from '../song/song';

export const songItem = (song: Song): ServiceItem => ({ id: newId(), content: { kind: 'song', songId: song.id } });
export const mediaItem = (media: MediaFile): ServiceItem => ({ id: newId(), title: media.title, content: { kind: 'media', mediaId: media.id, options: defaultVideoOptions } });
export const captureItem = (source: CaptureSource): ServiceItem => ({ id: newId(), title: source.name, content: { kind: 'capture', source, options: defaultCaptureOptions } });
export const headerItem = (title: string): ServiceItem => ({ id: newId(), title, content: { kind: 'header' } });

export function withPartLyrics(song: SongItem, version: SongVersion, occurrence: number, lyrics: string): SongItem {
  const entries = entriesOf(song, version);
  const target = entries[occurrence];
  if (!target) return song;
  const same = (e: typeof target) => target.sectionId ? e.sectionId === target.sectionId : e.id === target.id;
  return { ...song, arrangement: entries.map(e => same(e) ? { ...e, variation: { ...e.variation, lyrics } } : e) };
}

export function copyItem(item: ServiceItem): ServiceItem {
  return { ...structuredClone(item), id: newId() };
}
