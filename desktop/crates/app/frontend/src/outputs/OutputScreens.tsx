import { useEffect, useId, useRef, useState } from 'react';
import { Icon } from '../ui/Icons';
import type { Monitor, Output, OutputDisplay, Pair, Screens } from './useScreens';

const sizes: Pair[] = [[3840, 2160], [2560, 1440], [1920, 1080], [1600, 900], [1280, 720], [1024, 768], [800, 600]];
const aspects: Pair[] = [[16, 9], [16, 10], [4, 3], [21, 9]];
const aspectLabel = ([w, h]: Pair) => `${w}:${h}`;
/** "BenQ GW2480 · HDMI", marked when the operator window is on it. */
const monitorLabel = (monitor: Monitor) => [monitor.name, monitor.connector, monitor.operator ? 'operator screen' : null].filter(Boolean).join(' · ');
const pair = (value: string): Pair | null => { const [w, h] = value.split(/[x:]/).map(Number); return w > 0 && h > 0 ? [w, h] : null; };

/**
 * A pill: the left half shows or hides the screen, the chevron opens its
 * display, size and aspect settings.
 */
export function ScreenSwitch({ output, name, screens }: { output: Output; name: string; screens: Screens }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const popoverId = useId();
  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setOpen(false);
      root.current?.querySelector<HTMLButtonElement>('.screen-switch-more')?.focus();
    };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', away); document.removeEventListener('keydown', escape); };
  }, [open]);
  const { state } = screens;
  const current = state?.outputs[output];
  const monitor = current?.monitor == null ? null : state?.monitors[current.monitor] ?? null;
  const shown = Boolean(current?.shown);
  const disconnected = Boolean(state && !monitor);
  const lower = name.toLowerCase();
  return <div className={`screen-switch${shown ? ' is-shown' : ''}`} ref={root}>
    <button className="screen-switch-show" role="switch" aria-checked={shown} aria-label={`Show the ${lower} screen`} disabled={!state || disconnected}
      title={disconnected ? `The ${lower} screen's display is disconnected` : monitor ? `${shown ? 'Showing' : 'Show'} on ${monitor.name}` : undefined}
      onClick={() => screens.show(output, !shown)}>
      <Icon name={shown ? 'screen' : 'screen-off'}/><span>{disconnected ? 'No display' : shown ? 'On' : 'Off'}</span>
    </button>
    <button className="screen-switch-more" aria-label={`${name} screen settings`} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? popoverId : undefined} disabled={!state}
      onClick={() => { if (!open) screens.reload(); setOpen(!open); }}>
      <Icon name="chevron"/>
    </button>
    {/* Live monitors hide while this is open: their native windows would cover it. */}
    {open && state && current && <div className="screen-popover" id={popoverId} role="dialog" aria-label={`${name} screen settings`} data-covers-monitors="">
      <ScreenSettings output={output} name={name} current={current} monitor={monitor} monitors={state.monitors} screens={screens}/>
    </div>}
  </div>;
}

function ScreenSettings({ output, name, current, monitor, monitors, screens }: { output: Output; name: string; current: OutputDisplay; monitor: Monitor | null; monitors: Monitor[]; screens: Screens }) {
  // Windows larger than the display would hang off its edges.
  const fitting = sizes.filter(([w, h]) => !monitor || (w <= monitor.width && h <= monitor.height && !(w === monitor.width && h === monitor.height)));
  const sizeOptions = current.size && !fitting.some(size => size.join('x') === current.size?.join('x')) ? [...fitting, current.size] : fitting;
  // The display's own shape is what "Fill screen" already gives.
  const aspectOptions = aspects.filter(aspect => aspectLabel(aspect) !== monitor?.aspect || aspectLabel(aspect) === (current.aspect && aspectLabel(current.aspect)));
  return <>
    <label>Display
      <select aria-label={`${name} display`} value={current.monitor ?? -1} onChange={e => screens.change(output, { monitor: Number(e.target.value) })}>
        {current.monitor == null && <option value={-1} disabled>Saved display disconnected</option>}
        {monitors.map((entry, index) => <option value={index} key={entry.key}>{monitorLabel(entry)}</option>)}
      </select>
    </label>
    <label>Size
      <select aria-label={`${name} size`} disabled={current.monitor == null} value={current.fullscreen ? 'full' : (current.size ?? [1280, 720]).join('x')} onChange={e => screens.change(output, e.target.value === 'full' ? { fullscreen: true } : { fullscreen: false, size: pair(e.target.value) })}>
        <option value="full">{monitor ? `Fullscreen · ${monitor.width} × ${monitor.height}` : 'Fullscreen'}</option>
        {sizeOptions.map(size => <option key={size.join('x')} value={size.join('x')}>{`Window · ${size.join(' × ')}`}</option>)}
      </select>
    </label>
    <label>Aspect ratio
      <select aria-label={`${name} aspect ratio`} disabled={current.monitor == null} value={current.aspect ? aspectLabel(current.aspect) : 'fill'} onChange={e => screens.change(output, { aspect: pair(e.target.value) })}>
        <option value="fill">{monitor?.aspect ? `Fill screen · ${monitor.aspect}` : 'Fill screen'}</option>
        {aspectOptions.map(aspect => <option key={aspectLabel(aspect)} value={aspectLabel(aspect)}>{`Letterbox to ${aspectLabel(aspect)}`}</option>)}
      </select>
    </label>
  </>;
}
