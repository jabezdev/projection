import { useEffect, useState } from 'react';
import { invoke } from './tauri';
import type { SongHeader, SongModel } from './models';
import { decodeSong, type SongDocument } from './song';

const cache = new Map<number, Promise<SongModel>>();
const load = (id: number) => {
  let pending = cache.get(id);
  if (!pending) {
    pending = invoke<SongModel>('get_song', { id });
    cache.set(id, pending);
    void pending.catch(() => cache.delete(id));
  }
  return pending;
};
/** Drops a cached song after it was saved or deleted. */
export const forgetSong = (id: number) => { cache.delete(id); };

/** Library songs' documents, for thumbnails and lyric search. Loads in the background. */
export function useSongDocs(songs: SongHeader[]) {
  const [docs, setDocs] = useState<Map<number, SongDocument>>(new Map());
  useEffect(() => {
    let disposed = false;
    void Promise.all(songs.map(song => load(song.id).then(model => [song.id, decodeSong(model.lyrics)] as const).catch(() => null)))
      .then(entries => { if (!disposed) setDocs(new Map(entries.filter(e => e !== null))); });
    return () => { disposed = true; };
  }, [songs]);
  return docs;
}
