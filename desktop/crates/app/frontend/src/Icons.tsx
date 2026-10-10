import type { ReactNode } from 'react';

type IconName = 'settings' | 'play' | 'pause' | 'volume' | 'mute' | 'edit' | 'chevron' | 'grip' | 'logo' | 'black' | 'clear' | 'live' | 'video' | 'library' | 'plus' | 'audio' | 'up' | 'down' | 'close';
export function Icon({ name, className = '' }: { name: IconName; className?: string }) {
  const paths: Record<IconName, ReactNode> = {
    settings: <><path d="m9 3-1 3-3 1-2 3 2 3 1 3 3 2 3-1 3-1 3-3-1-3-1-3-3-2Z"/><circle cx="12" cy="12" r="3"/></>,
    play: <path d="m7 4 14 8-14 8Z"/>,
    pause: <><path d="M8 4v16M16 4v16" strokeWidth="4"/></>,
    volume: <><path d="M3 9h4l5-4v14l-5-4H3Z"/><path d="M16 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14"/></>,
    mute: <><path d="M3 9h4l5-4v14l-5-4H3Z"/><path d="m16 9 6 6m0-6-6 6"/></>,
    edit: <><path d="m16 3 5 5-12 12-6 1 1-6Z"/><path d="m14 5 5 5"/></>,
    chevron: <path d="m8 10 4 4 4-4"/>,
    grip: <>{[5, 12, 19].flatMap(y => [9, 15].map(x => <circle key={`${x}-${y}`} cx={x} cy={y} r="1" fill="currentColor" stroke="none"/>))}</>,
    logo: <><rect x="3" y="3" width="18" height="18" rx="2"/><path d="m5 17 5-6 4 4 3-3 4 5"/><circle cx="16" cy="8" r="1.5"/></>,
    black: <rect x="3" y="4" width="18" height="16" fill="currentColor" stroke="none"/>,
    clear: <rect x="3" y="4" width="18" height="16"/>,
    live: <><rect x="2" y="3" width="20" height="14" rx="1"/><path d="M8 21h8m-4-4v4"/><path d="m10 7 5 3-5 3Z" fill="currentColor" stroke="none"/></>,
    video: <><rect x="3" y="3" width="18" height="18" rx="1"/><path d="M7 3v18m10-18v18M3 8h4m-4 8h4M17 8h4m-4 8h4m-11-8 5 4-5 4Z"/></>,
    library: <><path d="M4 4h4v16H4zm7 0h4v16h-4zm7 0 4 1-3 15-4-1"/></>,
    audio: <><path d="M9 18V5l11-2v13M9 8l11-2"/><ellipse cx="6" cy="18" rx="3" ry="2"/><ellipse cx="17" cy="16" rx="3" ry="2"/></>,
    up: <path d="m6 14 6-6 6 6"/>,
    down: <path d="m6 10 6 6 6-6"/>,
    close: <path d="m6 6 12 12M6 18 18 6"/>,
    plus: <path d="M12 5v14M5 12h14"/>,
  };
  return <svg className={`ui-icon ${className}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
