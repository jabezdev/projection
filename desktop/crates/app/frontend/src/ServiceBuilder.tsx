import PresentationEditor from './PresentationEditor';
import { useState } from 'react';
import { invoke } from './tauri';
import { Icon } from './Icons';
import { decodeSong, songSteps } from './song';
import { newItemId, type Media, type SessionItem, type SongHeader, type SongModel } from './models';
import CaptureEditor from './CaptureEditor';

/** `confirmRemove` asks before an item is removed (while the outputs are shown). */
type Props = { name?: string; items?: SessionItem[]; currentItems: SessionItem[]; songs: SongHeader[]; media: Media[]; onClose: () => void; onSave: (name: string, items: SessionItem[]) => Promise<void>; confirmRemove?: (title: string) => Promise<boolean> };
export default function ServiceBuilder({ name = '', items = [], currentItems, songs, media, onClose, onSave, confirmRemove }: Props) {
  const [title, setTitle] = useState(name);
  const [schedule, setSchedule] = useState(items);
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('all');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [presentation, setPresentation] = useState(false);
  const [capture, setCapture] = useState(false);
  function move(from: number, to: number) { setSchedule(current => { const next = [...current]; const [item] = next.splice(from, 1); if (item) next.splice(to, 0, item); return next; }); }
  async function addSong(id: number) {
    setBusy(true); setError('');
    try {
      const song = await invoke<SongModel>('get_song', { id }); const doc = decodeSong(song.lyrics);
      setSchedule(current => [...current, { id: newItemId(), title: song.title, song: JSON.stringify(doc), background: doc.background, kind: { kind: 'text', data: { steps: songSteps(doc) } } }]);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  function addMedia(item: Media) { setSchedule(current => [...current, { id: newItemId(), title: item.title, kind: { kind: item.kind, data: { path: item.path } } }]); }
  return <><div className="modal-backdrop"><form className="service-builder" role="dialog" aria-modal="true" aria-label={name ? 'Edit service' : 'Create service'} onSubmit={e => { e.preventDefault(); setBusy(true); setError(''); onSave(title.trim(), schedule).catch(e => { setError(String(e)); setBusy(false); }); }} onKeyDown={e => { if (e.key === 'Escape' && !busy) onClose(); }}>
    <header className="surface-header"><div><small>{name ? 'EDIT SERVICE' : 'NEW SERVICE'}</small><input aria-label="Service name" autoFocus required placeholder="Sunday morning service" value={title} onChange={e => setTitle(e.target.value)}/></div><button type="button" className="icon-button" aria-label="Close service builder" disabled={busy} onClick={onClose}><Icon name="close"/></button></header>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <div className="service-builder-body"><section className="service-source"><div className="panel-heading">From your library</div><input aria-label="Search service library" placeholder="Search songs and media…" value={query} onChange={e => setQuery(e.target.value)}/><select aria-label="Service library category" value={category} onChange={e => setCategory(e.target.value)}>{['all', 'songs', 'video', 'image', 'audio'].map(value => <option key={value} value={value}>{value === 'all' ? 'Songs & all media' : value.charAt(0).toUpperCase() + value.slice(1)}</option>)}</select><button type="button" className="copy-service" onClick={() => setPresentation(true)}>+ Import image presentation</button><button type="button" className="copy-service" onClick={() => setCapture(true)}>+ Screen capture</button><div className="service-source-list">
      {(category === 'all' || category === 'songs') && songs.filter(s => s.title.toLowerCase().includes(query.toLowerCase())).map(song => <button type="button" key={`song-${song.id}`} disabled={busy} onClick={() => void addSong(song.id)}><span>{song.title}<small>Song</small></span><Icon name="plus"/></button>)}
      {media.filter(m => (category === 'all' || category === m.kind) && m.title.toLowerCase().includes(query.toLowerCase())).map(item => <button type="button" key={`media-${item.id}`} onClick={() => addMedia(item)}><span>{item.title}<small>{item.kind}</small></span><Icon name="plus"/></button>)}
      {!songs.length && !media.length && <p className="empty-panel">Create songs and import media in the library, or create an empty service and add them later.</p>}
    </div></section><section className="service-lineup"><div className="panel-heading">Schedule <small>{schedule.length} items</small></div>{!name && currentItems.length > 0 && <button type="button" className="copy-service" onClick={() => setSchedule(current => [...current, ...currentItems.map(item => ({ ...item, id: newItemId() }))])}>Copy current service items</button>}<div className="service-lineup-list">
      {!schedule.length && <p className="empty-panel">Add songs and media from the left, then drag them to set the order of your service.</p>}
      {schedule.map((item, index) => <div className="service-lineup-item" key={item.id} draggable onDragStart={e => { e.dataTransfer.setData('application/x-magi-service', String(index)); e.dataTransfer.effectAllowed = 'move'; }} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); const from = e.dataTransfer.getData('application/x-magi-service'); if (/^\d+$/.test(from)) move(Number(from), index); }}><Icon name="grip"/><span>{item.title}<small>{item.kind.kind === 'text' ? 'Song' : item.kind.kind}</small></span><button type="button" className="icon-button" aria-label={`Move ${item.title} up`} disabled={!index} onClick={() => move(index, index - 1)}><Icon name="up"/></button><button type="button" className="icon-button" aria-label={`Move ${item.title} down`} disabled={index === schedule.length - 1} onClick={() => move(index, index + 1)}><Icon name="down"/></button><button type="button" className="icon-button" aria-label={`Remove ${item.title} from service`} onClick={() => void (async () => { if (confirmRemove && !(await confirmRemove(item.title || 'this item'))) return; setSchedule(current => current.filter(old => old.id !== item.id)); })()}><Icon name="close"/></button></div>)}
    </div></section></div>
    <footer><span>{schedule.length ? 'Your schedule is ready. You can still edit it during the service.' : 'You can add items to this service later.'}</span><button type="button" disabled={busy} onClick={onClose}>Cancel</button><button className="primary-action" disabled={busy || !title.trim()}>{busy ? 'Saving…' : name ? 'Save service' : 'Create service'}</button></footer>
  </form></div>{presentation && <PresentationEditor images={media} onClose={() => setPresentation(false)} onSave={async item => { setSchedule(current => [...current, item]); setPresentation(false); }}/>}{capture && <CaptureEditor onClose={() => setCapture(false)} onSave={async item => { setSchedule(current => [...current, item]); setCapture(false); }}/>}</>;
}
