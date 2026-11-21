import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Icon } from '../ui/Icons';
import { invoke } from '../api/tauri';
import { defaultCaptureOptions, defaultVideoOptions, isPast, type ItemContent, type MediaFile, type Service, type ServiceItem, type Song, type SongItem, type SongVersion, type VideoOptions } from '../api/models';
import { fromServiceDraft, serviceDraft, splitSlides, type SongDraft } from '../song/song';
import { captureItem, copyItem, headerItem, mediaItem, songItem } from './serviceItems';
import { catalog, resolveItem } from '../api/resolve';
import SongWorkspace from '../song/SongWorkspace';
import MiniLibrary, { type LibraryPick } from '../library/MiniLibrary';
import PresentationEditor from './PresentationEditor';
import { CaptureSettings } from './CaptureEditor';
import { ItemThumb, MediaThumb } from '../library/Thumbs';
import { itemSummary } from '../outputs/preview';
import { Background } from '../song/SlidePreview';
type Props = {
  service: Service;
  persist: (service: Service) => Promise<void>;
  songs: Song[];
  media: MediaFile[];
  initialItemId?: string;
  activeItemId?: string | null;
  confirmRemove?: (title: string) => Promise<boolean>;
  onCatalogChanged: () => Promise<void>;
  onClose: () => void;
};
type Toast = { text: string; undo?: ServiceItem[] };
type Change = (item: ServiceItem) => ServiceItem;
const SAVE_DELAY = 350;
const TOP = '';

const localInput = (ms: number) => { const d = new Date(ms); d.setMinutes(d.getMinutes() - d.getTimezoneOffset()); return d.toISOString().slice(0, 16); };

