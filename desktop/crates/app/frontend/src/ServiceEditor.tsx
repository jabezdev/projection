import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { Icon } from './Icons';
import { invoke } from './tauri';
import { defaultCaptureOptions, defaultVideoOptions, newItemId, type Media, type ProjectModel, type SessionItem, type SongHeader, type SongModel, type VideoOptions } from './models';
import { decodeSong, itemSong, songSteps, type SongDocument } from './song';
import { songItem } from './serviceItems';
import SongWorkspace from './SongWorkspace';
import MiniLibrary, { type LibraryPick } from './MiniLibrary';
import PresentationEditor from './PresentationEditor';
import { CaptureSettings } from './CaptureEditor';
import { ItemThumb, MediaThumb } from './Thumbs';
import { itemSummary } from './preview';
import { Background } from './SlidePreview';
import { forgetSong } from './useSongDocs';

type Props = {
  project: ProjectModel;
  persist: (project: ProjectModel) => Promise<void>;
  songs: SongHeader[];
  media: Media[];
  /** The item to open on, such as the one whose edit button was pressed. */
  initialItemId?: number;
  /** The item on air, which asks before it is removed while the screens are on. */
  activeItemId?: number | null;
  confirmRemove?: (title: string) => Promise<boolean>;
  onCatalogChanged: () => Promise<void>;
  onRenamed: (name: string) => void;
  onClose: () => void;
};
type Toast = { text: string; undo?: SessionItem[] };
const SAVE_DELAY = 350;

const mediaItem = (media: Media): SessionItem => ({ id: newItemId(), title: media.title, kind: { kind: media.kind, data: { path: media.path } } });

/**
 * Edits the loaded service: its items on the left (pictures, order, insert,
 * remove), the selected item's settings on the right. Changes save as you go;
 * song changes stay in this service and are marked as edited.
 */
