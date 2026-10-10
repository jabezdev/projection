import { useVideoPoster } from './useVideoPoster';
import { Icon } from './Icons';
import VideoDesk from './VideoDesk';
import { defaultVideoOptions, type VideoOptions } from './models';
export default function ScheduleVideo({ title, path, active, onSelect, options, onOptions, controls = false }: {
  title: string; path: string; active: boolean; onSelect: () => void; options?: VideoOptions; onOptions?: (options: VideoOptions) => void; controls?: boolean;
}) {
  const poster = useVideoPoster(path);
  const value = { ...defaultVideoOptions, ...options };
  const update = (patch: Partial<VideoOptions>) => onOptions?.({ ...value, ...patch });
  const thumbnail = poster ? <img src={poster} alt="" draggable={false}/> : <Icon name="video"/>;
  if (!controls) return <button className={`schedule-video ${active ? 'is-active' : ''}`} onClick={onSelect} aria-label={`Cue video ${title}`} aria-pressed={active}><span className="video-symbol">{poster ? <img src={poster} alt="" draggable={false}/> : <span className="video-poster-placeholder">VIDEO</span>}</span><span className="video-details"><strong>{title}</strong><span>{path.split(/[\\/]/).pop()}</span></span></button>;
  return <section className={`video-deck ${active ? 'is-live' : ''}`} aria-label={`${title} video controls`}>
    <div className="video-deck-header"><div><span className="video-deck-kicker"><Icon name="video"/> VIDEO <span className="video-deck-status">{active ? 'ON AIR' : 'READY TO CUE'}</span></span><h3>{title}</h3><p title={path}>{path.split(/[\\/]/).pop()}</p></div><button className="video-cue-button" disabled={active} onClick={onSelect}><Icon name="live"/>{active ? 'Cued' : 'Cue video'}</button></div>
    <div className="video-deck-body"><button className="video-deck-poster" onClick={onSelect} aria-label={`Cue video ${title}`}>{thumbnail}</button><div className="video-deck-settings"><label className="video-switch"><input type="checkbox" checked={value.confidence} onChange={e => update({ confidence: e.target.checked })}/><span>Show on confidence<small>Muted stage playback</small></span></label><label className="video-switch"><input type="checkbox" checked={value.looping} onChange={e => update({ looping: e.target.checked })}/><span>Loop video<small>Repeat the selected clip</small></span></label></div></div>
    <VideoDesk path={path} active={active} options={value} onOptions={update} noun="video"/>
    <ClipSettings options={value} update={update} noun="Video"/>
  </section>;
}
/** Clip bounds and speed, saved with the item and applied to its session. */
export function ClipSettings({ options, update, noun }: { options: VideoOptions; update: (patch: Partial<VideoOptions>) => void; noun: string }) {
  return <div className="video-clip-settings"><label>Start <span className="video-number"><input aria-label={`${noun} start in seconds`} type="number" min={0} step={.1} value={options.start_ms / 1000} onChange={e => { const start_ms = Math.max(0, Math.round(Number(e.target.value) * 1000)); update({ start_ms, end_ms: options.end_ms !== null && options.end_ms <= start_ms ? null : options.end_ms }); }}/><span>sec</span></span></label><label>End <span className="video-number"><input aria-label={`${noun} end in seconds`} type="number" min={options.start_ms / 1000 + .1} step={.1} placeholder="Full length" value={options.end_ms === null ? '' : options.end_ms / 1000} onChange={e => update({ end_ms: e.target.value === '' ? null : Math.max(options.start_ms + 100, Math.round(Number(e.target.value) * 1000)) })}/><span>sec</span></span></label><label>Speed<select aria-label={`${noun} playback speed`} value={options.rate_percent} onChange={e => update({ rate_percent: Number(e.target.value) })}>{[50,75,100,125,150,200].map(rate => <option key={rate} value={rate}>{rate / 100}×</option>)}</select></label></div>;
}