export default function ServiceEditor({ service, persist, songs, media, initialItemId, activeItemId, confirmRemove, onCatalogChanged, onClose }: Props) {
  const [name, setName] = useState(service.title);
  const [scheduledAtMs, setScheduledAtMs] = useState(service.scheduledAtMs);
  const [items, setItems] = useState(service.items);
  const [selected, setSelected] = useState<string | null>(initialItemId ?? service.items[0]?.id ?? null);
  const [insert, setInsert] = useState<{ after: string; top: number } | null>(null);
  const [slides, setSlides] = useState<{ after: string } | { item: ServiceItem } | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [error, setError] = useState('');
  const [dragged, setDragged] = useState<string | null>(null);
  const [dropAfter, setDropAfter] = useState<string | null>(null);
  const versions = usePinnedVersions(items, songs);
  const library = useMemo(() => catalog(songs, media, versions), [songs, media, versions]);
  const body = useRef<HTMLDivElement>(null);
  const latest = useRef({ name, scheduledAtMs, items });
  const timer = useRef<number | undefined>(undefined);
  const toastTimer = useRef<number | undefined>(undefined);

  const flush = useCallback(() => {
    window.clearTimeout(timer.current); timer.current = undefined;
    const { name, scheduledAtMs, items } = latest.current;
    return persist({ ...service, title: name.trim() || service.title, scheduledAtMs, items })
      .then(() => { setError(''); return true; }, e => { setError(String(e)); return false; });
  }, [persist, service]);
  const save = useCallback((next: { name?: string; scheduledAtMs?: number; items?: ServiceItem[] }, delay = SAVE_DELAY) => {
    latest.current = { ...latest.current, ...next };
    if (next.items) setItems(next.items);
    if (next.name !== undefined) setName(next.name);
    if (next.scheduledAtMs !== undefined) setScheduledAtMs(next.scheduledAtMs);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void flush(), delay);
  }, [flush]);
  useEffect(() => () => { if (timer.current !== undefined) void flush(); window.clearTimeout(toastTimer.current); }, [flush]);
  function showToast(next: Toast) { setToast(next); window.clearTimeout(toastTimer.current); toastTimer.current = window.setTimeout(() => setToast(null), 6000); }
  async function close() {
    if (!(await flush())) return;
    onClose();
  }

  const updateItem = (id: string, change: Change, delay?: number) =>
    save({ items: latest.current.items.map(item => item.id === id ? change(item) : item) }, delay);
  const titleOf = (item?: ServiceItem) => item ? resolveItem(item, library).title || 'item' : 'item';
  function place(item: ServiceItem, after: string) {
    const before = latest.current.items;
    const at = after === TOP ? 0 : before.findIndex(i => i.id === after) + 1 || before.length;
    save({ items: [...before.slice(0, at), item, ...before.slice(at)] }, 0);
    setSelected(item.id); setInsert(null);
    showToast({ text: `Added ${titleOf(item)}`, undo: before });
  }
  function pick(choice: LibraryPick, after: string) {
    setError('');
    if (choice.kind === 'song') place(songItem(choice.song), after);
    else if (choice.kind === 'media') place(mediaItem(choice.media), after);
    else place(captureItem(choice.source), after);
  }
  async function remove(item: ServiceItem) {
    if (item.id === activeItemId && confirmRemove && !(await confirmRemove(titleOf(item)))) return;
    const before = latest.current.items;
    const index = before.findIndex(i => i.id === item.id);
    const next = before.filter(i => i.id !== item.id);
    save({ items: next }, 0);
    if (selected === item.id) setSelected((next[index] ?? next[index - 1])?.id ?? null);
    showToast({ text: `Removed ${titleOf(item)}`, undo: before });
  }
  function duplicate(item: ServiceItem) {
    const before = latest.current.items;
    const copy = copyItem(item);
    const at = before.findIndex(i => i.id === item.id) + 1;
    save({ items: [...before.slice(0, at), copy, ...before.slice(at)] }, 0);
    setSelected(copy.id);
    showToast({ text: `Duplicated ${titleOf(item)}`, undo: before });
  }
  function move(id: string, after: string) {
    const before = latest.current.items;
    if (id === after) return;
    const moving = before.find(i => i.id === id);
    if (!moving) return;
    const rest = before.filter(i => i.id !== id);
    const at = after === TOP ? 0 : rest.findIndex(i => i.id === after) + 1;
    save({ items: [...rest.slice(0, at), moving, ...rest.slice(at)] }, 0);
  }
  function addSection(after: string) {
    place(headerItem('New section'), after);
    requestAnimationFrame(() => document.querySelector<HTMLInputElement>('.section-editor input')?.select());
  }
  function undo() { if (toast?.undo) { save({ items: toast.undo }, 0); setToast(null); } }
  function openInsert(after: string, anchor: HTMLElement) {
    const box = body.current?.getBoundingClientRect();
    const at = anchor.getBoundingClientRect();
    const top = box ? Math.max(8, Math.min(at.top - box.top - 40, box.height - 480)) : 8;
    setInsert(current => current?.after === after ? null : { after, top });
  }
  const title = (id: string) => titleOf(items.find(i => i.id === id));
  const where = insert ? insert.after === TOP ? 'at the top' : `after “${title(insert.after)}”` : '';
  const item = items.find(i => i.id === selected) ?? null;
  const resolved = items.map(i => resolveItem(i, library));
  const past = isPast({ ...service, scheduledAtMs });
  const gap = (after: string) => <div className={`insert-gap ${insert?.after === after ? 'is-open' : ''} ${dropAfter === after ? 'is-drop' : ''}`}
    onDragOver={e => { if (dragged === null) return; e.preventDefault(); setDropAfter(after); }}
    onDrop={e => { e.preventDefault(); if (dragged !== null) move(dragged, after); setDragged(null); setDropAfter(null); }}>
    <button aria-label={after === TOP ? 'Insert at the top' : `Insert after ${title(after)}`} onClick={e => openInsert(after, e.currentTarget)}><span>+ insert here</span></button>
    <button className="insert-section" aria-label={after === TOP ? 'Add a section at the top' : `Add a section after ${title(after)}`} onClick={() => addSection(after)}>+ section</button>
  </div>;

  return <div className="modal-backdrop" onKeyDown={e => {
    if (e.key === 'Escape' && !e.defaultPrevented && !insert && !slides) void close();
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && toast?.undo && !(e.target as HTMLElement).closest('input, textarea')) { e.preventDefault(); undo(); }
  }}><section className="service-editor" role="dialog" aria-modal="true" aria-label="Edit service">
    <header className="service-editor-head">
      <input aria-label="Service name" value={name} placeholder="Service name" onChange={e => save({ name: e.target.value })}/>
      <input type="datetime-local" aria-label="Service date and time" value={localInput(scheduledAtMs)} onChange={e => { const ms = new Date(e.target.value).getTime(); if (Number.isFinite(ms)) save({ scheduledAtMs: ms }); }}/>
      <span className="grow"/>
      <span className="saved-note">{past ? 'Past service · songs keep the words they had' : 'Changes save as you go'}</span>
      <button className="primary-action" onClick={() => void close()}>Done</button>
    </header>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <div className="service-editor-body" ref={body}>
      <section className="service-items" aria-label="Items">
        <div className="panel-heading">Items <small>{items.length} · drag to reorder</small></div>
        <div className="service-items-scroll">
          {gap(TOP)}
          {items.map((entry, index) => { const shown = resolved[index]; return <div key={entry.id}>
            <div className={`service-item ${entry.content.kind === 'header' ? 'is-section' : ''} ${entry.id === selected ? 'is-selected' : ''} ${dragged === entry.id ? 'is-dragging' : ''}`} draggable tabIndex={0} role="button" aria-pressed={entry.id === selected}
              onClick={() => setSelected(entry.id)} onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSelected(entry.id); } if (e.key === 'Delete') void remove(entry); }}
              onDragStart={e => { setDragged(entry.id); e.dataTransfer.setData('text/plain', entry.id); e.dataTransfer.effectAllowed = 'move'; }}
              onDragEnd={() => { setDragged(null); setDropAfter(null); }}
              onDragOver={e => { if (dragged === null) return; e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); setDropAfter(e.clientY < r.top + r.height / 2 ? (items[index - 1]?.id ?? TOP) : entry.id); }}
              onDrop={e => { e.preventDefault(); if (dragged !== null && dropAfter !== null) move(dragged, dropAfter); setDragged(null); setDropAfter(null); }}>
              {entry.content.kind === 'header'
                ? <span className="service-section-name"><strong>{entry.title || 'Untitled section'}</strong><small>{sectionSize(items, index)} items</small></span>
                : <><span className="service-item-index">{items.slice(0, index + 1).filter(i => i.content.kind !== 'header').length}</span><ItemThumb item={shown}/><span className="service-item-text"><strong>{shown.title || 'Untitled'}</strong><small>{itemSummary(shown)}{entry.content.kind === 'song' && edited(entry) ? ' · edited here' : ''}</small></span></>}
              <span className="service-item-tools">
                <button className="icon-button" aria-label={`Duplicate ${shown.title}`} title="Duplicate" onClick={e => { e.stopPropagation(); duplicate(entry); }}><Icon name="copy"/></button>
                <button className="icon-button" aria-label={`Remove ${shown.title}`} title="Remove (Delete)" onClick={e => { e.stopPropagation(); void remove(entry); }}><Icon name="close"/></button>
              </span>
            </div>
            {gap(entry.id)}
          </div>; })}
          {!items.length && <p className="empty-panel">This service is empty. Add the first item.</p>}
          <div className="add-row"><button className="add-item" onClick={e => openInsert(items.at(-1)?.id ?? TOP, e.currentTarget)}><Icon name="plus"/>Add item</button><button className="add-item" onClick={() => addSection(items.at(-1)?.id ?? TOP)}><Icon name="plus"/>Add section</button></div>
        </div>
      </section>
      <section className="service-item-editor" aria-label="Selected item">
        {item ? <ItemEditor key={item.id} item={item} songs={songs} versions={library.versions} media={media} onChange={(change, delay) => updateItem(item.id, change, delay)} onEditSlides={() => setSlides({ item })}/> : <p className="empty-panel">Select an item to edit it here.</p>}
      </section>
      {insert && <MiniLibrary top={insert.top} where={where} songs={songs} media={media} onClose={() => setInsert(null)}
        onPick={choice => void pick(choice, insert.after)}
        onImport={async () => { const imported = await invoke<MediaFile[]>('import_media_files'); if (imported.length) await onCatalogChanged(); }}
        onSlides={() => { setSlides({ after: insert.after }); setInsert(null); }}/>}
    </div>
    {toast && <div className="editor-toast" role="status">{toast.text}{toast.undo && <button onClick={undo}>Undo</button>}</div>}
  </section>
  {slides && <PresentationEditor images={media} item={'item' in slides ? slides.item : undefined} onClose={() => setSlides(null)} onSave={async saved => {
    if ('item' in slides) updateItem(saved.id, () => saved, 0); else place(saved, slides.after);
    setSlides(null);
  }}/>}
  </div>;
}

