import { useEffect, useRef, useState } from 'react';
import { Icon } from '../ui/Icons';
import { running, sendMedia, usePlayback, type MediaAction, type PlaybackState } from '../api/playback';
import type { SessionItem } from '../api/models';
import { MediaThumb } from '../library/Thumbs';

const FADE_MS = 2000;
const FADE_STEPS = 20;
const clock = (ms: number) => { const s = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };

function positionOf(p: PlaybackState, now: number) {
  if (!running(p) || p.phase !== 'playing') return p.position_ms;
  return p.position_ms + Math.max(0, now - p.position_at_ms) * p.rate_percent / 100;
}

export default function NowPlaying({ items, nextItemId }: { items: SessionItem[]; nextItemId?: number | null }) {
  const playback = usePlayback();
  return playback ? <MediaTransport key={playback.session_id} playback={playback} items={items} nextItemId={nextItemId}/> : null;
}

function MediaTransport({ playback, items, nextItemId }: { playback: PlaybackState; items: SessionItem[]; nextItemId?: number | null }) {
  const [now, setNow] = useState(() => Date.now());
  const [scrub, setScrub] = useState<number | null>(null);
  const [fading, setFading] = useState(false);
  const [error, setError] = useState('');
  const fade = useRef<number | undefined>(undefined);
  const live = playback && running(playback) && playback.phase === 'playing';
  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [live]);
  useEffect(() => () => window.clearInterval(fade.current), []);
  const session = playback.session_id;
  const send = (action: MediaAction) => { setError(''); void sendMedia(session, action).catch(e => setError(String(e))); };
  const item = items.find(i => i.id === playback.item_id);
  const next = items.find(i => i.id === nextItemId);
  const start = playback.start_ms;
  const end = playback.end_ms ?? playback.duration_ms ?? 0;
  const position = Math.min(Math.max(scrub ?? positionOf(playback, now), start), end || Infinity);
  const remaining = end ? end - position : 0;
  const paused = !running(playback) || playback.phase !== 'playing';
  function fadeOut() {
    if (fading) return;
    const from = playback.volume, id = playback.session_id;
    let step = 0;
    setFading(true);
    window.clearInterval(fade.current);
    fade.current = window.setInterval(() => {
      step++;
      void sendMedia(id, { type: 'set_volume', payload: { volume: Math.round(from * (1 - step / FADE_STEPS)) } }).catch(e => {
        window.clearInterval(fade.current);
        setFading(false);
        setError(String(e));
      });
      if (step >= FADE_STEPS) {
        window.clearInterval(fade.current);
        void sendMedia(id, { type: 'stop' }).then(() => sendMedia(id, { type: 'set_volume', payload: { volume: from } }))
          .catch(e => setError(String(e))).finally(() => setFading(false));
      }
    }, FADE_MS / FADE_STEPS);
  }
  return <section className={`now-playing ${paused ? 'is-paused' : 'is-playing'}`} aria-label="Now playing">
    <div className="np-what">
      <MediaThumb kind={playback.kind} path={playback.path}/>
      <span className="np-title"><strong>{item?.title || playback.path.split(/[\\/]/).pop()}</strong>
        <small>{playback.error ? <span className="cue-problem">{playback.error}</span> : playback.looping ? 'Loops until the next cue' : next ? <>Next: {next.title || next.kind.kind}</> : 'Last item in the service'}</small>
      </span>
    </div>
    <div className="np-transport">
      <button className="np-key" aria-label="Stop" title="Stop and go back to the start" disabled={fading} onClick={() => send({ type: 'stop' })}><Icon name="black"/></button>
      <button className="np-key np-play" aria-label={paused ? 'Play' : 'Pause'} title="Play / pause (Space)" disabled={fading} onClick={() => send({ type: paused ? 'play' : 'pause' })}><Icon name={paused ? 'play' : 'pause'}/></button>
      <span className="np-time tabular">{clock(position - start)}</span>
      <input className="np-scrub" type="range" aria-label="Position" min={start} max={Math.max(start + 1, end)} step={100} value={position} disabled={!end || fading}
        onChange={e => setScrub(Number(e.target.value))}
        onPointerUp={() => { if (scrub !== null) { send({ type: 'seek', payload: { position_ms: Math.round(scrub) } }); setScrub(null); } }}
        onKeyUp={() => { if (scrub !== null) { send({ type: 'seek', payload: { position_ms: Math.round(scrub) } }); setScrub(null); } }}
        style={{ '--progress': `${end > start ? ((position - start) / (end - start)) * 100 : 0}%` } as React.CSSProperties}/>
      <span className="np-remaining tabular" title="Time remaining">{end ? `−${clock(remaining)}` : '–:––'}</span>
    </div>
    <div className="np-level">
      <button className="np-key small" aria-label={playback.muted ? 'Unmute' : 'Mute'} aria-pressed={playback.muted} onClick={() => send({ type: 'set_muted', payload: { muted: !playback.muted } })}><Icon name={playback.muted ? 'mute' : 'volume'}/></button>
      <input className="np-volume" type="range" aria-label="Volume" min={0} max={100} value={playback.volume} disabled={fading} onChange={e => send({ type: 'set_volume', payload: { volume: Number(e.target.value) } })}/>
      <button className="np-fade" disabled={fading || paused} onClick={fadeOut} title="Lower the volume over two seconds, then stop">{fading ? 'Fading…' : 'Fade out'}</button>
    </div>
    {error && <p className="np-error" role="alert">{error}</p>}
  </section>;
}
