import { useEffect, useState } from 'react';
import { invoke, isTauri, listen } from './tauri';

export type TransitionKind = 'fade' | 'cut' | 'slide';
export type Easing = 'linear' | 'ease' | 'ease_in' | 'ease_out' | 'ease_in_out';
export type TransitionDirection = 'left' | 'right' | 'up' | 'down';
/** Output styling; Rust's `protocol::OutputConfig` is the source of truth. Flat fields style the projection, `confidence*` the confidence screen. */
export type OutputConfig = {
  /** Settings schema version, owned by Rust. */
  version?: number;
  font: string; size: number; weight: number; italic: boolean; caps: boolean;
  color: string; align: 'left' | 'center' | 'right'; vertical: 'top' | 'center' | 'bottom';
  marginX: number; marginY: number; lineHeight: number; letterSpacing: number;
  border: number; borderColor: string; boxColor: string; boxOpacity: number;
  backgroundFit: 'cover' | 'contain'; transition: number;
  transitionKind: TransitionKind; transitionEasing: Easing; transitionDirection: TransitionDirection;
  textShadow: boolean; shadowSize: number; shadowOffsetX: number; shadowOffsetY: number; shadowColor: string; shadowOpacity: number;
  textOutline: boolean; outlineWidth: number; outlineColor: string;
  textGlow: boolean; glowSize: number; glowColor: string; glowOpacity: number;
  shaderColorA: string; shaderColorB: string; shaderSpeed: number; shaderAnimate: boolean;
  confidenceSize: number; confidenceFont: string; confidenceLineHeight: number; confidenceCaps: boolean;
  confidenceClock: boolean; confidenceTape: boolean; confidenceTransition: number;
  logoPath: string;
};
export const defaultOutputConfig: OutputConfig = {
  font: 'Arial', size: 6, weight: 800, italic: false, caps: false, color: '#ffffff', align: 'center', vertical: 'bottom',
  marginX: 5, marginY: 10, lineHeight: 1.2, letterSpacing: 0, border: 0, borderColor: '#ffffff', boxColor: '#000000', boxOpacity: 0,
  backgroundFit: 'cover', transition: 250, transitionKind: 'fade', transitionEasing: 'linear', transitionDirection: 'left',
  textShadow: false, shadowSize: 8, shadowOffsetX: 0, shadowOffsetY: 4, shadowColor: '#000000', shadowOpacity: 80,
  textOutline: false, outlineWidth: 3, outlineColor: '#000000',
  textGlow: false, glowSize: 12, glowColor: '#ffffff', glowOpacity: 60,
  shaderColorA: '#14213d', shaderColorB: '#47c6bd', shaderSpeed: 1, shaderAnimate: true,
  confidenceSize: 4, confidenceFont: 'Segoe UI', confidenceLineHeight: 1.35, confidenceCaps: false,
  confidenceClock: true, confidenceTape: true, confidenceTransition: 250,
  logoPath: '',
};
/** Built-in animated backgrounds (`shader:<name>` paths); matches `protocol::SHADER_PRESETS`. */
export const shaderPresets = [{ name: 'gradient', title: 'Gradient (animated)' }, { name: 'aurora', title: 'Aurora (animated)' }, { name: 'bokeh', title: 'Bokeh lights (animated)' }] as const;
const prefix = 'magi.output.';
export function useOutputState<T extends object>(domain: 'config', defaults: T): [T, (value: T) => Promise<void>] {
  const [value, setValue] = useState<T>(() => {
    try { return { ...defaults, ...JSON.parse(localStorage.getItem(prefix + domain) || '{}') }; } catch { return defaults; }
  });
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    if (isTauri()) {
      void (async () => {
        const unlisten = await listen<{ domain: string; value: T }>('output:changed', ({ payload }) => { if (!disposed && payload.domain === domain) setValue({ ...defaults, ...payload.value }); });
        if (disposed) { unlisten(); return; }
        cleanup = unlisten;
        const saved = await invoke<T>('get_output_state', { domain });
        if (!disposed) setValue({ ...defaults, ...saved });
      })().catch(console.error);
    }
    const receive = (event: StorageEvent) => { if (event.key === prefix + domain && event.newValue) { try { setValue({ ...defaults, ...JSON.parse(event.newValue) }); } catch { /* Ignore malformed storage. */ } } };
    window.addEventListener('storage', receive);
    return () => { disposed = true; cleanup?.(); window.removeEventListener('storage', receive); };
  }, [domain, defaults]);
  async function update(next: T) {
    setValue(next);
    if (isTauri()) await invoke('set_output_state', { domain, value: next });
    else localStorage.setItem(prefix + domain, JSON.stringify(next));
  }
  return [value, update];
}