function sectionSize(items: ServiceItem[], index: number) {
  let count = 0;
  for (let i = index + 1; i < items.length && items[i].content.kind !== 'header'; i++) count++;
  return count;
}

const edited = (item: ServiceItem) => item.content.kind === 'song' && (!!item.content.arrangement || item.title !== undefined || item.background !== undefined);

function usePinnedVersions(items: ServiceItem[], songs: Song[]) {
  const [versions, setVersions] = useState<SongVersion[]>([]);
  const wanted = items.flatMap(i => i.content.kind === 'song' && i.content.versionId ? [i.content.versionId] : []).filter(id => !songs.some(s => s.current.id === id)).sort().join(' ');
  useEffect(() => {
    if (!wanted) return;
    let disposed = false;
    void Promise.all(wanted.split(' ').map(id => invoke<SongVersion>('get_song_version', { id }).catch(() => null)))
      .then(all => { if (!disposed) setVersions(all.filter((v): v is SongVersion => !!v)); });
    return () => { disposed = true; };
  }, [wanted]);
  return versions;
}

type EditorProps = { item: ServiceItem; songs: Song[]; versions: Map<string, SongVersion>; media: MediaFile[]; onChange: (change: Change, delay?: number) => void; onEditSlides: () => void };

function ItemEditor({ item, songs, versions, media, onChange, onEditSlides }: EditorProps) {
  const content = item.content;
  const setTitle = (title: string) => onChange(i => ({ ...i, title }));
  const setContent = (change: (content: ItemContent) => ItemContent, delay?: number) => onChange(i => ({ ...i, content: change(i.content) }), delay);
  if (content.kind === 'song') return <SongItemEditor item={item} song={content} songs={songs} versions={versions} media={media} onChange={onChange}/>;
  const titleField = <label className="item-field item-title">Title<input aria-label="Item title" value={item.title || ''} onChange={e => setTitle(e.target.value)}/></label>;
  if (content.kind === 'text') return <ItemLayout preview={<div className="thumb"><span className="thumb-tile"><small>Text</small></span></div>} kindLabel="Text" title={titleField}>
    <label className="item-field">Slides <small>A blank line starts a new slide</small><textarea aria-label="Text slides" rows={8} defaultValue={content.steps.join('\n\n')} onChange={e => { const steps = splitSlides(e.target.value); setContent(c => c.kind === 'text' ? { ...c, steps } : c); }}/></label>
  </ItemLayout>;
  if (content.kind === 'capture') {
    const source = content.source;
    return <ItemLayout preview={<div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>{source?.name ?? 'Choose a source'}</small></span></div>} kindLabel="Screen capture" title={titleField}>
      {!source && <p className="field-hint">This capture was set up on another computer. Choose what to capture here.</p>}
      <div className="settings-fields item-capture"><CaptureSettings source={source ?? null} options={{ ...defaultCaptureOptions, ...content.options }}
        onSource={next => onChange(i => i.content.kind === 'capture' ? { ...i, title: !i.title || i.title === i.content.source?.name ? next.name : i.title, content: { ...i.content, source: next } } : i, 0)}
        onOptions={options => setContent(c => c.kind === 'capture' ? { ...c, options } : c, 0)}/></div>
    </ItemLayout>;
  }
  if (content.kind === 'presentation') {
    const paths = content.slides;
    const setPaths = (next: string[]) => setContent(c => c.kind === 'presentation' ? { ...c, slides: next } : c, 0);
    return <ItemLayout preview={<div className="thumb"><Background path={paths[0] || ''} fit="contain"/></div>} kindLabel={`Slides · ${paths.length}`} title={titleField}>
      <div className="item-field">Slides <small>Drag to reorder</small>
        <SlideGrid paths={paths} onChange={setPaths}/>
      </div>
      <div><button onClick={onEditSlides}><Icon name="plus"/>Add slides from a PDF or images…</button></div>
    </ItemLayout>;
  }
  if (content.kind === 'header') return <div className="item-layout section-editor">
    <label className="item-field item-title">Section name<input aria-label="Section name" value={item.title || ''} placeholder="Praise & Worship" onChange={e => setTitle(e.target.value)}/></label>
    <p className="field-hint">Groups the items below it, up to the next section. In the left column the section collapses as one, and cueing passes over its name.</p>
  </div>;
  const file = media.find(m => m.id === content.mediaId);
  const path = file?.path ?? '';
  if (!file) return <ItemLayout preview={<div className="thumb"><span className="thumb-tile"><small>Missing</small></span></div>} kindLabel="Media" title={titleField}><p className="field-hint">This file is no longer in the library.</p></ItemLayout>;
  if (file.kind === 'image' || file.kind === 'presentation') return <ItemLayout preview={<MediaThumb kind="image" path={path}/>} kindLabel="Image" title={titleField}><p className="field-hint">Shown full screen on the projection, filled or fitted as set in Settings → Projection.</p></ItemLayout>;
  const options = { ...defaultVideoOptions, ...content.options };
  const setOptions = (patch: Partial<VideoOptions>) => setContent(c => c.kind === 'media' ? { ...c, options: { ...defaultVideoOptions, ...c.options, ...patch } } : c);
  const seconds = (ms: number | null) => ms === null ? '' : String(Math.round(ms / 100) / 10);
  return <ItemLayout preview={<MediaThumb kind={file.kind} path={path}/>} kindLabel={file.kind === 'video' ? 'Video' : 'Audio'} title={titleField}>
    <div className="item-options">
      {file.kind === 'video' && <label className="setting-check"><input type="checkbox" checked={options.confidence} onChange={e => setOptions({ confidence: e.target.checked })}/>Show on the confidence screen <small>Muted</small></label>}
      <label className="setting-check"><input type="checkbox" checked={options.looping} onChange={e => setOptions({ looping: e.target.checked })}/>Loop <small>Repeat the clip until the next cue</small></label>
      <div className="item-option-row">
        <label>Start <span className="unit-input"><input inputMode="decimal" aria-label="Start, seconds" value={seconds(options.start_ms)} onChange={e => { const v = Number(e.target.value); if (Number.isFinite(v) && v >= 0) setOptions({ start_ms: Math.round(v * 1000) }); }}/>s</span></label>
        <label>End <span className="unit-input"><input inputMode="decimal" aria-label="End, seconds" placeholder="Full length" value={seconds(options.end_ms)} onChange={e => { const raw = e.target.value.trim(); const v = Number(raw); if (!raw) setOptions({ end_ms: null }); else if (Number.isFinite(v) && v > 0) setOptions({ end_ms: Math.round(v * 1000) }); }}/>s</span></label>
        <label>Speed <select aria-label="Speed" value={options.rate_percent} onChange={e => setOptions({ rate_percent: Number(e.target.value) })}>{[50, 75, 90, 100, 110, 125, 150, 200].map(v => <option key={v} value={v}>{v / 100}×</option>)}</select></label>
      </div>
      <label className="item-range">Volume <input type="range" min={0} max={100} value={options.volume} onChange={e => setOptions({ volume: Number(e.target.value) })}/><output>{options.volume}%</output></label>
    </div>
  </ItemLayout>;
}

