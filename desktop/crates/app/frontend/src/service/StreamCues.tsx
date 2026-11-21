import { useEffect, useState } from 'react';
import { Icon } from '../ui/Icons';
import { invoke, isTauri } from '../api/tauri';
import { defaultCaptureOptions, defaultVideoOptions, type SessionItem } from '../api/models';
import { Background } from '../song/SlidePreview';
import { MediaThumb } from '../library/Thumbs';

type Cue = (index?: number) => void;
const clock = (ms: number) => { const s = Math.max(0, Math.round(ms / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };

function CueTile({ active, label, onClick, children }: { active: boolean; label: string; onClick: () => void; children: React.ReactNode }) {
  return <button className={`cue-tile ${active ? 'is-live' : ''}`} aria-label={label} aria-pressed={active} onClick={onClick}>
    {children}
    {active && <span className="cue-tile-air"><span className="lamp"/>On air</span>}
  </button>;
}

export function ImageCue({ item, active, onCue }: { item: SessionItem; active: boolean; onCue: Cue }) {
  if (item.kind.kind !== 'image') return null;
  return <div className="stream-cues single"><CueTile active={active} label={`Put ${item.title || 'image'} on air`} onClick={() => onCue()}><div className="thumb"><Background path={item.kind.data.path}/></div></CueTile></div>;
}

export function SlidesCue({ item, active, step, onCue }: { item: SessionItem; active: boolean; step?: number | null; onCue: Cue }) {
  if (item.kind.kind !== 'presentation') return null;
  const paths = item.kind.data.slides || [item.kind.data.path];
  return <div className="stream-cues grid">{paths.map((path, index) => <div key={index} data-step-index={index}>
    <CueTile active={active && step === index} label={`Put slide ${index + 1} of ${item.title || 'slides'} on air`} onClick={() => onCue(index)}>
      <div className="thumb"><Background path={path} fit="contain"/><span className="thumb-badge">{index + 1}</span></div>
    </CueTile>
  </div>)}</div>;
}

export function MediaCue({ item, active, onCue }: { item: SessionItem; active: boolean; onCue: Cue }) {
  if (item.kind.kind !== 'video' && item.kind.kind !== 'audio') return null;
  const options = { ...defaultVideoOptions, ...item.kind.data.options };
  const notes = [
    options.looping && 'Loops',
    item.kind.kind === 'video' && options.confidence && 'On confidence',
    options.start_ms > 0 && `From ${clock(options.start_ms)}`,
    options.end_ms !== null && `To ${clock(options.end_ms)}`,
    options.rate_percent !== 100 && `${options.rate_percent / 100}× speed`,
    options.volume !== 100 && `Volume ${options.volume}%`,
  ].filter(Boolean) as string[];
  return <div className="stream-cues media">
    <CueTile active={active} label={`Put ${item.title || item.kind.kind} on air`} onClick={() => onCue()}>
      <MediaThumb kind={item.kind.kind} path={item.kind.data.path}/>
      {!active && <span className="cue-tile-play"><Icon name="play"/></span>}
    </CueTile>
    <div className="media-cue-notes">
      <span>{active ? 'On air · controls are in the bar below' : `Click to ${item.kind.kind === 'video' ? 'show and play' : 'play'}`}</span>
      {notes.length > 0 && <div className="note-chips">{notes.map(note => <span key={note}>{note}</span>)}</div>}
    </div>
  </div>;
}

type CaptureStatus = { id: string; name: string; error: string | null };

export function CaptureCueTile({ item, active, onCue }: { item: SessionItem; active: boolean; onCue: Cue }) {
  const [problem, setProblem] = useState<string | null>(null);
  const source = item.kind.kind === 'capture' ? item.kind.data.source : null;
  useEffect(() => {
    if (!active || !isTauri() || !source) return;
    const check = () => void invoke<CaptureStatus[]>('get_capture_status').then(list => setProblem(list.find(s => s.id === source.id)?.error ?? null)).catch(() => {});
    check();
    const timer = window.setInterval(check, 1000);
    return () => { window.clearInterval(timer); setProblem(null); };
  }, [active, source]);
  if (item.kind.kind !== 'capture' || !source) return null;
  const options = { ...defaultCaptureOptions, ...item.kind.data.options };
  const screens = [options.projection && 'Projection', options.confidence && 'Confidence'].filter(Boolean).join(' and ');
  return <div className="stream-cues media">
    <CueTile active={active} label={`Put ${source.name} on air`} onClick={() => onCue()}>
      <div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>{source.name}</small></span></div>
    </CueTile>
    <div className="media-cue-notes">
      <span>{problem ? <strong className="cue-problem">Capture problem: {problem}</strong> : active ? `Live on ${screens}` : `Click to show it live on ${screens}`}</span>
      <div className="note-chips"><span>{source.kind === 'window' ? 'Window' : 'Screen'}</span>{options.projection && <span>{options.projection_fit === 'fill' ? 'Fills' : 'Fits'} the projection</span>}</div>
    </div>
  </div>;
}
