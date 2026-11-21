import { useEffect, useMemo, useState } from 'react';
import { Icon } from '../ui/Icons';
import { invoke, isTauri } from '../api/tauri';
import { mediaSource } from './media';
import type { MediaFile, MediaKind, Service, Song } from '../api/models';
import { arranged, draftSteps, sectionCode } from '../song/song';
import { backgroundPath } from '../api/resolve';
import { authorsText } from '../song/credits';
import { MediaThumb } from './Thumbs';
import { SlidePreview } from '../song/SlidePreview';
import { isVideoPath } from '../outputs/preview';
import { useVideoPoster } from './useVideoPoster';

type Filter = 'all' | 'song' | Exclude<MediaKind, 'presentation'>;
type Entry = { kind: 'song'; song: Song } | { kind: 'media'; media: MediaFile };
type MissingMedia = { path: string; title: string | null; services: string[] };
type Props = {
  songs: Song[];
  media: MediaFile[];
  onClose: () => void;
  onCatalogChanged: () => Promise<void>;
  onNewSong: () => void;
  onEditSong: (id: string) => void;
  confirm: (message: string, action: string) => Promise<boolean>;
};
const key = (entry: Entry) => entry.kind === 'song' ? `s${entry.song.id}` : `m${entry.media.id}`;

