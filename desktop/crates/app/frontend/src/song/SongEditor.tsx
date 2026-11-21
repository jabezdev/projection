import { useEffect, useRef, useState } from 'react';
import { arrangedParts, emptySong, slides, type SongDocument } from './song';
import type { Media } from '../api/models';
import SongWorkspace from './SongWorkspace';

type Props = { title?: string; song?: SongDocument; backgrounds?: Media[]; heading?: string; onClose: () => void; onSave: (title: string, song: SongDocument) => Promise<void> };

export default function SongEditor({ title = '', song, backgrounds = [], heading = 'Library song', onClose, onSave }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const [name, setName] = useState(title);
  const [document, setDocument] = useState(song || emptySong());
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    const previous = globalThis.document.activeElement as HTMLElement | null;
    return () => previous?.focus();
  }, []);
  async function save() {
    if (!name.trim() || !document.arrangement.length || arrangedParts(document).some(p => !slides(p).length)) {
      setError('Name the song, add lyrics, and put at least one part in the arrangement.'); return;
    }
    setSaving(true); setError('');
    try { await onSave(name.trim(), document); } catch (e) { setError(String(e)); setSaving(false); }
  }
  return <div ref={dialog} className="song-editor" role="dialog" aria-modal="true" aria-label="Song editor" onKeyDown={e => {
    if (e.key === 'Escape' && !saving && !e.defaultPrevented) onClose();
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); void save(); }
  }}>
    <header className="song-editor-bar"><span className="eyebrow">{heading}</span><span className="song-editor-name">{name || 'Untitled song'}</span><button disabled={saving} onClick={onClose}>Cancel</button><button className="primary-action" disabled={saving} onClick={() => void save()}>{saving ? 'Saving…' : 'Save song'}</button></header>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <SongWorkspace title={name} onTitle={setName} song={document} onSong={setDocument} backgrounds={backgrounds} autoFocusTitle={!title}/>
  </div>;
}
