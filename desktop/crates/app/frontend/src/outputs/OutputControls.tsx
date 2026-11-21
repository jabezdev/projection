import type { CSSProperties } from 'react';
import { Icon } from '../ui/Icons';
import { LiveMonitor } from './NativePreview';
import { ScreenSwitch } from './OutputScreens';
import { useScreens } from './useScreens';
import { useOutputSizes } from './useOutputSizes';
import type { DisplayMode } from '../api/useEngine';

type Override = Exclude<DisplayMode, 'live'>;
const OVERRIDES: [Override, string, string][] = [
  ['logo', 'Logo', 'Show the logo instead of the cue'],
  ['black', 'Black', 'Black out the projection'],
  ['clear', 'Clear', 'Hide the text, keep the background'],
];

export default function OutputControls({ display, loading, onDisplay, onError }: { display: DisplayMode; loading: Record<string, boolean>; onDisplay: (display: DisplayMode) => void; onError: (message: string) => void }) {
  const outputSizes = useOutputSizes(true);
  const screens = useScreens(onError);
  return <aside className="operator-controls" aria-label="Output controls and previews">
    <section className="output-panel" aria-label="Projection">
      <header className="output-bar">
        <h2>Projection</h2>
        {loading.projection && <span className="on-air-pending" role="status">Loading… previous picture still on air</span>}
        <ScreenSwitch output="projection" name="Projection" screens={screens}/>
      </header>
      <div className="display-overrides" role="group" aria-label="Projection overrides">
        {OVERRIDES.map(([value, label, hint]) => <button key={value} className={`display-override is-${value}`} aria-pressed={display === value}
          title={display === value ? `${label} is on. Click to go back to live.` : hint}
          onClick={() => onDisplay(display === value ? 'live' : value)}>
          <Icon name={value}/><span>{label}</span>
        </button>)}
      </div>
      <div className="preview-viewport"><div className="output-preview-frame" style={{ '--output-aspect': outputSizes.projection } as CSSProperties}><LiveMonitor target="projection" label="Projection output"/></div></div>
    </section>
    <section className="output-panel" aria-label="Confidence">
      <header className="output-bar">
        <h2>Confidence</h2>
        {loading.confidence && <span className="on-air-pending" role="status">Loading…</span>}
        <ScreenSwitch output="confidence" name="Confidence" screens={screens}/>
      </header>
      <div className="preview-viewport"><div className="output-preview-frame confidence-preview-surface" style={{ '--output-aspect': outputSizes.confidence } as CSSProperties}><LiveMonitor target="confidence" label="Confidence output"/></div></div>
    </section>
  </aside>;
}
