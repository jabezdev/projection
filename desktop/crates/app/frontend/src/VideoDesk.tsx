import { useState } from 'react';
import { Icon } from './Icons';
import { running, sendMedia, usePlayback, type MediaAction, type PlaybackState } from './playback';
import type { VideoOptions } from './models';
const timeLabel = (seconds: number) => `${Math.floor((seconds || 0) / 60)}:${String(Math.floor((seconds || 0) % 60)).padStart(2, '0')}`;
type Props = { path: string; active: boolean; options: VideoOptions; onOptions: (patch: Partial<VideoOptions>) => void; noun: 'video' | 'audio' };
export default function VideoDesk(props: Props) {
  const playback = usePlayback();
  const [error, setError] = useState('');
  // Controls only address the engine's session for this card's file.
  const session = props.active && playback?.path === props.path ? playback : null;
  function send(action: MediaAction) {
    if (!session) return;
    setError('');
    void sendMedia(session.session_id, action).catch(e => setError(String(e)));
  }
  return <><Transport {...props} session={session} send={send}/>{(error || session?.error) && <p className="video-deck-error" role="status">{error || session?.error}</p>}</>;
}
function Transport({ options, onOptions, noun, session, send }: Props & { session: PlaybackState | null; send: (action: MediaAction) => void }) {
  const live = Boolean(session);
  const duration = (session?.duration_ms ?? 0) / 1000;
  const time = (session?.position_ms ?? options.start_ms) / 1000;
  const start = Math.min(options.start_ms / 1000, duration || options.start_ms / 1000);
  const end = Math.min(options.end_ms === null ? duration : options.end_ms / 1000, duration || Infinity) || start + 1;
  const paused = !session || !running(session);
  const title = noun === 'video' ? 'Video' : 'Audio';
  return <div className="video-deck-transport"><div className="video-timeline"><span>{timeLabel(time)}</span><input aria-label={`${title} seek`} type="range" min={start} max={Math.max(start + .01, end)} step={.01} value={Math.max(start, Math.min(time, end))} disabled={!live || !duration} onChange={e => send({ type: 'seek', payload: { position_ms: Math.round(Number(e.target.value) * 1000) } })}/><span>{timeLabel(duration)}</span></div>
    <div className="video-playback-row"><div className="video-playback-buttons"><button className="video-play-button" disabled={!live} aria-label={`${paused ? 'Play' : 'Pause'} ${noun}`} onClick={() => send({ type: paused ? 'play' : 'pause' })}><Icon name={paused ? 'play' : 'pause'}/><span>{paused ? 'Play' : 'Pause'}</span></button><button className="icon-button" disabled={!live} aria-label={`Stop ${noun}`} onClick={() => send({ type: 'stop' })}><Icon name="black"/></button></div><div className="video-volume"><button className="icon-button" disabled={!live} aria-label={`${session?.muted ? 'Unmute' : 'Mute'} ${noun}`} aria-pressed={Boolean(session?.muted)} onClick={() => send({ type: 'set_muted', payload: { muted: !session?.muted } })}><Icon name={session?.muted ? 'mute' : 'volume'}/></button><input aria-label={`${title} volume`} type="range" min={0} max={100} step={1} value={options.volume} onChange={e => onOptions({ volume: Number(e.target.value) })}/><output>{options.volume}%</output></div></div>
  </div>;
}
