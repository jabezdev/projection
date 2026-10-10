import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from 'react';
import { arrangedParts, emptySong, partCode, partColor, partColors, slides, moveArrangement, type SongDocument } from './song';
import SongPartEditor from './SongPartEditor';
import { Icon } from './Icons';
import type { Media } from './models';

type Props = { title?: string; song?: SongDocument; backgrounds?: Media[]; onClose: () => void; onSave: (title: string, song: SongDocument) => Promise<void> };
const PART_DRAG = 'application/x-magi-part';
const ORDER_DRAG = 'application/x-magi-order';
export default function SongEditor({ title = '', song, backgrounds = [], onClose, onSave }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const [name, setName] = useState(title);
  const [document, setDocument] = useState(song || emptySong());
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [dropSlot, setDropSlot] = useState<number | null>(null);
  const dragSource = useRef<{ partId?: string; index?: number } | null>(null);
  function endDrag() { setDragging(false); setDropSlot(null); dragSource.current = null; }
  function rowSlot(event: DragEvent, index: number) {
    const bounds = event.currentTarget.getBoundingClientRect();
    return index + (event.clientY >= bounds.top + bounds.height / 2 ? 1 : 0);
  }
  useEffect(() => {
    const previous = globalThis.document.activeElement as HTMLElement | null;
    return () => previous?.focus();
  }, []);
  function addPart(type: string) {
    const id = crypto.randomUUID();
    setDocument(current => ({ ...current, parts: [...current.parts, { id, type, number: Math.max(0, ...current.parts.filter(p => p.type === type).map(p => p.number)) + 1, lyrics: '' }] }));
    requestAnimationFrame(() => globalThis.document.getElementById(`lyrics-${id}`)?.focus());
  }
  function insert(id: string, index = document.arrangement.length) {
    setDocument(current => { const arrangement = [...current.arrangement]; arrangement.splice(index, 0, id); return { ...current, arrangement }; });
  }
  function reorder(from: number, to: number) {
    setDocument(current => {
      if (from < 0 || from >= current.arrangement.length) return current;
      const arrangement = [...current.arrangement];
      const [id] = arrangement.splice(from, 1); arrangement.splice(to, 0, id);
      return { ...current, arrangement };
    });
  }
  function drop(event: DragEvent, index: number) {
    event.preventDefault(); event.stopPropagation();
    const partId = dragSource.current?.partId || event.dataTransfer.getData(PART_DRAG);
    const from = dragSource.current?.index ?? Number(event.dataTransfer.getData(ORDER_DRAG) || -1);
    if (partId && document.parts.some(p => p.id === partId)) insert(partId, index);
    else if (Number.isInteger(from) && from >= 0) setDocument(current => ({ ...current, arrangement: moveArrangement(current.arrangement, from, index) }));
    endDrag();
  }

  async function save() {
    if (!name.trim() || !document.arrangement.length || arrangedParts(document).some(p => !slides(p).length)) {
      setError('Name the song, add lyrics, and insert at least one part into the arrangement.'); return;
    }
    setSaving(true); setError('');
    try { await onSave(name.trim(), document); } catch (e) { setError(String(e)); setSaving(false); }
  }
  return <div ref={dialog} className="song-editor" role="dialog" aria-modal="true" aria-label="Song editor" onKeyDown={e => {
    if (e.key === 'Escape' && !saving) onClose();
    if (e.key !== 'Tab') return;
    const targets = dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input, textarea, select');
    if (!targets?.length) return;
    const first = targets[0], last = targets[targets.length - 1];
    if (e.shiftKey && globalThis.document.activeElement === first) { e.preventDefault(); last.focus(); }
    if (!e.shiftKey && globalThis.document.activeElement === last) { e.preventDefault(); first.focus(); }
  }}>
    <header className="editor-header"><div><input aria-label="Song title" placeholder="Untitled song" value={name} onChange={e => setName(e.target.value)} autoFocus/><label className="song-author">Author / Singer<input aria-label="Author / Singer" placeholder="Add author or singer" value={document.authorSinger || ''} onChange={e => setDocument(current => ({ ...current, authorSinger: e.target.value }))}/></label></div><button disabled={saving} onClick={onClose}>Cancel</button><button className="primary-action" disabled={saving} onClick={save}>{saving ? 'Saving…' : 'Save song'}</button></header>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <div className="editor-workspace">
      <nav className="editor-picker" aria-label="Add song part"><div className="panel-heading">Add a part</div>{Object.keys(partColors).map(type => <button key={type} style={{ '--part-color': partColors[type] } as CSSProperties} onClick={() => addPart(type)}><span className="part-dot"/>{type}<Icon name="plus"/></button>)}</nav>
      <section className="editor-parts"><div className="panel-heading">Lyrics <small>A blank line starts a new slide</small></div><div className="editor-scroll">
        {!document.parts.length && <p className="empty-panel">Choose a part, paste your lyrics, and separate slides with an empty line.</p>}
        {document.parts.map(part => { const chunks = slides(part); return <article className="part-card" key={part.id} style={{ '--part-color': partColor(part) } as CSSProperties}>
          <header draggable onDragStart={e => { dragSource.current = { partId: part.id }; e.dataTransfer.setData(PART_DRAG, part.id); e.dataTransfer.setData('text/plain', part.id); e.dataTransfer.effectAllowed = 'copy'; setDragging(true); }} onDragEnd={endDrag}>
            <span className="drag-handle" title="Drag into arrangement"><Icon name="grip"/></span><strong className="part-code-badge">{partCode(part)}</strong><h2>{part.type} {part.number}</h2><span>{chunks.length} {chunks.length === 1 ? 'slide' : 'slides'}</span>
            <input type="color" aria-label={`${part.type} ${part.number} color`} title="Part color" value={partColor(part)} onChange={e => setDocument(current => ({ ...current, parts: current.parts.map(p => p.id === part.id ? { ...p, color: e.target.value } : p) }))}/>
            <button className="insert-part" aria-label={`Insert ${part.type} ${part.number}`} onClick={() => insert(part.id)}><Icon name="plus"/>Insert</button><button className="icon-button" aria-label={`Delete ${part.type} ${part.number}`} onClick={() => setDocument(current => ({ ...current, parts: current.parts.filter(p => p.id !== part.id), arrangement: current.arrangement.filter(id => id !== part.id) }))}><Icon name="close"/></button>
          </header>
          <SongPartEditor part={part} background={backgrounds.find(media => media.path === document.background && media.kind === 'image')?.path} onChange={lyrics => setDocument(current => ({ ...current, parts: current.parts.map(p => p.id === part.id ? { ...p, lyrics, slideTexts: undefined } : p) }))}/>

        </article>; })}
      </div></section>
      <aside className={`editor-order ${dragging ? 'accepting-drop' : ''}`}><div className="panel-heading">Arrangement <small>{document.arrangement.length} parts</small></div>
        <div className="arrangement-insert"><span>Insert a part</span><div>{document.parts.map(part => <button key={part.id} style={{ '--part-color': partColor(part) } as CSSProperties} onClick={() => insert(part.id)} aria-label={`Add ${partCode(part)} to arrangement`}><span className="part-dot"/>{partCode(part)}<Icon name="plus"/></button>)}</div></div>
        <div className="editor-scroll arrangement-dropzone" aria-label="Arrangement drop area" onDragOver={e => { e.preventDefault(); setDropSlot(document.arrangement.length); }} onDrop={e => drop(e, document.arrangement.length)}>
          {!document.arrangement.length && <p className="empty-panel">Drop a part here or use the buttons above. Add the same part again to repeat it.</p>}
          {arrangedParts(document).map((part, index) => <div className={`order-row ${dropSlot === index ? 'drop-before' : ''}`} key={`${part.id}-${index}`} style={{ '--part-color': partColor(part) } as CSSProperties} draggable aria-label={`${part.type} ${part.number}, position ${index + 1}`} onDragStart={e => { dragSource.current = { index }; e.dataTransfer.setData(ORDER_DRAG, String(index)); e.dataTransfer.setData('text/plain', String(index)); e.dataTransfer.effectAllowed = 'move'; setDragging(true); }} onDragEnd={endDrag} onDragOver={e => { e.preventDefault(); e.stopPropagation(); setDropSlot(rowSlot(e, index)); }} onDrop={e => drop(e, rowSlot(e, index))}>
            <Icon name="grip"/><strong>{partCode(part)}</strong><span>{part.type} {part.number}</span><button className="icon-button" aria-label={`Move ${partCode(part)} up, position ${index + 1}`} disabled={!index} onClick={() => reorder(index, index - 1)}><Icon name="up"/></button><button className="icon-button" aria-label={`Move ${partCode(part)} down, position ${index + 1}`} disabled={index === document.arrangement.length - 1} onClick={() => reorder(index, index + 1)}><Icon name="down"/></button><button className="icon-button" aria-label={`Remove ${partCode(part)}, position ${index + 1}`} onClick={() => setDocument(current => ({ ...current, arrangement: current.arrangement.filter((_, i) => i !== index) }))}><Icon name="close"/></button>
          </div>)}
          {document.arrangement.length > 0 && <div className={`arrangement-end ${dropSlot === document.arrangement.length ? 'drop-before' : ''}`}>Drop here to append</div>}
        </div>
        <div className="editor-background"><label htmlFor="song-background">Background</label><select aria-label="Choose song background" value={backgrounds.some(m => m.path === document.background) ? document.background : ''} onChange={e => setDocument(current => ({ ...current, background: e.target.value }))}><option value="">Keep previous / custom path</option>{backgrounds.filter(m => m.kind !== 'audio').map(m => <option value={m.path} key={m.id}>{m.title}</option>)}</select><input id="song-background" value={document.background} placeholder="Image or video file path" onChange={e => setDocument(current => ({ ...current, background: e.target.value }))}/><small>Leave empty to retain the previous background.</small></div>
      </aside>
    </div>
  </div>;
}
