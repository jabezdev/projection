import { useRef, useState } from 'react';
import { invoke, isTauri } from './tauri';
import { Icon } from './Icons';
import { useMediaUrl } from './useMediaUrl';
import { newItemId, type Media, type SessionItem } from './models';
function Thumbnail({ path }: { path: string }) { const url = useMediaUrl(path); return <img src={url || undefined} alt=""/>; }
export default function PresentationEditor({ images, item, onClose, onSave }: { images: Media[]; item?: SessionItem; onClose: () => void; onSave: (item: SessionItem) => Promise<void> }) {
  const [title, setTitle] = useState(item?.title || 'Image presentation');
  const [paths, setPaths] = useState<string[]>(item?.kind.kind === 'presentation' ? item.kind.data.slides || [] : []);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const drag = useRef<number | null>(null);
  function move(from: number, to: number) { setPaths(current => { const next = [...current]; const [path] = next.splice(from, 1); next.splice(to, 0, path); return next; }); }
  async function browse() {
    try { const files = await invoke<string[]>('choose_presentation_images'); setPaths(current => [...current, ...files]); } catch (e) { setError(String(e)); }
  }
  return <div className="modal-backdrop"><form className="presentation-editor" role="dialog" aria-modal="true" aria-label="Image presentation" onKeyDown={e => { if (e.key === 'Escape' && !busy) onClose(); }} onSubmit={e => { e.preventDefault(); setBusy(true); void onSave({ id: item?.id ?? newItemId(), title: title.trim(), kind: { kind: 'presentation', data: { path: paths[0], slides: paths } } }).catch(e => { setError(String(e)); setBusy(false); }); }}>
    <header className="surface-header"><div><small>IMAGE PRESENTATION</small><input aria-label="Presentation title" value={title} onChange={e => setTitle(e.target.value)} autoFocus/></div><button type="button" className="icon-button" aria-label="Close presentation editor" disabled={busy} onClick={onClose}><Icon name="close"/></button></header>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <div className="presentation-import">{isTauri() ? <button type="button" className="primary-action" onClick={() => void browse()}><Icon name="plus"/>Import image group</button> : <label className="primary-action">Import image group<input type="file" aria-label="Import presentation images" accept="image/*" multiple onChange={e => { const files = [...(e.target.files || [])]; void Promise.all(files.map(file => new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = reject; reader.readAsDataURL(file); }))).then(values => setPaths(current => [...current, ...values])).catch(e => setError(String(e))); }}/></label>}
      <select aria-label="Add library image" value="" onChange={e => { if (e.target.value) setPaths(current => [...current, e.target.value]); }}><option value="">Add an image from the library…</option>{images.filter(media => media.kind === 'image').map(media => <option key={media.id} value={media.path}>{media.title}</option>)}</select><small>Import multiple images, then drag them into order.</small>
    </div><div className="presentation-image-list">{paths.map((path, index) => <div key={`${index}-${path}`} className="presentation-image-row" draggable onDragStart={e => { drag.current = index; e.dataTransfer.setData('text/plain', String(index)); }} onDragEnd={() => { drag.current = null; }} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); if (drag.current !== null) move(drag.current, index); drag.current = null; }}><Icon name="grip"/><Thumbnail path={path}/><span>Slide {index + 1}<small>{path.startsWith('data:') ? 'Imported image' : path.split(/[\\/]/).pop()}</small></span><button type="button" className="icon-button" aria-label={`Move slide ${index + 1} up`} disabled={!index} onClick={() => move(index, index - 1)}><Icon name="up"/></button><button type="button" className="icon-button" aria-label={`Move slide ${index + 1} down`} disabled={index === paths.length - 1} onClick={() => move(index, index + 1)}><Icon name="down"/></button><button type="button" className="icon-button" aria-label={`Remove slide ${index + 1}`} onClick={() => setPaths(current => current.filter((_, i) => i !== index))}><Icon name="close"/></button></div>)}{!paths.length && <p className="empty-panel">Choose your images to start the presentation.</p>}</div>
    <footer><span>{paths.length} slides</span><button type="button" disabled={busy} onClick={onClose}>Cancel</button><button className="primary-action" disabled={busy || !paths.length || !title.trim()}>{busy ? 'Saving…' : 'Save presentation'}</button></footer>
  </form></div>;
}
export function PresentationCues({ item, active, step, onSelect }: { item: SessionItem; active: boolean; step?: number | null; onSelect: (index: number) => void }) {
  const paths = item.kind.kind === 'presentation' ? item.kind.data.slides || [item.kind.data.path] : [];
  return <div className="presentation-cues">{paths.map((path, index) => <button key={index} data-step-index={index} aria-label={`Cue ${item.title}, slide ${index + 1}`} aria-pressed={active && step === index} onClick={() => onSelect(index)}><Thumbnail path={path}/><span>{index + 1}</span></button>)}</div>;
}
