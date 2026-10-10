import { Icon } from './Icons';
import { mediaSource } from './media';
export default function ScheduleImage({ title, path, active, onSelect }: { title: string; path: string; active: boolean; onSelect: () => void }) {
  return <button className={`schedule-image ${active ? 'is-active' : ''}`} aria-label={`Cue image ${title}`} aria-pressed={active} onClick={onSelect}><img src={mediaSource(path)} alt=""/><span>Image<small>{title}</small></span><Icon name="live"/></button>;
}