export default function LibraryModal({ songs, media: allMedia, onClose, onCatalogChanged, onNewSong, onEditSong, confirm }: Props) {
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [missing, setMissing] = useState<MissingMedia[]>([]);
  const [services, setServices] = useState<Service[]>([]);
  const media = useMemo(() => allMedia.filter(m => m.kind !== 'presentation'), [allMedia]);
  useEffect(() => { if (isTauri()) void invoke<MissingMedia[]>('list_missing_media').then(setMissing).catch(() => {}); }, []);
  useEffect(() => {
    let disposed = false;
    void invoke<Service[]>('list_services').then(all => { if (!disposed) setServices(all); }).catch(() => {});
    return () => { disposed = true; };
  }, []);
  async function run(work: () => Promise<void>) { setBusy(true); setError(''); setNotice(''); try { await work(); } catch (e) { setError(String(e)); } finally { setBusy(false); } }

  const q = query.trim().toLowerCase();
  const entries: Entry[] = useMemo(() => [
    ...songs.filter(s => !q || s.title.toLowerCase().includes(q) || draftSteps(s.current).some(t => t.toLowerCase().includes(q))).map(song => ({ kind: 'song' as const, song })),
    ...media.filter(m => !q || m.title.toLowerCase().includes(q)).map(m => ({ kind: 'media' as const, media: m })),
  ].filter(e => filter === 'all' || (e.kind === 'song' ? filter === 'song' : e.media.kind === filter)), [songs, media, q, filter]);
  const current = [...songs.map(song => ({ kind: 'song' as const, song })), ...media.map(m => ({ kind: 'media' as const, media: m }))].find(e => key(e) === selected) ?? null;
  const counts: [Filter, string, number][] = [['all', 'All', songs.length + media.length], ['song', 'Songs', songs.length], ['video', 'Videos', media.filter(m => m.kind === 'video').length], ['image', 'Images', media.filter(m => m.kind === 'image').length], ['audio', 'Audio', media.filter(m => m.kind === 'audio').length]];

  function usedIn(entry: Entry) {
    return services.filter(service => service.items.some(item => {
      const content = item.content;
      if (entry.kind === 'song') return content.kind === 'song' && content.songId === entry.song.id;
      const id = entry.media.id;
      return (content.kind === 'media' && content.mediaId === id) || (content.kind === 'presentation' && !!entry.media.path && content.slides.includes(entry.media.path)) || item.background === id;
    })).map(s => s.title);
  }
  async function importFiles() {
    const imported = await invoke<MediaFile[]>('import_media_files');
    if (!imported.length) return;
    await onCatalogChanged();
    setNotice(`Imported ${imported.length} file${imported.length === 1 ? '' : 's'}.`);
    setSelected(`m${imported[0].id}`);
  }
  async function importOpenLyrics() {
    const result = await invoke<{ imported: Song[]; failed: string[] }>('import_openlyrics');
    if (result.imported.length) await onCatalogChanged();
    setNotice(result.imported.length ? `Imported ${result.imported.length} song${result.imported.length === 1 ? '' : 's'}.` : '');
    if (result.failed.length) setError(`Not imported: ${result.failed.join('; ')}`);
  }
  async function relink() {
    const result = await invoke<{ found: number; missing: number } | null>('relink_media');
    if (!result) return;
    setMissing(await invoke<MissingMedia[]>('list_missing_media'));
    await onCatalogChanged();
    setNotice(result.found ? `Relinked ${result.found} file${result.found === 1 ? '' : 's'}.${result.missing ? ` ${result.missing} still missing.` : ''}` : 'No missing files were found in that folder.');
  }
  async function remove(entry: Entry) {
    const used = usedIn(entry);
    const name = entry.kind === 'song' ? entry.song.title : entry.media.title;
    const message = entry.kind === 'song'
      ? `Delete “${name}” from the library?${used.length ? ` Services that use it (${used.join(', ')}) keep showing it.` : ''}`
      : `Remove “${name}” from the library?${used.length ? ` It is used in ${used.join(', ')}; those keep working, since the file itself stays.` : ''}`;
    if (!(await confirm(message, 'Delete'))) return;
    await run(async () => {
      if (entry.kind === 'song') await invoke('archive_song', { id: entry.song.id, archived: true });
      else await invoke('delete_media', { id: entry.media.id });
      setSelected(null);
      await onCatalogChanged();
      setNotice(`Deleted ${name}.`);
    });
  }

  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape' && !busy && !e.defaultPrevented) onClose(); }}>
    <section className="library-modal" role="dialog" aria-modal="true" aria-label="Library">
      <header className="library-modal-head">
        <h2>Library</h2>
        <div className="library-search-box"><Icon name="search"/><input autoFocus aria-label="Search the library" placeholder="Search titles and lyrics…" value={query} onChange={e => setQuery(e.target.value)}/></div>
        <span className="grow"/>
        <button className="primary-action" onClick={onNewSong}><Icon name="plus"/>New song</button>
        <button disabled={busy || !isTauri()} title={isTauri() ? 'Images, videos and audio' : 'Needs the desktop app'} onClick={() => void run(importFiles)}><Icon name="import"/>Import files…</button>
        {isTauri() && <button disabled={busy} onClick={() => void run(importOpenLyrics)}>Import OpenLyrics…</button>}
        <button className="icon-button" aria-label="Close library" onClick={onClose}><Icon name="close"/></button>
      </header>
      <nav className="library-filters" aria-label="Type">{counts.map(([value, label, count]) => <button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}<span>{count}</span></button>)}</nav>
      {missing.length > 0 && <div className="library-banner" role="status"><span>{missing.length} file{missing.length === 1 ? ' is' : 's are'} missing (moved, or on another computer).</span><button disabled={busy} onClick={() => void run(relink)}>{busy ? 'Searching…' : 'Find them in a folder…'}</button></div>}
      {error && <p className="ui-error" role="alert">{error}</p>}
      {notice && <p className="library-notice" role="status">{notice}</p>}
      <div className={`library-modal-body ${current ? 'has-detail' : ''}`}>
        <div className="library-grid" role="list">
          {entries.map(entry => { const k = key(entry); return <button key={k} role="listitem" className="library-tile" aria-pressed={selected === k} onClick={() => setSelected(selected === k ? null : k)} onDoubleClick={() => { if (entry.kind === 'song') onEditSong(entry.song.id); }}>
            {entry.kind === 'song' ? <div className="thumb"><SlidePreview text={draftSteps(entry.song.current)[0] || ''} background={backgroundPath(entry.song.current.background, media)}/></div> : <MediaThumb kind={entry.media.kind} path={entry.media.path ?? ''}/>}
            <strong>{entry.kind === 'song' ? entry.song.title : entry.media.title}</strong>
            <span>{entry.kind === 'song' ? `Song · ${arranged(entry.song.current).map(sectionCode).join(' ')}` : kindName[entry.media.kind]}</span>
          </button>; })}
          {!entries.length && <p className="empty-panel">{q ? 'Nothing matches. Song lyrics are searched too.' : filter === 'song' || filter === 'all' ? 'Your library is empty. Create a song or import files.' : 'Nothing of this type yet. Import files to add some.'}</p>}
        </div>
        {current && <Details key={key(current)} entry={current} usedIn={usedIn(current)} busy={busy} media={media}
          onEdit={() => current.kind === 'song' && onEditSong(current.song.id)}
          onRename={title => run(async () => { if (current.kind !== 'media') return; await invoke('rename_media', { id: current.media.id, title }); await onCatalogChanged(); })}
          onDelete={() => void remove(current)}/>}
      </div>
    </section>
  </div>;
}

