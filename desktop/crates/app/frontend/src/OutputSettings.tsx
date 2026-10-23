import { useEffect, useState } from 'react';
import { invoke, isTauri } from './tauri';
import { Icon } from './Icons';
import { defaultOutputConfig, type OutputConfig } from './outputState';
import { NativePreview } from './NativePreview';
export default function OutputSettings({ config, onChange, onClose }: { config: OutputConfig; onChange: (config: OutputConfig) => Promise<void>; onClose: () => void }) {
  const [error, setError] = useState('');
  // The native engine renders the sample in exactly the output's style.
  useEffect(() => { if (isTauri()) void invoke('preview_output_style', { config }).catch(e => setError(String(e))); }, [config]);
  const update = (patch: Partial<OutputConfig>) => { void onChange({ ...config, ...patch }).catch(e => setError(String(e))); };
  const range = (key: keyof OutputConfig, label: string, min: number, max: number, step = 1) => <label className="setting-range">{label}<output>{String(config[key])}</output><input aria-label={label} type="range" min={min} max={max} step={step} value={Number(config[key])} onChange={e => update({ [key]: Number(e.target.value) })}/></label>;
  const color = (key: keyof OutputConfig, label: string) => <label className="setting-color">{label}<input aria-label={label} type="color" value={String(config[key])} onChange={e => update({ [key]: e.target.value })}/></label>;
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') onClose();
    if (e.key === 'Tab') {
      const targets = e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input, select');
      const first = targets[0], last = targets[targets.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
      if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
    } }}><section className="output-settings" role="dialog" aria-modal="true" aria-label="Output settings">
    <header className="surface-header"><div><small>OUTPUT SETTINGS</small><h2>Projection & confidence</h2></div><button autoFocus className="icon-button" aria-label="Close settings" onClick={onClose}><Icon name="close"/></button></header>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <div className="settings-body"><div className="settings-fields">
      <fieldset><legend>Typography</legend><label>Font family<input aria-label="Font family" list="projection-fonts" value={config.font} onChange={e => update({ font: e.target.value })}/><datalist id="projection-fonts">{['Arial', 'Verdana', 'Tahoma', 'Georgia', 'Times New Roman', 'Trebuchet MS', 'Impact', 'sans-serif', 'serif', 'monospace'].map(font => <option key={font}>{font}</option>)}</datalist></label><small>Use any font installed on this computer.</small>
        {range('size', 'Font size (% of screen width)', 1, 16, .1)}{range('weight', 'Font weight', 100, 900, 100)}
        <div className="setting-toggles"><button aria-pressed={config.weight >= 700} onClick={() => update({ weight: config.weight >= 700 ? 400 : 700 })}><b>Bold</b></button><button aria-pressed={config.italic} onClick={() => update({ italic: !config.italic })}><i>Italic</i></button><button aria-pressed={config.caps} onClick={() => update({ caps: !config.caps })}>ALL CAPS</button></div>
        {color('color', 'Text color')}{range('lineHeight', 'Line height', .8, 2, .05)}{range('letterSpacing', 'Letter spacing', -2, 15, .5)}
      </fieldset>
      <fieldset><legend>Placement</legend><label>Vertical position<select aria-label="Vertical position" value={config.vertical} onChange={e => update({ vertical: e.target.value as OutputConfig['vertical'] })}>{['top', 'center', 'bottom'].map(value => <option key={value}>{value}</option>)}</select></label><label>Text alignment<select aria-label="Text alignment" value={config.align} onChange={e => update({ align: e.target.value as OutputConfig['align'] })}>{['left', 'center', 'right'].map(value => <option key={value}>{value}</option>)}</select></label>{range('marginX', 'Horizontal safe margin (%)', 0, 30)}{range('marginY', 'Vertical safe margin (%)', 0, 30)}</fieldset>
      <fieldset><legend>Square text box</legend>{range('border', 'Border width', 0, 10)}{color('borderColor', 'Border color')}{color('boxColor', 'Box fill')}{range('boxOpacity', 'Box opacity (%)', 0, 100)}</fieldset>
      <fieldset><legend>Logo</legend><label>Logo image or video<input aria-label="Logo media path" value={config.logoPath} placeholder="Choose an image or video for the Logo button" onChange={e => update({ logoPath: e.target.value })}/></label>{isTauri() && <button onClick={() => void invoke<string | null>('choose_media_file', { kind: 'logo' }).then(path => { if (path) update({ logoPath: path }); }).catch(e => setError(String(e)))}>Choose logo media…</button>}{config.logoPath && <button onClick={() => update({ logoPath: '' })}>Clear logo media</button>}<small>Logo videos play muted and loop. The operator preview uses a thumbnail.</small></fieldset>
      <fieldset><legend>Backgrounds & confidence</legend><label>Background fit<select aria-label="Background fit" value={config.backgroundFit} onChange={e => update({ backgroundFit: e.target.value as OutputConfig['backgroundFit'] })}><option value="cover">Fill screen</option><option value="contain">Fit entire image / video</option></select></label>{range('transition', 'Text & background fade (ms)', 0, 1500, 50)}{range('confidenceSize', 'Confidence text size', 2, 8, .1)}</fieldset>
      {isTauri() && <fieldset><legend>Troubleshooting</legend><button onClick={() => void invoke('open_logs_folder').catch(e => setError(String(e)))}>Open logs folder</button><small>The log files show what happened if something goes wrong during a service.</small></fieldset>}
    </div><aside className="settings-preview"><div className="preview-label">Live style preview</div><div className="settings-native-preview"><NativePreview target="style" label="Style preview"/></div><p>Changes apply live to the main output and are saved automatically.</p><button onClick={() => void onChange(defaultOutputConfig).catch(e => setError(String(e)))}>Reset to defaults</button></aside></div>
  </section></div>;
}
