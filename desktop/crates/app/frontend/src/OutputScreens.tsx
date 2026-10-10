import { useCallback, useEffect, useState } from 'react';
import { invoke, isTauri } from './tauri';

type Pair = [number, number];
type OutputDisplay = { monitor: number | null; enabled: boolean; fullscreen: boolean; size: Pair | null; aspect: Pair | null };
type OutputDisplays = { monitors: string[]; shown: boolean; outputs: Record<Output, OutputDisplay> };
type Output = 'projection' | 'confidence';

const outputs: [Output, string][] = [['projection', 'Projection'], ['confidence', 'Confidence']];
const sizes: Pair[] = [[3840, 2160], [2560, 1440], [1920, 1080], [1600, 900], [1280, 720], [1024, 768], [800, 600]];
const aspects: Pair[] = [[16, 9], [16, 10], [4, 3], [21, 9]];
const pair = (value: string): Pair | null => { const [w, h] = value.split(/[x:]/).map(Number); return w > 0 && h > 0 ? [w, h] : null; };

/** Display, size and aspect of each output window, and the switch that shows them. */
export default function OutputScreens({ onError }: { onError: (message: string) => void }) {
  const [state, setState] = useState<OutputDisplays | null>(null);
  const load = useCallback(() => { if (isTauri()) void invoke<OutputDisplays>('get_output_displays').then(setState).catch(e => onError(String(e))); }, [onError]);
  // Displays can be connected while the app runs.
  useEffect(() => { load(); window.addEventListener('focus', load); return () => window.removeEventListener('focus', load); }, [load]);
  if (!state) return null;
  const change = (output: Output, patch: Partial<OutputDisplay>) => {
    const next = { ...state.outputs[output], ...patch };
    if (next.monitor == null) return;
    void invoke<OutputDisplays>('set_output_display', { output, label: state.monitors[next.monitor], ...next }).then(setState).catch(e => { onError(String(e)); load(); });
  };
  return <section className="output-screens" aria-label="Output screens">
    <div className="output-screens-heading"><span>Screens</span><button className="output-screens-toggle" aria-pressed={state.shown} onClick={() => void invoke<OutputDisplays>('set_outputs_shown', { shown: !state.shown }).then(setState).catch(e => onError(String(e)))}>{state.shown ? 'Screens on' : 'Screens off'}</button></div>
    {outputs.map(([output, name]) => {
      const current = state.outputs[output];
      return <div className="output-screen" key={output}>
        <button className="output-screen-enable" role="switch" aria-checked={current.enabled} aria-label={`Enable ${name} screen`} disabled={current.monitor == null} onClick={() => change(output, { enabled: !current.enabled })}><span className="confidence-toggle-check" aria-hidden="true">{current.enabled ? '✓' : ''}</span>{name}</button>
        <select aria-label={`${name} display`} value={current.monitor ?? -1} onChange={e => change(output, { monitor: Number(e.target.value) })}>
          {current.monitor == null && <option value={-1} disabled>Saved display disconnected</option>}
          {state.monitors.map((label, index) => <option value={index} key={index}>{label}</option>)}
        </select>
        <select aria-label={`${name} size`} disabled={current.monitor == null} value={current.fullscreen ? 'full' : (current.size ?? [1280, 720]).join('x')} onChange={e => change(output, e.target.value === 'full' ? { fullscreen: true } : { fullscreen: false, size: pair(e.target.value) })}>
          <option value="full">Fullscreen</option>
          {[...sizes, ...(current.size && !sizes.some(size => size.join('x') === current.size?.join('x')) ? [current.size] : [])].map(size => <option key={size.join('x')} value={size.join('x')}>{size.join(' × ')}</option>)}
        </select>
        <select aria-label={`${name} aspect ratio`} disabled={current.monitor == null} value={current.aspect?.join(':') ?? 'fill'} onChange={e => change(output, { aspect: pair(e.target.value) })}>
          <option value="fill">Fill screen</option>
          {aspects.map(aspect => <option key={aspect.join(':')} value={aspect.join(':')}>{aspect.join(':')}</option>)}
        </select>
      </div>;
    })}
  </section>;
}
