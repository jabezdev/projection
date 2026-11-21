import { newItemId, type SessionItem, type SongModel } from '../api/models';
import { decodeSong, songSteps, type SongDocument } from '../song/song';

export function songItem(song: SongModel): SessionItem {
  const doc: SongDocument = { ...decodeSong(song.lyrics), librarySongId: song.id, editedInService: false };
  return { id: newItemId(), title: song.title, song: JSON.stringify(doc), background: doc.background, kind: { kind: 'text', data: { steps: songSteps(doc) } } };
}
