import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Icon } from '../ui/Icons';
import { newId, type MediaFile, type Section, type SectionKind } from '../api/models';
import { arranged, kindColor, sectionCode, sectionColor, sectionKinds, sectionLabel, slideAtCaret, slides, type SongDraft } from './song';
import { Background, SlidePreview } from './SlidePreview';
import { backgroundChoices, backgroundName } from '../outputs/preview';
import { backgroundPath } from '../api/resolve';

type Props = {
  title: string;
  onTitle: (title: string) => void;
  song: SongDraft;
  onSong: (song: SongDraft) => void;
  backgrounds: MediaFile[];
  notice?: ReactNode;
  details?: ReactNode;
  autoFocusTitle?: boolean;
};

type Part = Section;

export default function SongWorkspace({ title, onTitle, song, onSong, backgrounds, notice, details, autoFocusTitle }: Props) {
  const [cursor, setCursor] = useState<{ partId: string; slide: number } | null>(null);
  const update = (patch: Partial<SongDraft>) => onSong({ ...song, ...patch });
  const setPart = (id: string, patch: Partial<Part>) => update({ sections: song.sections.map(p => p.id === id ? { ...p, ...patch } : p) });
  function addPart(kind: SectionKind) {
    const id = newId();
    const numbered = kind !== 'other';
    const number = numbered ? Math.max(0, ...song.sections.filter(p => p.kind === kind).map(p => p.number ?? 1)) + 1 : undefined;
    onSong({ ...song, sections: [...song.sections, { id, kind, number, lyrics: '' }], arrangement: [...song.arrangement, id] });
    requestAnimationFrame(() => document.getElementById(`lyrics-${id}`)?.focus());
  }
  const cursorPart = cursor && song.sections.find(p => p.id === cursor.partId);
  const shownPart = cursorPart || arranged(song)[0] || song.sections[0];
  const shownSlides = shownPart ? slides(shownPart) : [];
  const shownIndex = cursorPart ? Math.min(cursor.slide, Math.max(0, shownSlides.length - 1)) : 0;
  return <div className="song-workspace">
    <section className="song-text" aria-label="Lyrics">
      <div className="song-text-head">
        <span className="label-text">Lyrics <small>A blank line starts a new slide</small></span>
        <div className="add-parts" role="group" aria-label="Add a part">{sectionKinds.map(({ kind, label }) => <button key={kind} style={{ '--part-color': kindColor(kind) } as CSSProperties} onClick={() => addPart(kind)}><span className="part-dot"/>{label}</button>)}</div>
      </div>
      <div className="song-text-scroll">
        {!song.sections.length && <p className="empty-panel">Add a part above, then type or paste its lyrics. Each part is added to the arrangement.</p>}
        {song.sections.map(part => <LyricPart key={part.id} part={part} active={shownPart?.id === part.id}
          onLyrics={lyrics => setPart(part.id, { lyrics })}
          onColor={color => setPart(part.id, { color })}
          onName={name => setPart(part.id, { name: name || undefined })}
          onCaret={slide => setCursor({ partId: part.id, slide })}
          onArrange={() => update({ arrangement: [...song.arrangement, part.id] })}
          onDelete={() => update({ sections: song.sections.filter(p => p.id !== part.id), arrangement: song.arrangement.filter(id => id !== part.id) })}/>)}
      </div>
    </section>
    <aside className="song-panel" aria-label="Song details and preview">
      {notice}
      <div className="song-preview">
        <SlidePreview text={shownSlides[shownIndex] || ''} background={backgroundPath(song.background, backgrounds)}/>
        <div className="song-preview-caption"><span>As on the projection</span>{shownPart && <span className="tabular">{sectionCode(shownPart)} · slide {shownIndex + 1} of {Math.max(1, shownSlides.length)}</span>}</div>
      </div>
      <label className="song-field song-title-field">Title<input aria-label="Song title" placeholder="Untitled song" value={title} autoFocus={autoFocusTitle} onChange={e => onTitle(e.target.value)}/></label>
      {details}
      <Arrangement song={song} onChange={arrangement => update({ arrangement })} onPreview={partId => setCursor({ partId, slide: 0 })}/>
      <div className="song-field">Background<BackgroundButton value={song.background ?? ''} backgrounds={backgrounds} onChange={background => update({ background: background || undefined })}/></div>
    </aside>
  </div>;
}