function ItemLayout({ preview, kindLabel, title, children }: { preview: ReactNode; kindLabel: string; title: ReactNode; children: ReactNode }) {
  return <div className="item-layout">
    <div className="item-layout-top"><div className="item-preview">{preview}</div><div className="item-layout-head"><span className="eyebrow">{kindLabel}</span>{title}</div></div>
    {children}
  </div>;
}

function SongItemEditor({ item, song, songs, versions, media, onChange }: { item: ServiceItem; song: SongItem; songs: Song[]; versions: Map<string, SongVersion>; media: MediaFile[]; onChange: (change: Change, delay?: number) => void }) {
  const library = songs.find(s => s.id === song.songId);
  const version = song.versionId ? versions.get(song.versionId) : library?.current;
  if (!library || !version) return <p className="empty-panel">{library ? 'Loading this song…' : 'This song is no longer in the library.'}</p>;
  const draft: SongDraft = { ...serviceDraft(song, version), background: item.background ?? version.background };
  const title = item.title ?? library.title;
  const write = (next: SongDraft, nextTitle = title, key = song.key) => onChange(i => i.content.kind !== 'song' ? i : {
    ...i,
    title: nextTitle === library.title ? undefined : nextTitle,
    background: next.background === version.background ? undefined : next.background,
    content: { ...i.content, key, arrangement: fromServiceDraft(next, version) },
  });
  const revert = () => onChange(i => i.content.kind !== 'song' ? i : { ...i, title: undefined, background: undefined, content: { ...i.content, arrangement: undefined } }, 0);
  const notice = <>
    {song.versionId && <div className="edited-notice" role="status"><Icon name="edit"/><span>This service is past, so it keeps version {version.number} of the song. Library changes don't reach it.</span></div>}
    {edited(item) && <div className="edited-notice" role="status"><Icon name="edit"/><span>Edited for this service. The library song is unchanged.</span><button onClick={revert}>Use library version</button></div>}
  </>;
  const details = <label className="song-field">Key for this service<input aria-label="Key for this service" placeholder={library.key ?? 'Library key'} value={song.key ?? ''} onChange={e => write(draft, title, e.target.value.trim() || undefined)}/></label>;
  return <SongWorkspace title={title} onTitle={next => write(draft, next)} song={draft} onSong={next => write(next)} backgrounds={media} notice={notice} details={details}/>;
}

function SlideGrid({ paths, onChange }: { paths: string[]; onChange: (paths: string[]) => void }) {
  const [dragged, setDragged] = useState<number | null>(null);
  return <div className="slide-grid">{paths.map((path, index) => <div key={`${index}-${path}`} className={`slide-grid-item ${dragged === index ? 'is-dragging' : ''}`} draggable
    onDragStart={() => setDragged(index)} onDragEnd={() => setDragged(null)} onDragOver={e => e.preventDefault()}
    onDrop={e => { e.preventDefault(); if (dragged === null || dragged === index) return; const next = [...paths]; const [moved] = next.splice(dragged, 1); next.splice(index, 0, moved); onChange(next); setDragged(null); }}>
    <div className="thumb"><Background path={path} fit="contain"/><span className="thumb-badge">{index + 1}</span></div>
    <button className="slide-grid-remove icon-button" aria-label={`Remove slide ${index + 1}`} disabled={paths.length < 2} onClick={() => onChange(paths.filter((_, i) => i !== index))}><Icon name="close"/></button>
  </div>)}</div>;
}
