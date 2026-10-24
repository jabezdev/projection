import { ScreenShare, ChevronDown, ChevronUp, Film, GripVertical, Image, Library, MonitorPlay, Music, Pause, Pencil, Play, Plus, Settings, Square, Volume2, VolumeX, X, type LucideIcon } from 'lucide-react';

// The operator UI's one icon set: Lucide (https://lucide.dev, ISC license).
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
  clear: Square,
  live: MonitorPlay,
  video: Film,
  library: Library,
  plus: Plus,
  audio: Music,
  up: ChevronUp,
  down: ChevronDown,
  close: X,
  capture: ScreenShare,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof icons;

export function Icon({ name, className = '' }: { name: IconName; className?: string }) {
  const Glyph = icons[name];
  // Black is the filled square, Clear the empty one.
  return <Glyph className={`ui-icon ${className}`} strokeWidth={1.6} fill={name === 'black' ? 'currentColor' : 'none'} aria-hidden="true"/>;
}
