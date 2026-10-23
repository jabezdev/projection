import type { CSSProperties } from 'react';
import { Icon } from './Icons';
import { NativePreview } from './NativePreview';
import OutputScreens from './OutputScreens';
import { useOutputSizes } from './useOutputSizes';
import type { DisplayMode } from './useEngine';

const DISPLAY_MODES: DisplayMode[] = ['logo', 'black', 'clear', 'live'];

/** The right column: output windows, display modes and the native preview stills. */
export default function OutputControls({ display, loading, onDisplay, onError }: { display: DisplayMode; loading: Record<string, boolean>; onDisplay: (display: DisplayMode) => void; onError: (message: string) => void }) {
  const outputSizes = useOutputSizes(true);
  return <aside className="operator-controls" aria-label="Output controls and previews">
    <OutputScreens onError={onError}/>
    <div className="display-controls">{DISPLAY_MODES.map(value => <button key={value} aria-pressed={display === value} onClick={() => onDisplay(value)}><Icon name={value}/><span>{value.charAt(0).toUpperCase() + value.slice(1)}</span></button>)}</div>
    <div className="preview-stack">
      <section aria-label="Projection preview"><div className="preview-label">Projection{loading.projection && <span className="on-air-pending" role="status">Loading… previous picture still on air</span>}</div><div className="preview-viewport"><div className="output-preview-frame" style={{ '--output-aspect': outputSizes.projection } as CSSProperties}><NativePreview target="projection" label="Projection output"/></div></div></section>
      <section aria-label="Confidence preview"><div className="preview-label">Confidence{loading.confidence && <span className="on-air-pending" role="status">Loading…</span>}</div><div className="preview-viewport"><div className="output-preview-frame confidence-preview-surface" style={{ '--output-aspect': outputSizes.confidence } as CSSProperties}><NativePreview target="confidence" label="Confidence output"/></div></div></section>
    </div>
  </aside>;
}
