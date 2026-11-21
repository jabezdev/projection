import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Icon } from '../ui/Icons';
import type { Media } from '../api/models';
import { arrangedParts, partCode, partColor, partColors, slideAtCaret, slides, type Part, type SongDocument } from './song';
import { Background, SlidePreview } from './SlidePreview';
import { backgroundChoices, backgroundName } from '../outputs/preview';

type Props = {
  title: string;
  onTitle: (title: string) => void;
  song: SongDocument;
  onSong: (song: SongDocument) => void;
  backgrounds: Media[];
  notice?: ReactNode;
  autoFocusTitle?: boolean;
};

export default function SongWorkspace({ title, onTitle, song, onSong, backgrounds, notice, autoFocusTitle }: Props) {
  const [cursor, setCursor] = useState<{ partId: string; slide: number } | null>(null);
  const update = (patch: Partial<SongDocument>) => onSong({ ...song, ...patch });
  const setPart = (id: string, patch: Partial<Part>) => update({ parts: song.parts.map(p => p.id === id ? { ...p, ...patch } : p) });
  function addPart(type: string) {
    const id = crypto.randomUUID();
    const number = Math.max(0, ...song.parts.filter(p => p.type === type).map(p => p.number)) + 1;
    onSong({ ...song, parts: [...song.parts, { id, type, number, lyrics: '' }], arrangement: [...song.arrangement, id] });
    requestAnimationFrame(() => document.getElementById(`lyrics-${id}`)?.focus());
  }
  const cursorPart = cursor && song.parts.find(p => p.id === cursor.partId);
  const shownPart = cursorPart || arrangedParts(song)[0] || song.parts[0];
  const shownSlides = shownPart ? slides(shownPart) : [];
  const shownIndex = cursorPart ? Math.min(cursor.slide, Math.max(0, shownSlides.length - 1)) : 0;
  return <div className="song-workspace">
    <section className="song-text" aria-label="Lyrics">
      <div className="song-text-head">
        <span className="label-text">Lyrics <small>A blank line starts a new slide</small></span>
        <div className="add-parts" role="group" aria-label="Add a part">{Object.keys(partColors).map(type => <button key={type} style={{ '--part-color': partColors[type] } as CSSProperties} onClick={() => addPart(type)}><span className="part-dot"/>{type}</button>)}</div>
      </div>
      <div className="song-text-scroll">
        {!song.parts.length && <p className="empty-panel">Add a part above, then type or paste its lyrics. Each part is added to the arrangement.</p>}
        {song.parts.map(part => <LyricPart key={part.id} part={part} active={shownPart?.id === part.id}
          onLyrics={lyrics => setPart(part.id, { lyrics, slideTexts: undefined })}
          onColor={color => setPart(part.id, { color })}
          onCaret={slide => setCursor({ partId: part.id, slide })}
          onArrange={() => update({ arrangement: [...song.arrangement, part.id] })}
          onDelete={() => update({ parts: song.parts.filter(p => p.id !== part.id), arrangement: song.arrangement.filter(id => id !== part.id) })}/>)}
      </div>
    </section>
    <aside className="song-panel" aria-label="Song details and preview">
      {notice}
      <div className="song-preview">
        <SlidePreview text={shownSlides[shownIndex] || ''} background={song.background}/>
        <div className="song-preview-caption"><span>As on the projection</span>{shownPart && <span className="tabular">{partCode(shownPart)} · slide {shownIndex + 1} of {Math.max(1, shownSlides.length)}</span>}</div>
      </div>
      <label className="song-field song-title-field">Title<input aria-label="Song title" placeholder="Untitled song" value={title} autoFocus={autoFocusTitle} onChange={e => onTitle(e.target.value)}/></label>
      <label className="song-field">Author / singer<input aria-label="Author / Singer" placeholder="Add author or singer" value={song.authorSinger || ''} onChange={e => update({ authorSinger: e.target.value })}/></label>
      <Arrangement song={song} onChange={arrangement => update({ arrangement })} onPreview={partId => setCursor({ partId, slide: 0 })}/>
      <div className="song-field">Background<BackgroundButton value={song.background} backgrounds={backgrounds} onChange={background => update({ background })}/></div>
    </aside>
  </div>;
}

