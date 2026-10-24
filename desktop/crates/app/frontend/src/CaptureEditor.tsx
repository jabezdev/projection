import { useEffect, useState } from 'react';
import { invoke, isTauri } from './tauri';
import { Icon } from './Icons';
import { defaultCaptureOptions, newItemId, type CaptureFit, type CaptureKind, type CaptureOptions, type CaptureSource, type SessionItem } from './models';

type Method = { name: string; label: string; system_picker: boolean };
type Methods = { current: string | null; choice: string; methods: Method[] };

/** Browser-only UI: sample sources so the editor can be laid out. */
const DEMO_SOURCES: CaptureSource[] = [
  { kind: 'screen', id: 'demo:screen-1', name: 'Display 1 (1920×1080)' },
  { kind: 'window', id: 'demo:window-1', name: 'Sermon notes — PowerPoint' },
];

/** A screen or window shown live, on the projection, the confidence screen or both. */
export default function CaptureEditor({ item, onClose, onSave }: { item?: SessionItem; onClose: () => void; onSave: (item: SessionItem) => Promise<void> }) {
  const existing = item?.kind.kind === 'capture' ? item.kind.data : null;
  const [source, setSource] = useState<CaptureSource | null>(existing?.source ?? null);
  const [options, setOptions] = useState<CaptureOptions>({ ...defaultCaptureOptions, ...existing?.options });
  const [title, setTitle] = useState(item?.title ?? '');
  const [method, setMethod] = useState<Method | null>(isTauri() ? null : { name: 'demo', label: 'Demo', system_picker: false });
  const [sources, setSources] = useState<CaptureSource[]>(isTauri() ? [] : DEMO_SOURCES);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const refresh = () => {
    if (!isTauri()) { setSources(DEMO_SOURCES); return; }
    void invoke<CaptureSource[]>('list_capture_sources').then(setSources).catch(e => setError(String(e)));
  };
  useEffect(() => {
    if (!isTauri()) return;
    void invoke<Methods>('get_capture_methods').then(methods => {
      const current = methods.methods.find(m => m.name === methods.current) ?? null;
      setMethod(current);
      if (current && !current.system_picker) void invoke<CaptureSource[]>('list_capture_sources').then(setSources).catch(e => setError(String(e)));
      if (!current) setError('No screen capture method is available on this computer.');
    }).catch(e => setError(String(e)));
  }, []);
  const choose = (picked: CaptureSource) => { setSource(picked); if (!title.trim() || title === source?.name) setTitle(picked.name); };
  const pick = async (kind: CaptureKind) => {
    setBusy(true); setError('');
    try { const picked = await invoke<CaptureSource | null>('pick_capture_source', { kind }); if (picked) choose(picked); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const fit = (key: 'projection_fit' | 'confidence_fit', label: string, disabled: boolean) => <label>{label}<select aria-label={label} disabled={disabled} value={options[key]} onChange={e => setOptions({ ...options, [key]: e.target.value as CaptureFit })}><option value="fit">Fit the whole picture</option><option value="fill">Fill the screen (crop edges)</option></select></label>;
  const group = (kind: CaptureKind, heading: string) => {
    const list = sources.filter(s => s.kind === kind);
    return list.length > 0 && <div className="capture-sources"><small>{heading}</small>{list.map(s => <label key={s.id} className="setting-check"><input type="radio" name="capture-source" checked={source?.id === s.id} onChange={() => choose(s)}/>{s.name}</label>)}</div>;
  };
  return <div className="modal-backdrop"><form className="capture-editor" role="dialog" aria-modal="true" aria-label="Screen capture" onKeyDown={e => { if (e.key === 'Escape' && !busy) onClose(); }} onSubmit={e => {
    e.preventDefault();
    if (!source) return;
    setBusy(true);
    void onSave({ id: item?.id ?? newItemId(), title: title.trim() || source.name, kind: { kind: 'capture', data: { source, options } } }).catch(e => { setError(String(e)); setBusy(false); });
  }}>
    <header className="surface-header"><div><small>SCREEN CAPTURE</small><input aria-label="Capture title" placeholder="Title" value={title} onChange={e => setTitle(e.target.value)} autoFocus/></div><button type="button" className="icon-button" aria-label="Close screen capture" disabled={busy} onClick={onClose}><Icon name="close"/></button></header>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <div className="capture-editor-body settings-fields">
      <fieldset><legend>What to show</legend>
        {source && <p className="capture-chosen"><Icon name="capture"/>{source.name}</p>}
        {method?.system_picker
          ? <><div className="capture-pick"><button type="button" disabled={busy} onClick={() => void pick('screen')}>Choose a screen…</button><button type="button" disabled={busy} onClick={() => void pick('window')}>Choose a window…</button></div><small>The system asks which screen or window to share, once; PALTO remembers it.</small></>
          : <>{group('screen', 'Screens')}{group('window', 'Windows')}{!sources.length && <small>No screens or windows found.</small>}<button type="button" onClick={refresh}>Refresh list</button></>}
        {method && <small>Capture method: {method.label} (Settings → System).</small>}
      </fieldset>
      <fieldset><legend>Show on</legend>
        <label className="setting-check"><input type="checkbox" checked={options.projection} onChange={e => setOptions({ ...options, projection: e.target.checked })}/>Projection</label>
        {fit('projection_fit', 'Projection fit', !options.projection)}
        <label className="setting-check"><input type="checkbox" checked={options.confidence} onChange={e => setOptions({ ...options, confidence: e.target.checked })}/>Confidence screen</label>
        {fit('confidence_fit', 'Confidence fit', !options.confidence)}
        <small>The capture runs only while this item is cued. If the window closes or the screen disconnects, the outputs go black and the cue shows why.</small>
      </fieldset>
    </div>
    <footer><span/><button type="button" disabled={busy} onClick={onClose}>Cancel</button><button className="primary-action" disabled={busy || !source || (!options.projection && !options.confidence)}>Save capture</button></footer>
  </form></div>;
}

type CaptureStatus = { id: string; name: string; error: string | null };

/** The cue for a capture item, with its problem while it is on air. */
export function CaptureCue({ item, active, onSelect }: { item: SessionItem; active: boolean; onSelect: () => void }) {
  const [problem, setProblem] = useState<string | null>(null);
  const source = item.kind.kind === 'capture' ? item.kind.data.source : null;
  useEffect(() => {
    if (!active || !isTauri() || !source) return;
    const check = () => void invoke<CaptureStatus[]>('get_capture_status').then(list => setProblem(list.find(s => s.id === source.id)?.error ?? null)).catch(() => {});
    check();
    const timer = window.setInterval(check, 1000);
    return () => { window.clearInterval(timer); setProblem(null); };
  }, [active, source]);
  const options = item.kind.kind === 'capture' ? { ...defaultCaptureOptions, ...item.kind.data.options } : defaultCaptureOptions;
  const screens = [options.projection && 'Projection', options.confidence && 'Confidence'].filter(Boolean).join(' + ');
  return <button className={`schedule-audio-cue capture-cue ${active ? 'is-active' : ''}`} aria-pressed={active} onClick={onSelect}><Icon name="capture"/><span>{source?.name ?? 'Screen capture'}<small>{problem ? `Capture problem: ${problem}` : screens}</small></span><Icon name="live"/></button>;
}
