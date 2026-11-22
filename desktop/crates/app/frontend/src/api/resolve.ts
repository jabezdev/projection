import type { LiveService, MediaFile, Service, ServiceItem, SessionItem, SessionItemKind, Song, SongVersion } from './models';
import { resolveSong, slides } from '../song/song';

export type Catalog = { songs: Map<string, Song>; versions: Map<string, SongVersion>; media: Map<string, MediaFile> };

export function catalog(songs: Song[], media: MediaFile[], versions: SongVersion[] = []): Catalog {
  return { songs: new Map(songs.map(s => [s.id, s])), versions: new Map([...songs.map(s => [s.current.id, s.current] as const), ...versions.map(v => [v.id, v] as const)]), media: new Map(media.map(m => [m.id, m])) };
}

export function backgroundPath(value: string | undefined, media: Map<string, MediaFile> | MediaFile[]) {
  if (!value) return '';
  if (value.startsWith('shader:')) return value;
  const found = Array.isArray(media) ? media.find(m => m.id === value) : media.get(value);
  return found?.path ?? '';
}

export function resolveItem(item: ServiceItem, { songs, versions, media }: Catalog): SessionItem {
  const content = item.content;
  let kind: SessionItemKind;
  let song: SessionItem['song'];
  let background = item.background;
  let title = item.title ?? null;
  switch (content.kind) {
    case 'song': {
      const library = songs.get(content.songId);
      const version = content.versionId ? versions.get(content.versionId) : library?.current;
      if (version) {
        const sections = resolveSong(content, version);
        song = { song_id: content.songId, version_id: version.id, sections, title: library?.title ?? '', credits: library?.credits ?? {} };
        background ??= version.background;
        kind = { kind: 'text', data: { steps: sections.flatMap(slides) } };
      } else kind = { kind: 'text', data: { steps: [] } };
      title ??= library?.title ?? null;
      break;
    }
    case 'text': kind = { kind: 'text', data: { steps: content.steps } }; break;
    case 'media': {
      const file = media.get(content.mediaId);
      const path = file?.path ?? '';
      kind = file?.kind === 'video' || file?.kind === 'audio' ? { kind: file.kind, data: { path, options: content.options } } : { kind: 'image', data: { path } };
      break;
    }
    case 'presentation': kind = { kind: 'presentation', data: { path: content.slides[0] ?? (content.mediaId ? media.get(content.mediaId)?.path ?? '' : ''), slides: content.slides } }; break;
    case 'capture': kind = content.source ? { kind: 'capture', data: { source: content.source, options: content.options } } : { kind: 'text', data: { steps: [] } }; break;
    case 'header': kind = { kind: 'section', data: {} }; break;
  }
  const path = backgroundPath(background, media);
  return { id: item.id, title, kind, ...(song ? { song } : {}), ...(path ? { background: path } : {}) };
}

export function resolveService(service: Service, from: Catalog): LiveService {
  return { service, items: service.items.map(item => resolveItem(item, from)) };
}
