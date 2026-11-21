import { createContext, useContext, type CSSProperties } from 'react';
import type { MediaFile, SessionItem } from '../api/models';
import { defaultOutputConfig, shaderPresets, type OutputConfig } from '../settings/outputState';

export const OutputConfigContext = createContext<OutputConfig>(defaultOutputConfig);
export const useOutputConfig = () => useContext(OutputConfigContext);

const VIDEO = /\.(mp4|m4v|mov|mkv|webm|avi)$/i;
export const isVideoPath = (path: string) => VIDEO.test(path);
export const isShader = (path: string) => path.startsWith('shader:');

export function withOpacity(color: string, percent: number) {
  const alpha = Math.round(Math.max(0, Math.min(100, percent)) * 2.55).toString(16).padStart(2, '0');
  return /^#[0-9a-f]{6}$/i.test(color) ? `${color}${alpha}` : color;
}

export function shaderStyle(name: string, config: OutputConfig): CSSProperties {
  const a = config.shaderColorA, b = config.shaderColorB;
  if (name === 'aurora') return { background: `radial-gradient(120% 90% at 20% 110%, ${b}88, transparent 60%), radial-gradient(90% 80% at 85% -10%, ${b}55, transparent 60%), ${a}` };
  if (name === 'bokeh') return { background: `radial-gradient(circle at 25% 35%, ${b}66 0 6%, transparent 7%), radial-gradient(circle at 70% 60%, ${b}44 0 9%, transparent 10%), radial-gradient(circle at 45% 80%, ${b}33 0 5%, transparent 6%), ${a}` };
  return { background: `linear-gradient(135deg, ${a}, ${b})` };
}

type Choice = { value: string; path: string; name: string; kind: string };
export function backgroundChoices(backgrounds: MediaFile[]): { group: string; items: Choice[] }[] {
  return [
    { group: 'None', items: [{ value: '', path: '', name: 'No background', kind: 'Keeps the previous one' }] },
    { group: 'Animated', items: shaderPresets.map(p => ({ value: `shader:${p.name}`, path: `shader:${p.name}`, name: p.title.replace(/ \(animated\)/i, ''), kind: 'Animated' })) },
    { group: 'Videos', items: backgrounds.filter(m => m.kind === 'video').map(m => ({ value: m.id, path: m.path ?? '', name: m.title, kind: 'Video' })) },
    { group: 'Images', items: backgrounds.filter(m => m.kind === 'image').map(m => ({ value: m.id, path: m.path ?? '', name: m.title, kind: 'Image' })) },
  ].filter(g => g.items.length);
}
export function backgroundName(value: string, backgrounds: MediaFile[]) {
  if (!value) return { name: 'No background', kind: 'Keeps the previous one' };
  if (isShader(value)) return { name: shaderPresets.find(p => `shader:${p.name}` === value)?.title.replace(/ \(animated\)/i, '') ?? value.slice(7), kind: 'Animated' };
  const media = backgrounds.find(m => m.id === value);
  return { name: media?.title ?? 'Missing file', kind: media ? (media.kind === 'video' ? 'Video' : 'Image') : 'File' };
}

export function itemSummary(item: SessionItem) {
  const kind = item.kind;
  switch (kind.kind) {
    case 'text': return `${item.song ? 'Song' : 'Text'} · ${kind.data.steps.length} slides`;
    case 'presentation': return `Slides · ${(kind.data.slides || [kind.data.path]).length}`;
    case 'capture': return `${kind.data.source.kind === 'window' ? 'Window' : 'Screen'} capture · ${kind.data.source.name}`;
    case 'video': return `Video${kind.data.options?.looping ? ' · loops' : ''}${kind.data.options?.confidence ? ' · confidence' : ''}`;
    case 'audio': return `Audio${kind.data.options?.looping ? ' · loops' : ''}`;
    case 'image': return 'Image';
    case 'section': return 'Section';
  }
}
