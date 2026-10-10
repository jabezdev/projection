import { useVideoPoster } from './useVideoPoster';
import { useState } from 'react';
import { Icon } from './Icons';
import { invoke, isTauri } from './tauri';
import { mediaSource } from './media';
import type { Media, MediaKind, SongHeader } from './models';

function VideoPreview({ path }: { path: string }) {
  const poster = useVideoPoster(path);
  return <img src={poster || undefined} alt="Video thumbnail"/>;
}

type Props = { songs: SongHeader[]; media: Media[]; canAdd: boolean; onClose: () => void; onAddSong: (id: number) => Promise<void>; onEditSong: (id: number) => Promise<void>; onNewSong: () => void; onAddMedia: (media: Media) => Promise<void>; onNewPresentation: () => void; onImported: (media: Media) => void };
export default function LibraryPanel({ songs, media, canAdd, onClose, onAddSong, onEditSong, onNewSong, onAddMedia, onImported, onNewPresentation }: Props) {
  const [tab, setTab] = useState<'songs' | MediaKind>('songs');
  const [query, setQuery] = useState('');
  const [title, setTitle] = useState('');
  const [path, setPath] = useState('');
  const [selected, setSelected] = useState<Media | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function run(work: () => Promise<void>) { setBusy(true); setError(''); try { await work(); } catch (e) { setError(String(e)); } finally { setBusy(false); } }
  const filtered = media.filter(m => m.kind === tab && m.title.toLowerCase().includes(query.toLowerCase()));
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape' && !busy) onClose(); }}><section className="library-panel" role="dialog" aria-modal="true" aria-label="Library">
    <header className="surface-header"><div><small>LIBRARY</small><h2>Songs & media</h2></div><button className="icon-button" aria-label="Close library" onClick={onClose}><Icon name="close"/></button></header>
    <nav className="library-tabs" aria-label="Library categories">{([['songs', 'Songs'], ['video', 'Videos'], ['image', 'Images'], ['audio', 'Audio']] as const).map(([value, label]) => <button key={value} aria-pressed={tab === value} onClick={() => { setTab(value); setSelected(null); setError(''); }}>{label}<span>{value === 'songs' ? songs.length : media.filter(m => m.kind === value).length}</span></button>)}</nav>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <div className="library-search"><button disabled={!canAdd} onClick={onNewPresentation}><Icon name="plus"/> Image presentation</button><input autoFocus aria-label="Search library" placeholder="Search library…" value={query} onChange={e => setQuery(e.target.value)}/>{tab === 'songs' && <button className="primary-action" onClick={onNewSong}><Icon name="plus"/> New song</button>}</div>
    <div className={`library-body ${tab === 'songs' ? 'songs-only' : ''}`}><div className="library-results">
      {tab === 'songs' ? songs.filter(s => s.title.toLowerCase().includes(query.toLowerCase())).map(song => <div className="library-entry" key={song.id}><span>{song.title}</span><button className="icon-button" aria-label={`Edit ${song.title}`} disabled={busy} onClick={() => void run(() => onEditSong(song.id))}><Icon name="edit"/></button><button className="library-add" disabled={!canAdd || busy} onClick={() => void run(() => onAddSong(song.id))}><Icon name="plus"/> Add to service</button></div>) : filtered.map(item => <div className={`library-entry ${selected?.id === item.id ? 'is-selected' : ''}`} key={item.id}><button className="media-select" onClick={() => setSelected(item)}><Icon name={item.kind === 'image' ? 'logo' : item.kind}/><span>{item.title}<small>{item.path.split(/[\\/]/).pop()}</small></span></button><button className="library-add" disabled={!canAdd || busy} onClick={() => void run(() => onAddMedia(item))}><Icon name="plus"/> Add to service</button></div>)}
      {(tab === 'songs' ? !songs.filter(s => s.title.toLowerCase().includes(query.toLowerCase())).length : !filtered.length) && <p className="empty-panel">{query ? 'No matching items.' : tab === 'songs' ? 'Create a song to start your library.' : 'Import a file to start this collection.'}</p>}
    </div>{tab !== 'songs' && <aside className="library-import">
      {selected && <section className="media-inspector"><h3>{selected.title}</h3>{selected.kind === 'image' ? <img src={mediaSource(selected.path)} alt={selected.title}/> : selected.kind === 'video' ? <VideoPreview key={selected.id} path={selected.path}/> : <p className="media-inspector-file"><Icon name="audio"/>{selected.path.split(/[\\/]/).pop()}<small>Add to a service and cue it to play on the output audio device.</small></p>}</section>}
      <form onSubmit={e => { e.preventDefault(); void run(async () => { const imported = await invoke<Media>('create_media', { title: title.trim(), kind: tab, path: path.trim() }); onImported(imported); setSelected(imported); setTitle(''); setPath(''); }); }}><h3>Import {tab}</h3>{isTauri() && <button type="button" className="browse-media" disabled={busy} onClick={() => void run(async () => { const file = await invoke<string | null>('choose_media_file', { kind: tab }); if (file) { setPath(file); if (!title.trim()) setTitle((file.split(/[\\/]/).pop() || file).replace(/\.[^.]+$/, '')); } })}>Browse files…</button>}<label>Title<input aria-label="Media title" required value={title} onChange={e => setTitle(e.target.value)} placeholder="Display name"/></label><label>File path<input aria-label="Media file path" required value={path} onChange={e => setPath(e.target.value)} placeholder={tab === 'video' ? '/media/welcome.mp4' : tab === 'audio' ? '/media/instrumental.mp3' : '/media/background.jpg'}/></label><small>Choose a file or enter its path. The library references your original file. Keep it in its current location.</small><button className="primary-action" disabled={busy || !title.trim() || !path.trim()}>{busy ? 'Importing…' : 'Import to library'}</button></form>
    </aside>}</div>
    {!canAdd && <p className="library-footnote">Create or select a service to add library items to its schedule.</p>}
  </section></div>;
}
