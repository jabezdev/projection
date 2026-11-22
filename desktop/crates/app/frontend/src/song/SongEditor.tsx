import { useEffect, useRef, useState } from 'react';
import { arranged, emptySong, slides, type SongDraft } from './song';
import type { Credits, MediaFile, Song } from '../api/models';
import { authorsText, parseAuthors } from './credits';
import SongWorkspace from './SongWorkspace';
import { editHistory, historyKey, stepEdit, type History } from '../api/history';
import HistoryButtons from '../ui/HistoryButtons';

type Props = { song?: Song; backgrounds?: MediaFile[]; heading?: string; onClose: () => void; onSave: (song: Song) => Promise<void>; onStepped?: () => Promise<void> };

export default function SongEditor({ song, backgrounds = [], heading = 'Library song', onClose, onSave, onStepped }: Props) {
  const [initial, setInitial] = useState(song ?? emptySong());
  const dialog = useRef<HTMLDivElement>(null);
  const [name, setName] = useState(initial.title);
  const [draft, setDraft] = useState<SongDraft>(initial.current);
  const [credits, setCredits] = useState<Credits>(initial.credits);
  const [authors, setAuthors] = useState(authorsText(initial.credits.authors));
  const [key, setKey] = useState(initial.key ?? '');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const [history, setHistory] = useState<History>({});
  const [dirty, setDirty] = useState(false);
  const [revision, setRevision] = useState(0);
  const changed = <T,>(set: (value: T) => void) => (value: T) => { set(value); setDirty(true); };
  useEffect(() => { if (song) void editHistory(song.id).then(setHistory, () => {}); }, [song]);
  async function step(redo: boolean) {
    if (!song || dirty || saving) return;
    setSaving(true); setError('');
    try {
      const result = await stepEdit('song', song.id, redo);
      setHistory(result.history);
      const next = result.song;
      if (next) {
        setInitial(next); setName(next.title); setDraft(next.current); setCredits(next.credits); setAuthors(authorsText(next.credits.authors)); setKey(next.key ?? '');
        setRevision(r => r + 1);
        await onStepped?.();
      }
    } catch (e) { setError(String(e)); } finally { setSaving(false); }
  }
  useEffect(() => {
    const previous = globalThis.document.activeElement as HTMLElement | null;
    return () => previous?.focus();
  }, []);
  async function save() {
    if (!name.trim() || !draft.arrangement.length || arranged(draft).some(p => !slides(p).length)) {
      setError('Name the song, add lyrics, and put at least one part in the arrangement.'); return;
    }
    setSaving(true); setError('');
    const parsed = parseAuthors(authors);
    const full: Song = { ...initial, title: name.trim(), key: key.trim() || undefined, credits: { ...credits, authors: parsed.length ? parsed : undefined }, current: { ...initial.current, ...draft } };
    try { await onSave(full); } catch (e) { setError(String(e)); setSaving(false); }
  }
  const text = (field: 'copyright' | 'ccli' | 'publisher') => (value: string) => { setDirty(true); setCredits(c => ({ ...c, [field]: value.trim() ? value : undefined })); };
  const details = <div className="song-credits">
    <label className="song-field">Authors<input aria-label="Authors" placeholder="John Newton (words), …" value={authors} onChange={e => changed(setAuthors)(e.target.value)}/></label>
    <div className="song-field-row">
      <label className="song-field">CCLI number<input aria-label="CCLI number" inputMode="numeric" value={credits.ccli ?? ''} onChange={e => text('ccli')(e.target.value)}/></label>
      <label className="song-field">Key<input aria-label="Key" placeholder="G" value={key} onChange={e => changed(setKey)(e.target.value)}/></label>
    </div>
    <label className="song-field">Copyright<input aria-label="Copyright" placeholder="© 2006 sixsteps Music" value={credits.copyright ?? ''} onChange={e => text('copyright')(e.target.value)}/></label>
    <div className="song-field-row">
      <label className="song-field">Publisher<input aria-label="Publisher" value={credits.publisher ?? ''} onChange={e => text('publisher')(e.target.value)}/></label>
      <label className="song-field">Year<input aria-label="Year" inputMode="numeric" value={credits.year ?? ''} onChange={e => { const year = Number(e.target.value); setDirty(true); setCredits(c => ({ ...c, year: e.target.value && Number.isInteger(year) ? year : undefined })); }}/></label>
    </div>
  </div>;
  return <div ref={dialog} className="song-editor" role="dialog" aria-modal="true" aria-label="Song editor" onKeyDown={e => {
    if (e.key === 'Escape' && !saving && !e.defaultPrevented) onClose();
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); void save(); }
    const key = song && historyKey(e);
    if (key) { e.preventDefault(); void step(key === 'redo'); }
  }}>
    <header className="song-editor-bar"><span className="eyebrow">{heading}</span><span className="song-editor-name">{name || 'Untitled song'}</span>{song && <HistoryButtons history={history} busy={saving} blocked={dirty ? 'Save or cancel your changes to step through saved edits' : undefined} onStep={redo => void step(redo)}/>}<button disabled={saving} onClick={onClose}>Cancel</button><button className="primary-action" disabled={saving} onClick={() => void save()}>{saving ? 'Saving…' : 'Save song'}</button></header>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <SongWorkspace key={revision} title={name} onTitle={changed(setName)} song={draft} onSong={changed(setDraft)} backgrounds={backgrounds} details={details} autoFocusTitle={!song}/>
  </div>;
}
