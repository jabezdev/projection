import { useEffect, useState } from 'react';
import { invoke, isTauri, listen } from './tauri';

export type OutputConfig = {
  /** Settings schema version, owned by Rust. */
  version?: number;
  font: string; size: number; weight: number; italic: boolean; caps: boolean;
  color: string; align: 'left' | 'center' | 'right'; vertical: 'top' | 'center' | 'bottom';
  marginX: number; marginY: number; lineHeight: number; letterSpacing: number;
  border: number; borderColor: string; boxColor: string; boxOpacity: number;
  backgroundFit: 'cover' | 'contain'; transition: number; confidenceSize: number; logoPath: string;
};
export const defaultOutputConfig: OutputConfig = { font: 'Arial', size: 6, weight: 800, italic: false, caps: false, color: '#ffffff', align: 'center', vertical: 'bottom', marginX: 5, marginY: 10, lineHeight: 1.2, letterSpacing: 0, border: 0, borderColor: '#ffffff', boxColor: '#000000', boxOpacity: 0, backgroundFit: 'cover', transition: 250, confidenceSize: 4, logoPath: '' };
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