export default function ServiceEditor({ project, persist, songs, media, initialItemId, activeItemId, confirmRemove, onCatalogChanged, onRenamed, onClose }: Props) {
  const [name, setName] = useState(project.name);
  const [items, setItems] = useState(project.session.items);
  const [selected, setSelected] = useState<number | null>(initialItemId ?? project.session.items[0]?.id ?? null);
  const [insert, setInsert] = useState<{ after: number; top: number } | null>(null);
  const [slides, setSlides] = useState<{ after: number } | { item: SessionItem } | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [error, setError] = useState('');
  const [dragged, setDragged] = useState<number | null>(null);
  const [dropAfter, setDropAfter] = useState<number | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const latest = useRef({ name, items });
  const timer = useRef<number | undefined>(undefined);
  const toastTimer = useRef<number | undefined>(undefined);

  const flush = useCallback(() => {
    window.clearTimeout(timer.current); timer.current = undefined;
    const { name, items } = latest.current;
    return persist({ ...project, name: name.trim() || project.name, session: { items } })
      .then(() => { setError(''); return true; }, e => { setError(String(e)); return false; });
  }, [persist, project]);
  const save = useCallback((next: { name?: string; items?: SessionItem[] }, delay = SAVE_DELAY) => {
    latest.current = { ...latest.current, ...next };
    if (next.items) setItems(next.items);
    if (next.name !== undefined) setName(next.name);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void flush(), delay);
  }, [flush]);
  useEffect(() => () => { if (timer.current !== undefined) void flush(); window.clearTimeout(toastTimer.current); }, [flush]);
  function showToast(next: Toast) { setToast(next); window.clearTimeout(toastTimer.current); toastTimer.current = window.setTimeout(() => setToast(null), 6000); }
  async function close() {
    if (!(await flush())) return;
    onRenamed(latest.current.name.trim() || project.name);
    onClose();
  }

  const updateItem = (id: number, change: (item: SessionItem) => SessionItem, delay?: number) =>
    save({ items: latest.current.items.map(item => item.id === id ? change(item) : item) }, delay);
  function place(item: SessionItem, after: number) {
    const before = latest.current.items;
    const at = after === 0 ? 0 : before.findIndex(i => i.id === after) + 1 || before.length;
    save({ items: [...before.slice(0, at), item, ...before.slice(at)] }, 0);
    setSelected(item.id); setInsert(null);
    showToast({ text: `Added ${item.title || 'item'}`, undo: before });
  }
  async function pick(choice: LibraryPick, after: number) {
    setError('');
    try {
      if (choice.kind === 'song') place(songItem(await invoke<SongModel>('get_song', { id: choice.id })), after);
      else if (choice.kind === 'media') place(mediaItem(choice.media), after);
      else place({ id: newItemId(), title: choice.source.name, kind: { kind: 'capture', data: { source: choice.source, options: defaultCaptureOptions } } }, after);
    } catch (e) { setError(String(e)); }
  }
  async function remove(item: SessionItem) {
    if (item.id === activeItemId && confirmRemove && !(await confirmRemove(item.title || 'this item'))) return;
    const before = latest.current.items;
    const index = before.findIndex(i => i.id === item.id);
    const next = before.filter(i => i.id !== item.id);
    save({ items: next }, 0);
    if (selected === item.id) setSelected((next[index] ?? next[index - 1])?.id ?? null);
    showToast({ text: `Removed ${item.title || 'item'}`, undo: before });
  }
  function duplicate(item: SessionItem) {
    const before = latest.current.items;
    const copy = { ...structuredClone(item), id: newItemId() };
    const at = before.findIndex(i => i.id === item.id) + 1;
    save({ items: [...before.slice(0, at), copy, ...before.slice(at)] }, 0);
    setSelected(copy.id);
    showToast({ text: `Duplicated ${item.title || 'item'}`, undo: before });
  }
  function move(id: number, after: number) {
    const before = latest.current.items;
    if (id === after) return;
    const moving = before.find(i => i.id === id);
    if (!moving) return;
    const rest = before.filter(i => i.id !== id);
    const at = after === 0 ? 0 : rest.findIndex(i => i.id === after) + 1;
    save({ items: [...rest.slice(0, at), moving, ...rest.slice(at)] }, 0);
  }
  function addSection(after: number) {
    place({ id: newItemId(), title: 'New section', kind: { kind: 'section', data: {} } }, after);
    requestAnimationFrame(() => document.querySelector<HTMLInputElement>('.section-editor input')?.select());
  }
  function undo() { if (toast?.undo) { save({ items: toast.undo }, 0); setToast(null); } }
  function openInsert(after: number, anchor: HTMLElement) {
    const box = body.current?.getBoundingClientRect();
    const at = anchor.getBoundingClientRect();
    const top = box ? Math.max(8, Math.min(at.top - box.top - 40, box.height - 480)) : 8;
    setInsert(current => current?.after === after ? null : { after, top });
  }
  const title = (id: number) => items.find(i => i.id === id)?.title || 'item';
  const where = insert ? insert.after === 0 ? 'at the top' : `after “${title(insert.after)}”` : '';
  const item = items.find(i => i.id === selected) ?? null;
  const gap = (after: number) => <div className={`insert-gap ${insert?.after === after ? 'is-open' : ''} ${dropAfter === after ? 'is-drop' : ''}`}
    onDragOver={e => { if (dragged === null) return; e.preventDefault(); setDropAfter(after); }}
    onDrop={e => { e.preventDefault(); if (dragged !== null) move(dragged, after); setDragged(null); setDropAfter(null); }}>
    <button aria-label={after === 0 ? 'Insert at the top' : `Insert after ${title(after)}`} onClick={e => openInsert(after, e.currentTarget)}><span>+ insert here</span></button>
    <button className="insert-section" aria-label={after === 0 ? 'Add a section at the top' : `Add a section after ${title(after)}`} onClick={() => addSection(after)}>+ section</button>
  </div>;

  return <div className="modal-backdrop" onKeyDown={e => {
    if (e.key === 'Escape' && !e.defaultPrevented && !insert && !slides) void close();
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && toast?.undo && !(e.target as HTMLElement).closest('input, textarea')) { e.preventDefault(); undo(); }
  }}><section className="service-editor" role="dialog" aria-modal="true" aria-label="Edit service">
    <header className="service-editor-head">
      <input aria-label="Service name" value={name} placeholder="Service name" onChange={e => save({ name: e.target.value })}/>
      <span className="grow"/>
      <span className="saved-note">Changes save as you go</span>
      <button className="primary-action" onClick={() => void close()}>Done</button>
    </header>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <div className="service-editor-body" ref={body}>
      <section className="service-items" aria-label="Items">
        <div className="panel-heading">Items <small>{items.length} · drag to reorder</small></div>
        <div className="service-items-scroll">
          {gap(0)}
          {items.map((entry, index) => <div key={entry.id}>
            <div className={`service-item ${entry.kind.kind === 'section' ? 'is-section' : ''} ${entry.id === selected ? 'is-selected' : ''} ${dragged === entry.id ? 'is-dragging' : ''}`} draggable tabIndex={0} role="button" aria-pressed={entry.id === selected}
              onClick={() => setSelected(entry.id)} onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSelected(entry.id); } if (e.key === 'Delete') void remove(entry); }}
              onDragStart={e => { setDragged(entry.id); e.dataTransfer.setData('text/plain', String(entry.id)); e.dataTransfer.effectAllowed = 'move'; }}
              onDragEnd={() => { setDragged(null); setDropAfter(null); }}
              onDragOver={e => { if (dragged === null) return; e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); setDropAfter(e.clientY < r.top + r.height / 2 ? (items[index - 1]?.id ?? 0) : entry.id); }}
              onDrop={e => { e.preventDefault(); if (dragged !== null && dropAfter !== null) move(dragged, dropAfter); setDragged(null); setDropAfter(null); }}>
              {entry.kind.kind === 'section'
                ? <span className="service-section-name"><strong>{entry.title || 'Untitled section'}</strong><small>{sectionSize(items, index)} items</small></span>
                : <><span className="service-item-index">{items.slice(0, index + 1).filter(i => i.kind.kind !== 'section').length}</span><ItemThumb item={entry}/><span className="service-item-text"><strong>{entry.title || 'Untitled'}</strong><small>{itemSummary(entry)}</small></span></>}
              <span className="service-item-tools">
                <button className="icon-button" aria-label={`Duplicate ${entry.title}`} title="Duplicate" onClick={e => { e.stopPropagation(); duplicate(entry); }}><Icon name="copy"/></button>
                <button className="icon-button" aria-label={`Remove ${entry.title}`} title="Remove (Delete)" onClick={e => { e.stopPropagation(); void remove(entry); }}><Icon name="close"/></button>
              </span>
            </div>
            {gap(entry.id)}
          </div>)}
          {!items.length && <p className="empty-panel">This service is empty. Add the first item.</p>}
          <div className="add-row"><button className="add-item" onClick={e => openInsert(items.at(-1)?.id ?? 0, e.currentTarget)}><Icon name="plus"/>Add item</button><button className="add-item" onClick={() => addSection(items.at(-1)?.id ?? 0)}><Icon name="plus"/>Add section</button></div>
        </div>
      </section>
      <section className="service-item-editor" aria-label="Selected item">
        {item ? <ItemEditor key={item.id} item={item} songs={songs} media={media} onChange={(change, delay) => updateItem(item.id, change, delay)} onEditSlides={() => setSlides({ item })}/> : <p className="empty-panel">Select an item to edit it here.</p>}
      </section>
      {insert && <MiniLibrary top={insert.top} where={where} songs={songs} media={media} onClose={() => setInsert(null)}
        onPick={choice => void pick(choice, insert.after)}
        onImport={async () => { const imported = await invoke<Media[]>('import_media_files'); if (imported.length) await onCatalogChanged(); }}
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

/** How many items a section at `index` groups: those up to the next section. */
function sectionSize(items: SessionItem[], index: number) {
  let count = 0;
  for (let i = index + 1; i < items.length && items[i].kind.kind !== 'section'; i++) count++;
  return count;
}

/** The selected item's settings, by kind. `onChange` updates the item and saves it. */
function ItemEditor({ item, songs, media, onChange, onEditSlides }: { item: SessionItem; songs: SongHeader[]; media: Media[]; onChange: (change: (item: SessionItem) => SessionItem, delay?: number) => void; onEditSlides: () => void }) {
  const kind = item.kind;
  const setTitle = (title: string) => onChange(i => ({ ...i, title }));
  if (kind.kind === 'text') return <SongItemEditor item={item} songs={songs} media={media} onChange={onChange}/>;
  const titleField = <label className="item-field item-title">Title<input aria-label="Item title" value={item.title || ''} onChange={e => setTitle(e.target.value)}/></label>;
  if (kind.kind === 'capture') {
    return <ItemLayout preview={<div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>{kind.data.source.name}</small></span></div>} kindLabel="Screen capture" title={titleField}>
      <div className="settings-fields item-capture"><CaptureSettings source={kind.data.source} options={{ ...defaultCaptureOptions, ...kind.data.options }}
        onSource={source => onChange(i => i.kind.kind === 'capture' ? { ...i, title: !i.title || i.title === i.kind.data.source.name ? source.name : i.title, kind: { kind: 'capture', data: { ...i.kind.data, source } } } : i, 0)}
        onOptions={options => onChange(i => i.kind.kind === 'capture' ? { ...i, kind: { kind: 'capture', data: { ...i.kind.data, options } } } : i, 0)}/></div>
    </ItemLayout>;
  }
  if (kind.kind === 'presentation') {
    const paths = kind.data.slides || [kind.data.path];
    const setPaths = (next: string[]) => onChange(i => ({ ...i, kind: { kind: 'presentation', data: { path: next[0], slides: next } } }), 0);
    return <ItemLayout preview={<div className="thumb"><Background path={paths[0] || ''} fit="contain"/></div>} kindLabel={`Slides · ${paths.length}`} title={titleField}>
      <div className="item-field">Slides <small>Drag to reorder</small>
        <SlideGrid paths={paths} onChange={setPaths}/>
      </div>
      <div><button onClick={onEditSlides}><Icon name="plus"/>Add slides from a PDF or images…</button></div>
    </ItemLayout>;
  }
  if (kind.kind === 'section') return <div className="item-layout section-editor">
    <label className="item-field item-title">Section name<input aria-label="Section name" value={item.title || ''} placeholder="Praise & Worship" onChange={e => setTitle(e.target.value)}/></label>
    <p className="field-hint">Groups the items below it, up to the next section. In the left column the section collapses as one, and cueing passes over its name.</p>
  </div>;
  if (kind.kind === 'image') return <ItemLayout preview={<MediaThumb kind="image" path={kind.data.path}/>} kindLabel="Image" title={titleField}><p className="field-hint">Shown full screen on the projection, filled or fitted as set in Settings → Projection.</p></ItemLayout>;
  const options = { ...defaultVideoOptions, ...kind.data.options };
  const setOptions = (patch: Partial<VideoOptions>) => onChange(i => i.kind.kind === 'video' || i.kind.kind === 'audio' ? { ...i, kind: { ...i.kind, data: { ...i.kind.data, options: { ...defaultVideoOptions, ...i.kind.data.options, ...patch } } } } : i);
  const seconds = (ms: number | null) => ms === null ? '' : String(Math.round(ms / 100) / 10);
  return <ItemLayout preview={<MediaThumb kind={kind.kind} path={kind.data.path}/>} kindLabel={kind.kind === 'video' ? 'Video' : 'Audio'} title={titleField}>
    <div className="item-options">
      {kind.kind === 'video' && <label className="setting-check"><input type="checkbox" checked={options.confidence} onChange={e => setOptions({ confidence: e.target.checked })}/>Show on the confidence screen <small>Muted</small></label>}
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

/** A song in this service. Edits stay in the service and mark it as edited; the library song is untouched. */
function SongItemEditor({ item, songs, media, onChange }: { item: SessionItem; songs: SongHeader[]; media: Media[]; onChange: (change: (item: SessionItem) => SessionItem, delay?: number) => void }) {
  const song = itemSong(item);
  const title = item.title || '';
  const [error, setError] = useState('');
  const write = (doc: SongDocument, nextTitle = title) => {
    onChange(i => ({ ...i, title: nextTitle, song: JSON.stringify(doc), background: doc.background, kind: { kind: 'text', data: { steps: songSteps(doc) } } }));
  };
  const edited = (doc: SongDocument) => ({ ...doc, editedInService: true });
  const linked = song.librarySongId != null && songs.some(s => s.id === song.librarySongId);
  async function revert() {
    if (song.librarySongId == null) return;
    setError('');
    try {
      forgetSong(song.librarySongId);
      const saved = await invoke<SongModel>('get_song', { id: song.librarySongId });
      write({ ...decodeSong(saved.lyrics), librarySongId: saved.id, editedInService: false }, saved.title);
    } catch (e) { setError(String(e)); }
  }
  const notice = <>
    {song.editedInService && <div className="edited-notice" role="status"><Icon name="edit"/><span>Edited for this service. The library song is unchanged.</span>{linked && <button onClick={() => void revert()}>Use library version</button>}</div>}
    {error && <p className="ui-error" role="alert">{error}</p>}
  </>;
  return <SongWorkspace title={title} onTitle={next => write(edited(song), next)} song={song} onSong={doc => write(edited(doc))} backgrounds={media} notice={notice}/>;
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