function LyricPart({ part, active, onLyrics, onColor, onCaret, onArrange, onDelete }: { part: Part; active: boolean; onLyrics: (lyrics: string) => void; onColor: (color: string) => void; onCaret: (slide: number) => void; onArrange: () => void; onDelete: () => void }) {
  const input = useRef<HTMLTextAreaElement>(null);
  const count = slides(part).length;
  useLayoutEffect(() => {
    const textarea = input.current;
    if (!textarea) return;
    const resize = () => { textarea.style.height = '0px'; textarea.style.height = `${textarea.scrollHeight}px`; };
    resize();
    let width = 0;
    const observer = new ResizeObserver(entries => { const next = entries[0].contentRect.width; if (width !== next) { width = next; resize(); } });
    observer.observe(textarea.parentElement!);
    return () => observer.disconnect();
  }, [part.lyrics]);
  const caret = () => { const t = input.current; if (t) onCaret(slideAtCaret(t.value, t.selectionStart)); };
  return <article className={`lyric-part ${active ? 'is-active' : ''}`} style={{ '--part-color': partColor(part) } as CSSProperties}>
    <header>
      <strong className="part-code-badge">{partCode(part)}</strong><span className="lyric-part-name">{part.type} {part.number}</span><small>{count} {count === 1 ? 'slide' : 'slides'}</small>
      <span className="lyric-part-tools">
        <input type="color" aria-label={`${part.type} ${part.number} colour`} title="Part colour" value={partColor(part)} onChange={e => onColor(e.target.value)}/>
        <button className="text-key" onClick={onArrange} title="Add another occurrence to the arrangement"><Icon name="plus"/>Arrangement</button>
        <button className="icon-button" aria-label={`Delete ${part.type} ${part.number}`} onClick={onDelete}><Icon name="close"/></button>
      </span>
    </header>
    <textarea ref={input} id={`lyrics-${part.id}`} aria-label={`${part.type} ${part.number} lyrics`} rows={3} placeholder={'Lines on the same slide\nstay together.\n\nA blank line starts the next slide.'} value={part.lyrics}
      onChange={e => { onLyrics(e.target.value); onCaret(slideAtCaret(e.target.value, e.target.selectionStart)); }} onSelect={caret} onFocus={caret} onClick={caret} onKeyUp={caret}/>
  </article>;
}

function Arrangement({ song, onChange, onPreview }: { song: SongDocument; onChange: (arrangement: string[]) => void; onPreview: (partId: string) => void }) {
  const [dragging, setDragging] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!adding) return;
    const away = (e: PointerEvent) => { if (!menu.current?.contains(e.target as Node)) setAdding(false); };
    document.addEventListener('pointerdown', away);
    return () => document.removeEventListener('pointerdown', away);
  }, [adding]);
  const parts = arrangedParts(song);
  return <div className="song-field">Arrangement
    <div className="arrangement-chips" role="list" aria-label="Arrangement">
      {parts.map((part, index) => <span key={`${part.id}-${index}`} role="listitem" className={`arrangement-chip ${dragging === index ? 'is-dragging' : ''}`} style={{ '--part-color': partColor(part) } as CSSProperties} draggable
        onDragStart={e => { setDragging(index); e.dataTransfer.setData('text/plain', String(index)); e.dataTransfer.effectAllowed = 'move'; }}
        onDragEnd={() => setDragging(null)} onDragOver={e => e.preventDefault()}
        onDrop={e => { e.preventDefault(); if (dragging === null || dragging === index) return; const next = [...song.arrangement]; const [id] = next.splice(dragging, 1); next.splice(index, 0, id); onChange(next); setDragging(null); }}>
        <button className="arrangement-code" onClick={() => onPreview(part.id)} title={`${part.type} ${part.number} · drag to reorder`}>{partCode(part)}</button>
        <button className="arrangement-remove" aria-label={`Remove ${partCode(part)}, position ${index + 1}`} onClick={() => onChange(song.arrangement.filter((_, i) => i !== index))}><Icon name="close"/></button>
      </span>)}
      <div className="arrangement-add" ref={menu}>
        <button className="arrangement-add-key" aria-haspopup="menu" aria-expanded={adding} disabled={!song.parts.length} onClick={() => setAdding(!adding)}><Icon name="plus"/>Part</button>
        {adding && <div className="arrangement-menu" role="menu">{song.parts.map(part => <button role="menuitem" key={part.id} style={{ '--part-color': partColor(part) } as CSSProperties} onClick={() => { onChange([...song.arrangement, part.id]); setAdding(false); }}><span className="part-dot"/>{partCode(part)}<small>{part.type} {part.number}</small></button>)}</div>}
      </div>
    </div>
    {!parts.length && <small className="field-hint">Add parts to set the order they are sung in.</small>}
  </div>;
}

export function BackgroundButton({ value, backgrounds, onChange }: { value: string; backgrounds: Media[]; onChange: (path: string) => void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) setOpen(false); };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); setOpen(false); } };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape, true);
    return () => { document.removeEventListener('pointerdown', away); document.removeEventListener('keydown', escape, true); };
  }, [open]);
  const current = backgroundName(value, backgrounds);
  return <div className="background-button-wrap" ref={root}>
    <button className="background-button" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(!open)}>
      <span className="thumb">{value ? <Background path={value} fit="cover"/> : <span className="thumb-tile">None</span>}</span>
      <span className="background-button-text"><strong>{current.name}</strong><small>{current.kind}</small></span>
      <Icon name="chevron"/>
    </button>
    {open && <div className="background-popover" role="dialog" aria-label="Choose a background" data-covers-monitors="">
      {backgroundChoices(backgrounds).map(group => <section key={group.group}><h3>{group.group}</h3><div className="background-grid">
        {group.items.map(choice => <button key={choice.path || 'none'} aria-pressed={choice.path === value} onClick={() => { onChange(choice.path); setOpen(false); }}>
          <span className="thumb">{choice.path ? <Background path={choice.path} fit="cover"/> : <span className="thumb-tile">None</span>}</span>
          <span>{choice.name}</span>
        </button>)}
      </div></section>)}
      {!backgrounds.some(m => m.kind !== 'audio') && <p className="field-hint">Import images or videos in the Library to use them as backgrounds.</p>}
    </div>}
  </div>;
}