function LyricPart({ part, active, onLyrics, onColor, onName, onCaret, onArrange, onDelete }: { part: Part; active: boolean; onLyrics: (lyrics: string) => void; onColor: (color: string) => void; onName: (name: string) => void; onCaret: (slide: number) => void; onArrange: () => void; onDelete: () => void }) {
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
  const label = sectionLabel(part);
  return <article className={`lyric-part ${active ? 'is-active' : ''}`} style={{ '--part-color': sectionColor(part) } as CSSProperties}>
    <header>
      <strong className="part-code-badge">{sectionCode(part)}</strong>{part.kind === 'other' ? <input className="lyric-part-name" aria-label="Part name" placeholder="Part name" value={part.name ?? ''} onChange={e => onName(e.target.value)}/> : <span className="lyric-part-name">{label}</span>}<small>{count} {count === 1 ? 'slide' : 'slides'}</small>
      <span className="lyric-part-tools">
        <input type="color" aria-label={`${label} colour`} title="Part colour" value={sectionColor(part)} onChange={e => onColor(e.target.value)}/>
        <button className="text-key" onClick={onArrange} title="Add another occurrence to the arrangement"><Icon name="plus"/>Arrangement</button>
        <button className="icon-button" aria-label={`Delete ${label}`} onClick={onDelete}><Icon name="close"/></button>
      </span>
    </header>
    <textarea ref={input} id={`lyrics-${part.id}`} aria-label={`${label} lyrics`} rows={3} placeholder={'Lines on the same slide\nstay together.\n\nA blank line starts the next slide.'} value={part.lyrics}
      onChange={e => { onLyrics(e.target.value); onCaret(slideAtCaret(e.target.value, e.target.selectionStart)); }} onSelect={caret} onFocus={caret} onClick={caret} onKeyUp={caret}/>
  </article>;
}

function Arrangement({ song, onChange, onPreview }: { song: SongDraft; onChange: (arrangement: string[]) => void; onPreview: (partId: string) => void }) {
  const [dragging, setDragging] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!adding) return;
    const away = (e: PointerEvent) => { if (!menu.current?.contains(e.target as Node)) setAdding(false); };
    document.addEventListener('pointerdown', away);
    return () => document.removeEventListener('pointerdown', away);
  }, [adding]);
  const parts = arranged(song);
  return <div className="song-field">Arrangement
    <div className="arrangement-chips" role="list" aria-label="Arrangement">
      {parts.map((part, index) => <span key={`${part.id}-${index}`} role="listitem" className={`arrangement-chip ${dragging === index ? 'is-dragging' : ''}`} style={{ '--part-color': sectionColor(part) } as CSSProperties} draggable
        onDragStart={e => { setDragging(index); e.dataTransfer.setData('text/plain', String(index)); e.dataTransfer.effectAllowed = 'move'; }}
        onDragEnd={() => setDragging(null)} onDragOver={e => e.preventDefault()}
        onDrop={e => { e.preventDefault(); if (dragging === null || dragging === index) return; const next = [...song.arrangement]; const [id] = next.splice(dragging, 1); next.splice(index, 0, id); onChange(next); setDragging(null); }}>
        <button className="arrangement-code" onClick={() => onPreview(part.id)} title={`${sectionLabel(part)} · drag to reorder`}>{sectionCode(part)}</button>
        <button className="arrangement-remove" aria-label={`Remove ${sectionCode(part)}, position ${index + 1}`} onClick={() => onChange(song.arrangement.filter((_, i) => i !== index))}><Icon name="close"/></button>
      </span>)}
      <div className="arrangement-add" ref={menu}>
        <button className="arrangement-add-key" aria-haspopup="menu" aria-expanded={adding} disabled={!song.sections.length} onClick={() => setAdding(!adding)}><Icon name="plus"/>Part</button>
        {adding && <div className="arrangement-menu" role="menu">{song.sections.map(part => <button role="menuitem" key={part.id} style={{ '--part-color': sectionColor(part) } as CSSProperties} onClick={() => { onChange([...song.arrangement, part.id]); setAdding(false); }}><span className="part-dot"/>{sectionCode(part)}<small>{sectionLabel(part)}</small></button>)}</div>}
      </div>
    </div>
    {!parts.length && <small className="field-hint">Add parts to set the order they are sung in.</small>}
  </div>;
}

export function BackgroundButton({ value, backgrounds, onChange }: { value: string; backgrounds: MediaFile[]; onChange: (value: string) => void }) {
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
      <span className="thumb">{value ? <Background path={backgroundPath(value, backgrounds)} fit="cover"/> : <span className="thumb-tile">None</span>}</span>
      <span className="background-button-text"><strong>{current.name}</strong><small>{current.kind}</small></span>
      <Icon name="chevron"/>
    </button>
    {open && <div className="background-popover" role="dialog" aria-label="Choose a background" data-covers-monitors="">
      {backgroundChoices(backgrounds).map(group => <section key={group.group}><h3>{group.group}</h3><div className="background-grid">
        {group.items.map(choice => <button key={choice.value || 'none'} aria-pressed={choice.value === value} onClick={() => { onChange(choice.value); setOpen(false); }}>
          <span className="thumb">{choice.path ? <Background path={choice.path} fit="cover"/> : <span className="thumb-tile">None</span>}</span>
          <span>{choice.name}</span>
        </button>)}
      </div></section>)}
      {!backgrounds.some(m => m.kind !== 'audio') && <p className="field-hint">Import images or videos in the Library to use them as backgrounds.</p>}
    </div>}
  </div>;
}
