import { useCallback, useEffect, useState } from 'react';
import { invoke } from '../api/tauri';

export type Pair = [number, number];
export type Output = 'projection' | 'confidence';
export type OutputDisplay = { monitor: number | null; shown: boolean; fullscreen: boolean; size: Pair | null; aspect: Pair | null };
export type Monitor = { key: string; name: string; connector: string | null; width: number; height: number; aspect: string; operator: boolean };
type OutputDisplays = { monitors: Monitor[]; shown: boolean; outputs: Record<Output, OutputDisplay> };

export type Screens = ReturnType<typeof useScreens>;

/** The output windows' displays and settings, shared by both screen switches. */
export function useScreens(onError: (message: string) => void) {
  const [state, setState] = useState<OutputDisplays | null>(null);
  const load = useCallback(() => { void invoke<OutputDisplays>('get_output_displays').then(setState).catch(e => onError(String(e))); }, [onError]);
  // Displays can be connected while the app runs.
  useEffect(() => { load(); window.addEventListener('focus', load); return () => window.removeEventListener('focus', load); }, [load]);
  const change = (output: Output, patch: Partial<OutputDisplay>) => {
    if (!state) return;
    const next = { ...state.outputs[output], ...patch };
    if (next.monitor == null) return;
    void invoke<OutputDisplays>('set_output_display', { output, key: state.monitors[next.monitor]?.key, monitor: next.monitor, fullscreen: next.fullscreen, size: next.size, aspect: next.aspect })
      .then(setState).catch(e => { onError(String(e)); load(); });
  };
  const show = (output: Output, shown: boolean) => {
    void invoke<OutputDisplays>('set_output_shown', { output, shown }).then(setState).catch(e => { onError(String(e)); load(); });
  };
  return { state, change, show, reload: load };
}
