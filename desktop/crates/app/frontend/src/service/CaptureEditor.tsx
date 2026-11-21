import { useEffect, useState } from 'react';
import { invoke, isTauri } from '../api/tauri';
import { Icon } from '../ui/Icons';
import { DEMO_SOURCES } from '../demo/service';
import { defaultCaptureOptions, newItemId, type CaptureFit, type CaptureKind, type CaptureOptions, type CaptureSource, type SessionItem } from '../api/models';

type Method = { name: string; label: string; system_picker: boolean };
type Methods = { current: string | null; choice: string; methods: Method[] };


/** Which screen or window to capture, and which screens show it. Loads the sources itself. */
export function CaptureSettings({ source, options, onSource, onOptions }: { source: CaptureSource | null; options: CaptureOptions; onSource: (source: CaptureSource) => void; onOptions: (options: CaptureOptions) => void }) {
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
  const pick = async (kind: CaptureKind) => {
    setBusy(true); setError('');
    try { const picked = await invoke<CaptureSource | null>('pick_capture_source', { kind }); if (picked) onSource(picked); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const fit = (key: 'projection_fit' | 'confidence_fit', label: string, disabled: boolean) => <label>{label}<select aria-label={label} disabled={disabled} value={options[key]} onChange={e => onOptions({ ...options, [key]: e.target.value as CaptureFit })}><option value="fit">Fit the whole picture</option><option value="fill">Fill the screen (crop edges)</option></select></label>;
  const group = (kind: CaptureKind, heading: string) => {
    const list = sources.filter(s => s.kind === kind);
    return list.length > 0 && <div className="capture-sources"><small>{heading}</small>{list.map(s => <label key={s.id} className="setting-check"><input type="radio" name="capture-source" checked={source?.id === s.id} onChange={() => onSource(s)}/>{s.name}</label>)}</div>;
  };
  return <>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <fieldset><legend>What to show</legend>
      {source && <p className="capture-chosen"><Icon name="capture"/>{source.name}</p>}
      {method?.system_picker
        ? <><div className="capture-pick"><button type="button" disabled={busy} onClick={() => void pick('screen')}>Choose a screen…</button><button type="button" disabled={busy} onClick={() => void pick('window')}>Choose a window…</button></div><small>The system asks which screen or window to share, once; PALTO remembers it.</small></>
        : <>{group('screen', 'Screens')}{group('window', 'Windows')}{!sources.length && <small>No screens or windows found.</small>}<button type="button" onClick={refresh}>Refresh list</button></>}
      {method && <small>Capture method: {method.label} (Settings → System).</small>}
    </fieldset>
    <fieldset><legend>Show on</legend>
      <label className="setting-check"><input type="checkbox" checked={options.projection} onChange={e => onOptions({ ...options, projection: e.target.checked })}/>Projection</label>
      {fit('projection_fit', 'Projection fit', !options.projection)}
      <label className="setting-check"><input type="checkbox" checked={options.confidence} onChange={e => onOptions({ ...options, confidence: e.target.checked })}/>Confidence screen</label>
      {fit('confidence_fit', 'Confidence fit', !options.confidence)}
      <small>The capture runs only while this item is cued. If the window closes or the screen disconnects, the outputs go black and the cue shows why.</small>
    </fieldset>
  </>;
}

/** A screen or window shown live, on the projection, the confidence screen or both. */
export default function CaptureEditor({ item, onClose, onSave }: { item?: SessionItem; onClose: () => void; onSave: (item: SessionItem) => Promise<void> }) {
  const existing = item?.kind.kind === 'capture' ? item.kind.data : null;
  const [source, setSource] = useState<CaptureSource | null>(existing?.source ?? null);
  const [options, setOptions] = useState<CaptureOptions>({ ...defaultCaptureOptions, ...existing?.options });
  const [title, setTitle] = useState(item?.title ?? '');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const choose = (picked: CaptureSource) => { setSource(picked); if (!title.trim() || title === source?.name) setTitle(picked.name); };
  return <div className="modal-backdrop"><form className="capture-editor" role="dialog" aria-modal="true" aria-label="Screen capture" onKeyDown={e => { if (e.key === 'Escape' && !busy) onClose(); }} onSubmit={e => {
    e.preventDefault();
    if (!source) return;
    setBusy(true);
    void onSave({ id: item?.id ?? newItemId(), title: title.trim() || source.name, kind: { kind: 'capture', data: { source, options } } }).catch(e => { setError(String(e)); setBusy(false); });
  }}>
    <header className="surface-header"><div><small>SCREEN CAPTURE</small><input aria-label="Capture title" placeholder="Title" value={title} onChange={e => setTitle(e.target.value)} autoFocus/></div><button type="button" className="icon-button" aria-label="Close screen capture" disabled={busy} onClick={onClose}><Icon name="close"/></button></header>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <div className="capture-editor-body settings-fields"><CaptureSettings source={source} options={options} onSource={choose} onOptions={setOptions}/></div>
    <footer><span/><button type="button" disabled={busy} onClick={onClose}>Cancel</button><button className="primary-action" disabled={busy || !source || (!options.projection && !options.confidence)}>Save capture</button></footer>
  </form></div>;
}
