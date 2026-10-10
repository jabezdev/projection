import { useLayoutEffect, useRef, useState } from 'react';
import { partCode, slides, type Part } from './song';
import { mediaSource } from './media';

export default function SongPartEditor({ part, background, onChange }: { part: Part; background?: string; onChange: (lyrics: string) => void }) {
  const input = useRef<HTMLTextAreaElement>(null);
  const [selected, setSelected] = useState(0);
  const chunks = slides(part);
  const index = Math.min(selected, Math.max(0, chunks.length - 1));
  useLayoutEffect(() => {
    const textarea = input.current;
    if (!textarea) return;
    const resize = () => { textarea.style.height = '0px'; textarea.style.height = `${textarea.scrollHeight}px`; };
    resize();
    let width = 0;
    const observer = new ResizeObserver(entries => {
      const next = entries[0].contentRect.width;
      if (width !== next) { width = next; resize(); }
    });
    observer.observe(textarea.parentElement!);
    return () => observer.disconnect();
  }, [part.lyrics]);
  return <div className="part-editor-columns">
    <div className="part-text-input"><label htmlFor={`lyrics-${part.id}`}>Lyrics</label><textarea ref={input} id={`lyrics-${part.id}`} aria-label={`${part.type} ${part.number} lyrics`} rows={3} placeholder={'Lines on the same slide\nstay together.\n\nAn empty line starts the next slide.'} value={part.lyrics} onChange={event => onChange(event.target.value)}/></div>
    <div className="part-slide-selector" aria-label={`${part.type} ${part.number} slides`}><div className="part-column-label">Slides</div>{chunks.length ? chunks.map((text, i) => <button key={i} aria-pressed={i === index} onClick={() => setSelected(i)}><span>{partCode(part)}.{i + 1}</span><p>{text}</p></button>) : <p className="part-preview-empty">Enter lyrics to create slides.</p>}</div>
    <div className="part-projection"><div className="part-column-label">Projection preview{chunks.length > 0 && <span>{partCode(part)}.{index + 1}</span>}</div><div className="lyric-projection-preview" style={background ? { backgroundImage: `url(${JSON.stringify(mediaSource(background))})` } : undefined}><p>{chunks[index] || ''}</p></div></div>
  </div>;
}