const kindName: Record<MediaKind, string> = { image: 'Image', video: 'Video', audio: 'Audio', presentation: 'Slides' };

function Details({ entry, usedIn, media, busy, onEdit, onRename, onDelete }: { entry: Entry; usedIn: string[]; media: MediaFile[]; busy: boolean; onEdit: () => void; onRename: (title: string) => Promise<void>; onDelete: () => void }) {
  const [title, setTitle] = useState(entry.kind === 'song' ? entry.song.title : entry.media.title);
  const original = entry.kind === 'song' ? entry.song.title : entry.media.title;
  return <aside className="library-detail" aria-label="Details">
    <div className="library-detail-preview">{entry.kind === 'song' ? <SlidePreview text={draftSteps(entry.song.current)[0] || ''} background={backgroundPath(entry.song.current.background, media)}/> : <MediaPreview media={entry.media}/>}</div>
    {entry.kind === 'media'
      ? <form className="library-rename" onSubmit={e => { e.preventDefault(); if (title.trim() && title.trim() !== original) void onRename(title.trim()); }}><label>Title<input aria-label="Title" value={title} onChange={e => setTitle(e.target.value)}/></label>{title.trim() !== original && <button className="primary-action" disabled={busy || !title.trim()}>Rename</button>}</form>
      : <h3>{original}</h3>}
    <dl className="library-facts">
      <dt>Type</dt><dd>{entry.kind === 'song' ? 'Song' : kindName[entry.media.kind]}</dd>
      {entry.kind === 'song' && <>
        {!!entry.song.credits.authors?.length && <><dt>Authors</dt><dd>{authorsText(entry.song.credits.authors)}</dd></>}
        {entry.song.credits.ccli && <><dt>CCLI</dt><dd>{entry.song.credits.ccli}</dd></>}
        {entry.song.credits.copyright && <><dt>Copyright</dt><dd>{entry.song.credits.copyright}</dd></>}
        <dt>Arrangement</dt><dd>{arranged(entry.song.current).map(sectionCode).join(' ') || 'None yet'}</dd>
      </>}
      {entry.kind === 'media' && <><dt>File</dt><dd className="library-path">{entry.media.path?.split(/[\\/]/).pop() ?? 'Missing'}</dd></>}
      <dt>Used in</dt><dd>{usedIn.length ? usedIn.join(', ') : 'No services'}</dd>
    </dl>
    <div className="library-detail-actions">
      {entry.kind === 'song' && <button className="primary-action" onClick={onEdit}><Icon name="edit"/>Edit song</button>}
      <button className="danger-action" disabled={busy} onClick={onDelete}><Icon name="trash"/>Delete</button>
    </div>
  </aside>;
}

function MediaPreview({ media }: { media: MediaFile }) {
  const path = media.path ?? '';
  const poster = useVideoPoster(media.kind === 'video' ? path : '');
  if (media.kind === 'audio') return <div className="library-audio"><Icon name="audio"/><audio controls src={mediaSource(path)} preload="none"/></div>;
  if (media.kind === 'video' || isVideoPath(path)) return <video controls muted src={mediaSource(path)} poster={poster || undefined} preload="metadata"/>;
  return <MediaThumb kind="image" path={path}/>;
}
