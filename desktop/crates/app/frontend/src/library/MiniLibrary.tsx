import { useEffect, useRef, useState } from 'react';
import { Icon } from '../ui/Icons';
import { invoke, isTauri } from '../api/tauri';
import type { CaptureKind, CaptureSource, Media, SongHeader } from '../api/models';
import { emptySong, songSteps } from '../song/song';
import { MediaThumb } from './Thumbs';
import { SlidePreview } from '../song/SlidePreview';
import { useSongDocs } from '../song/useSongDocs';
import { DEMO_SOURCES } from '../demo/service';

export type LibraryPick = { kind: 'song'; id: number } | { kind: 'media'; media: Media } | { kind: 'capture'; source: CaptureSource };
type Filter = 'all' | 'song' | 'video' | 'image' | 'audio' | 'capture';
type Method = { name: string; system_picker: boolean };

export default function MiniLibrary({ top, where, songs, media, onPick, onImport, onSlides, onClose }: {
  top: number; where: string; songs: SongHeader[]; media: Media[];
  onPick: (pick: LibraryPick) => void; onImport: () => Promise<void>; onSlides: () => void; onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const docs = useSongDocs(songs);
  const sources = useCaptureSources(filter === 'capture' || filter === 'all');
  useEffect(() => {
    const away = (e: PointerEvent) => { const t = e.target as HTMLElement; if (!root.current?.contains(t) && !t.closest('.insert-gap, .add-item')) onClose(); };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape, true);
    return () => { document.removeEventListener('pointerdown', away); document.removeEventListener('keydown', escape, true); };
  }, [onClose]);
  const q = query.trim().toLowerCase();
  const songMatches = songs.filter(song => !q || song.title.toLowerCase().includes(q) || songSteps(docs.get(song.id) ?? emptySong()).some(text => text.toLowerCase().includes(q)));
  const mediaMatches = media.filter(m => !q || m.title.toLowerCase().includes(q));
  const captureMatches = sources.list.filter(s => !q || s.name.toLowerCase().includes(q));
  const show = (kind: Filter) => filter === 'all' || filter === kind;
  const counts: [Filter, string, number][] = [['all', 'All', songs.length + media.length + sources.list.length], ['song', 'Songs', songs.length], ['video', 'Videos', media.filter(m => m.kind === 'video').length], ['image', 'Images', media.filter(m => m.kind === 'image').length], ['audio', 'Audio', media.filter(m => m.kind === 'audio').length], ['capture', 'Captures', sources.list.length]];
  const empty = !(show('song') && songMatches.length) && !mediaMatches.some(m => show(m.kind)) && !(show('capture') && (captureMatches.length || sources.picker));
  return <div ref={root} className="mini-library" role="dialog" aria-label="Add an item" style={{ top }} data-covers-monitors="">
    <div className="mini-head"><input autoFocus aria-label="Search the library" placeholder="Search titles and lyrics…" value={query} onChange={e => setQuery(e.target.value)}/><small>Insert {where}</small></div>
    <div className="mini-chips" role="group" aria-label="Type">{counts.map(([value, label, count]) => <button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}<span>{count}</span></button>)}</div>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <div className="mini-grid">
      {show('song') && songMatches.map(song => { const doc = docs.get(song.id); return <button key={`s${song.id}`} className="mini-tile" onClick={() => onPick({ kind: 'song', id: song.id })}>
        <div className="thumb">{doc ? <SlidePreview text={songSteps(doc)[0] || ''} background={doc.background}/> : null}</div><strong>{song.title}</strong><span>Song</span></button>; })}
      {mediaMatches.filter(m => show(m.kind)).map(m => <button key={`m${m.id}`} className="mini-tile" onClick={() => onPick({ kind: 'media', media: m })}><MediaThumb kind={m.kind} path={m.path}/><strong>{m.title}</strong><span>{{ image: 'Image', video: 'Video', audio: 'Audio' }[m.kind]}</span></button>)}
      {show('capture') && captureMatches.map(s => <button key={`c${s.id}`} className="mini-tile" onClick={() => onPick({ kind: 'capture', source: s })}><div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>{s.kind === 'window' ? 'Window' : 'Screen'}</small></span></div><strong>{s.name}</strong><span>Screen capture</span></button>)}
      {show('capture') && sources.picker && (['screen', 'window'] as CaptureKind[]).map(kind => <button key={kind} className="mini-tile" onClick={() => void sources.pick(kind).then(s => { if (s) onPick({ kind: 'capture', source: s }); }).catch(e => setError(String(e)))}><div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>Choose…</small></span></div><strong>Choose a {kind}…</strong><span>Screen capture</span></button>)}
      {empty && <p className="mini-empty">{q ? 'Nothing matches. Song lyrics are searched too.' : 'Nothing here yet. Import files below, or add songs in the Library.'}</p>}
    </div>
    <div className="mini-foot">
      <button disabled={busy || !isTauri()} title={isTauri() ? 'Images, videos and audio' : 'Needs the desktop app'} onClick={() => { setBusy(true); setError(''); void onImport().catch(e => setError(String(e))).finally(() => setBusy(false)); }}><Icon name="plus"/>{busy ? 'Importing…' : 'Import files…'}</button>
      <button onClick={onSlides}><Icon name="plus"/>Slides from PDF or images…</button>
    </div>
  </div>;
}

function useCaptureSources(wanted: boolean) {
  const [state, setState] = useState<{ list: CaptureSource[]; picker: boolean }>({ list: isTauri() ? [] : DEMO_SOURCES, picker: false });
  useEffect(() => {
    if (!wanted || !isTauri()) return;
    let disposed = false;
    void invoke<{ current: string | null; methods: Method[] }>('get_capture_methods').then(async methods => {
      const current = methods.methods.find(m => m.name === methods.current);
      if (!current) return;
      if (current.system_picker) { if (!disposed) setState({ list: [], picker: true }); return; }
      const list = await invoke<CaptureSource[]>('list_capture_sources');
      if (!disposed) setState({ list, picker: false });
    }).catch(() => {});
    return () => { disposed = true; };
  }, [wanted]);
  return { ...state, pick: (kind: CaptureKind) => invoke<CaptureSource | null>('pick_capture_source', { kind }) };
}
