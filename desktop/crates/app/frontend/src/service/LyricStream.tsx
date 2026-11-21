import { memo, useMemo } from 'react';
import type { SessionItem } from '../api/models';
import { arrangedParts, decodeSong, partCode, partColor, slides, type SongDocument } from '../song/song';
import { CaptureCueTile, ImageCue, MediaCue, SlidesCue } from './StreamCues';
type Cue = (item: SessionItem, index?: number) => void;
const StreamItem = memo(function StreamItem({ item, active, step, onSelect }: { item: SessionItem; active: boolean; step?: number | null; onSelect: Cue }) {
  const parts = useMemo(() => {
    if (item.kind.kind !== 'text') return [];
    const song: SongDocument = item.song ? decodeSong(item.song) : { format: 'magi.song.v1', slideSeparator: 'blank-line', background: item.background || '', parts: [{ id: 'legacy', type: 'Verse', number: 1, lyrics: item.kind.data.steps.join('\n\n'), slideTexts: item.kind.data.steps }], arrangement: ['legacy'] };
    let offset = 0;
    return arrangedParts(song).map(part => { const lines = slides(part); const start = offset; offset += lines.length; return { part, lines, start }; });
  }, [item]);
  if (item.kind.kind === 'section') return <div className="stream-section" data-item-id={item.id}><span>{item.title || 'Section'}</span></div>;
  const cue = (index?: number) => onSelect(item, index);
  return <article className="stream-song" data-item-id={item.id}><h2>{item.title || item.kind.kind}</h2>{item.kind.kind === 'text'
    ? parts.map(({ part, lines, start }, occurrence) => <section className="stream-part" key={`${part.id}-${occurrence}`}><div className={`part-rail part-${part.type.toLowerCase()}`} style={{ backgroundColor: partColor(part) }} title={`${part.type} ${part.number}`}><span>{partCode(part)}</span></div><div className="part-lines">{lines.map((line, index) => <button data-step-index={start + index} className={`lyric-line ${active && step === start + index ? 'is-live' : ''}`} key={index} onClick={() => onSelect(item, start + index)}><span className="line-code">{partCode(part)}.{index + 1}</span><span>{line}</span></button>)}</div></section>)
    : item.kind.kind === 'presentation' ? <SlidesCue item={item} active={active} step={step} onCue={cue}/>
    : item.kind.kind === 'image' ? <ImageCue item={item} active={active} onCue={cue}/>
    : item.kind.kind === 'capture' ? <CaptureCueTile item={item} active={active} onCue={cue}/>
    : <MediaCue item={item} active={active} onCue={cue}/>}</article>;
});
const LyricStream = memo(function LyricStream({ items, activeId, activeStep, onSelect }: { items?: SessionItem[]; activeId?: number | null; activeStep?: number | null; onSelect: Cue }) {
  return <>{items?.map(item => <StreamItem key={item.id} item={item} active={activeId === item.id} step={activeId === item.id ? activeStep : null} onSelect={onSelect}/>)}{!items?.length && <p className="empty-panel">Your service lyrics will appear here.</p>}</>;
});
export default LyricStream;
