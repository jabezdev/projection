import { memo, useMemo } from 'react';
import type { SessionItem, VideoOptions } from './models';
import { arrangedParts, decodeSong, partCode, partColor, slides, type SongDocument } from './song';
import { PresentationCues } from './PresentationEditor';
import ScheduleImage from './ScheduleImage';
import ScheduleAudio from './ScheduleAudio';
import ScheduleVideo from './ScheduleVideo';
type Options = (id: number, options: VideoOptions) => void;
type Cue = (item: SessionItem, index?: number) => void;
const StreamItem = memo(function StreamItem({ item, active, step, onSelect, onVideoOptions }: { item: SessionItem; active: boolean; step?: number | null; onSelect: Cue; onVideoOptions: Options }) {
  const parts = useMemo(() => {
    if (item.kind.kind !== 'text') return [];
    const song: SongDocument = item.song ? decodeSong(item.song) : { format: 'magi.song.v1', slideSeparator: 'blank-line', background: item.background || '', parts: [{ id: 'legacy', type: 'Verse', number: 1, lyrics: item.kind.data.steps.join('\n\n'), slideTexts: item.kind.data.steps }], arrangement: ['legacy'] };
    let offset = 0;
    return arrangedParts(song).map(part => { const lines = slides(part); const start = offset; offset += lines.length; return { part, lines, start }; });
  }, [item]);
  return <article className="stream-song" data-item-id={item.id}>{item.kind.kind !== 'video' && item.kind.kind !== 'audio' && <h2>{item.title || item.kind.kind}</h2>}{item.kind.kind === 'text' ? parts.map(({ part, lines, start }, occurrence) => <section className="stream-part" key={`${part.id}-${occurrence}`}><div className={`part-rail part-${part.type.toLowerCase()}`} style={{ backgroundColor: partColor(part) }} title={`${part.type} ${part.number}`}><span>{partCode(part)}</span></div><div className="part-lines">{lines.map((line, index) => <button data-step-index={start + index} className={`lyric-line ${active && step === start + index ? 'is-live' : ''}`} key={index} onClick={() => onSelect(item, start + index)}><span className="line-code">{partCode(part)}.{index + 1}</span><span>{line}</span></button>)}</div></section>) : item.kind.kind === 'presentation' ? <PresentationCues item={item} active={active} step={step} onSelect={index => onSelect(item, index)}/> : item.kind.kind === 'image' ? <ScheduleImage title={item.title || 'Image'} path={item.kind.data.path} active={active} onSelect={() => onSelect(item)}/> : item.kind.kind === 'audio' ? <ScheduleAudio title={item.title || 'Audio'} path={item.kind.data.path} active={active} options={item.kind.data.options} onOptions={options => onVideoOptions(item.id, options)} onSelect={() => onSelect(item)}/> : item.kind.kind === 'video' ? <ScheduleVideo controls options={item.kind.data.options} onOptions={options => onVideoOptions(item.id, options)} title={item.title || 'Untitled video'} path={item.kind.data.path} active={active} onSelect={() => onSelect(item)}/> : null}</article>;
});
const LyricStream = memo(function LyricStream({ items, activeId, activeStep, onSelect, onVideoOptions }: { items?: SessionItem[]; activeId?: number | null; activeStep?: number | null; onSelect: Cue; onVideoOptions: Options }) {
  return <>{items?.map(item => <StreamItem key={item.id} item={item} active={activeId === item.id} step={activeId === item.id ? activeStep : null} onSelect={onSelect} onVideoOptions={onVideoOptions}/>)}{!items?.length && <p className="empty-panel">Your service lyrics will appear here.</p>}</>;
});
export default LyricStream;
