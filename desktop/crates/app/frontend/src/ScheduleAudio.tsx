import { Icon } from './Icons';
import VideoDesk from './VideoDesk';
import { ClipSettings } from './ScheduleVideo';
import { defaultVideoOptions, type VideoOptions } from './models';
// Audio plays in the native media engine; this card only sends transport commands.
export default function ScheduleAudio({ title, path, active, onSelect, options, onOptions }: { title: string; path: string; active: boolean; onSelect: () => void; options?: VideoOptions; onOptions: (options: VideoOptions) => void }) {
  const value = { ...defaultVideoOptions, ...options };
  const update = (patch: Partial<VideoOptions>) => onOptions({ ...value, ...patch });
  return <section className={`video-deck audio-deck ${active ? 'is-live' : ''}`} aria-label={`${title} audio controls`}>
    <div className="video-deck-header"><div><span className="video-deck-kicker"><Icon name="audio"/> AUDIO <span className="video-deck-status">{active ? 'ON AIR' : 'READY TO CUE'}</span></span><h3>{title}</h3><p title={path}>{path.split(/[\\/]/).pop()}</p></div><button className="video-cue-button" disabled={active} onClick={onSelect}><Icon name="live"/>{active ? 'Cued' : 'Cue audio'}</button></div>
    <div className="audio-deck-settings"><label className="video-switch"><input type="checkbox" checked={value.looping} onChange={e => update({ looping: e.target.checked })}/><span>Loop audio<small>Repeat the selected clip</small></span></label></div>
    <VideoDesk path={path} active={active} options={value} onOptions={update} noun="audio"/>
    <ClipSettings options={value} update={update} noun="Audio"/>
  </section>;
}
