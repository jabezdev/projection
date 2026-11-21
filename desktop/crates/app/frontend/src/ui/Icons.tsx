import { Ellipsis, Copy, Search, Upload, Trash2, ScreenShare, ChevronDown, Monitor, MonitorOff, RemoveFormatting, ChevronUp, Film, GripVertical, Image, Library, MonitorPlay, Music, Pause, Pencil, Play, Plus, Settings, Square, Volume2, VolumeX, X, type LucideIcon } from 'lucide-react';

const icons = {
  settings: Settings,
  play: Play,
  pause: Pause,
  volume: Volume2,
  mute: VolumeX,
  edit: Pencil,
  chevron: ChevronDown,
  grip: GripVertical,
  logo: Image,
  black: Square,
  clear: RemoveFormatting,
  live: MonitorPlay,
  video: Film,
  library: Library,
  plus: Plus,
  audio: Music,
  up: ChevronUp,
  down: ChevronDown,
  close: X,
  capture: ScreenShare,
  screen: Monitor,
  copy: Copy,
  more: Ellipsis,
  search: Search,
  import: Upload,
  trash: Trash2,
  'screen-off': MonitorOff,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof icons;

export function Icon({ name, className = '' }: { name: IconName; className?: string }) {
  const Glyph = icons[name];
  return <Glyph className={`ui-icon ${className}`} strokeWidth={1.6} fill={name === 'black' ? 'currentColor' : 'none'} aria-hidden="true"/>;
}
