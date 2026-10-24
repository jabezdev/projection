import { useEffect, useState, type ReactNode } from 'react';
import { invoke, isTauri } from './tauri';
import { Icon } from './Icons';
import { defaultOutputConfig, type OutputConfig } from './outputState';
import { NativePreview } from './NativePreview';

type Tab = 'projection' | 'confidence' | 'backgrounds' | 'system';
const TABS: [Tab, string][] = [['projection', 'Projection'], ['confidence', 'Confidence'], ['backgrounds', 'Logo & backgrounds'], ['system', 'System']];
const FONTS = ['Arial', 'Verdana', 'Tahoma', 'Segoe UI', 'Georgia', 'Times New Roman', 'Trebuchet MS', 'Impact', 'sans-serif', 'serif', 'monospace'];

/** Output settings, one section per screen, each with a live native preview. Changes save as they are made. */
export default function OutputSettings({ config, onChange, onClose, extraTabs }: { config: OutputConfig; onChange: (config: OutputConfig) => Promise<void>; onClose: () => void; extraTabs?: { id: string; label: string; content: ReactNode }[] }) {
  const [error, setError] = useState('');
  const [tab, setTab] = useState<string>('projection');
  // The native engine renders both samples in exactly the outputs' style.
  useEffect(() => { if (isTauri()) void invoke('preview_output_style', { config }).catch(e => setError(String(e))); }, [config]);
  const update = (patch: Partial<OutputConfig>) => { void onChange({ ...config, ...patch }).catch(e => setError(String(e))); };
  const range = (key: keyof OutputConfig, label: string, min: number, max: number, step = 1) => <label className="setting-range">{label}<output>{String(config[key])}</output><input aria-label={label} type="range" min={min} max={max} step={step} value={Number(config[key])} onChange={e => update({ [key]: Number(e.target.value) })}/></label>;
  const color = (key: keyof OutputConfig, label: string) => <label className="setting-color">{label}<input aria-label={label} type="color" value={String(config[key])} onChange={e => update({ [key]: e.target.value })}/></label>;
  const toggle = (key: keyof OutputConfig, label: string) => <label className="setting-check"><input type="checkbox" checked={Boolean(config[key])} onChange={e => update({ [key]: e.target.checked })}/>{label}</label>;
  const select = <K extends keyof OutputConfig>(key: K, label: string, options: [OutputConfig[K], string][]) => <label>{label}<select aria-label={label} value={String(config[key])} onChange={e => update({ [key]: e.target.value } as Partial<OutputConfig>)}>{options.map(([value, text]) => <option key={String(value)} value={String(value)}>{text}</option>)}</select></label>;
  const fontField = (key: 'font' | 'confidenceFont', label: string) => <><label>{label}<input aria-label={label} list="projection-fonts" value={config[key]} onChange={e => update({ [key]: e.target.value })}/></label><small>Use any font installed on this computer.</small></>;
  const preview = (target: 'style' | 'confidence-style', note: string) => <aside className="settings-preview"><div className="preview-label">Live preview</div><div className="settings-native-preview"><NativePreview target={target} label={`${target === 'style' ? 'Projection' : 'Confidence'} style preview`}/></div><p>{note}</p><button onClick={() => void onChange(defaultOutputConfig).catch(e => setError(String(e)))}>Reset all output settings</button></aside>;
  const extra = extraTabs?.find(t => t.id === tab);
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') onClose();
    if (e.key === 'Tab') {
      const targets = e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input, select');
      const first = targets[0], last = targets[targets.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
      if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
    } }}><section className="output-settings" role="dialog" aria-modal="true" aria-label="Settings">
    <header className="surface-header"><div><small>SETTINGS</small><h2>Screens & system</h2></div><button autoFocus className="icon-button" aria-label="Close settings" onClick={onClose}><Icon name="close"/></button></header>
    <nav className="settings-tabs" role="tablist" aria-label="Settings sections">{[...TABS, ...(extraTabs || []).map(t => [t.id, t.label] as [string, string])].map(([id, label]) => <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>{label}</button>)}</nav>
    {error && <p role="alert" className="ui-error">{error}</p>}
    <datalist id="projection-fonts">{FONTS.map(font => <option key={font}>{font}</option>)}</datalist>
    {tab === 'projection' && <div className="settings-body"><div className="settings-fields">
      <fieldset><legend>Typography</legend>{fontField('font', 'Font family')}
        {range('size', 'Font size (% of screen width)', 1, 16, .1)}{range('weight', 'Font weight', 100, 900, 100)}
        <div className="setting-toggles"><button aria-pressed={config.weight >= 700} onClick={() => update({ weight: config.weight >= 700 ? 400 : 700 })}><b>Bold</b></button><button aria-pressed={config.italic} onClick={() => update({ italic: !config.italic })}><i>Italic</i></button><button aria-pressed={config.caps} onClick={() => update({ caps: !config.caps })}>ALL CAPS</button></div>
        {color('color', 'Text color')}{range('lineHeight', 'Line height', .8, 2, .05)}{range('letterSpacing', 'Letter spacing', -2, 15, .5)}
      </fieldset>
      <fieldset><legend>Placement</legend>{select('vertical', 'Vertical position', [['top', 'Top'], ['center', 'Center'], ['bottom', 'Bottom']])}{select('align', 'Text alignment', [['left', 'Left'], ['center', 'Center'], ['right', 'Right']])}{range('marginX', 'Horizontal safe margin (%)', 0, 30)}{range('marginY', 'Vertical safe margin (%)', 0, 30)}</fieldset>
      <fieldset><legend>Text box</legend>{range('border', 'Border width', 0, 10)}{color('borderColor', 'Border color')}{color('boxColor', 'Box fill')}{range('boxOpacity', 'Box opacity (%)', 0, 100)}</fieldset>
      <fieldset><legend>Transition</legend>{select('transitionKind', 'Style', [['fade', 'Crossfade'], ['cut', 'Cut'], ['slide', 'Slide']])}
        {config.transitionKind !== 'cut' && <>{range('transition', 'Duration (ms)', 0, 1500, 50)}{select('transitionEasing', 'Easing', [['linear', 'Linear'], ['ease', 'Ease'], ['ease_in', 'Ease in'], ['ease_out', 'Ease out'], ['ease_in_out', 'Ease in and out']])}</>}
        {config.transitionKind === 'slide' && select('transitionDirection', 'Direction', [['left', 'To the left'], ['right', 'To the right'], ['up', 'Upward'], ['down', 'Downward']])}
        <small>Applies to lyrics and to backgrounds, images and slides on the projection.</small></fieldset>
      <fieldset><legend>Shadow</legend>{toggle('textShadow', 'Text shadow')}<fieldset className="setting-group" disabled={!config.textShadow}>{range('shadowSize', 'Blur', 0, 40)}{range('shadowOffsetX', 'Offset across', -30, 30)}{range('shadowOffsetY', 'Offset down', -30, 30)}{color('shadowColor', 'Color')}{range('shadowOpacity', 'Opacity (%)', 0, 100)}</fieldset></fieldset>
      <fieldset><legend>Outline & glow</legend>{toggle('textOutline', 'Outline')}<fieldset className="setting-group" disabled={!config.textOutline}>{range('outlineWidth', 'Outline width', 1, 24)}{color('outlineColor', 'Outline color')}</fieldset>
        {toggle('textGlow', 'Glow')}<fieldset className="setting-group" disabled={!config.textGlow}>{range('glowSize', 'Glow size', 1, 48)}{color('glowColor', 'Glow color')}{range('glowOpacity', 'Glow opacity (%)', 0, 100)}</fieldset>
        <small>Effects add GPU work; check them on the church PC before a service.</small></fieldset>
    </div>{preview('style', 'Changes apply live to the projection and are saved automatically.')}</div>}
    {tab === 'confidence' && <div className="settings-body"><div className="settings-fields">
      <fieldset><legend>Lyrics</legend>{fontField('confidenceFont', 'Font family')}{range('confidenceSize', 'Text size (% of screen width)', 2, 8, .1)}{range('confidenceLineHeight', 'Line height', 1, 2, .05)}{toggle('confidenceCaps', 'ALL CAPS')}</fieldset>
      <fieldset><legend>Top bar</legend>{toggle('confidenceClock', 'Clock')}{toggle('confidenceTape', 'Song part tape')}<small>With both off, the lyrics use the whole screen.</small></fieldset>
      <fieldset><legend>Video</legend>{range('confidenceTransition', 'Video fade (ms)', 0, 1500, 50)}<small>Videos appear here when their Confidence option is on.</small></fieldset>
    </div>{preview('confidence-style', 'The confidence screen has its own style; changes apply live.')}</div>}
    {tab === 'backgrounds' && <div className="settings-body"><div className="settings-fields">
      <fieldset><legend>Backgrounds</legend>{select('backgroundFit', 'Image and video fit', [['cover', 'Fill screen'], ['contain', 'Fit entire image / video']])}</fieldset>
      <fieldset><legend>Logo</legend><label>Logo image or video<input aria-label="Logo media path" value={config.logoPath} placeholder="Choose an image or video for the Logo button" onChange={e => update({ logoPath: e.target.value })}/></label>{isTauri() && <button onClick={() => void invoke<string | null>('choose_media_file', { kind: 'logo' }).then(path => { if (path) update({ logoPath: path }); }).catch(e => setError(String(e)))}>Choose logo media…</button>}{config.logoPath && <button onClick={() => update({ logoPath: '' })}>Clear logo media</button>}<small>Logo videos play muted and loop. They are kept ready, so Logo appears at once.</small></fieldset>
      <fieldset><legend>Animated backgrounds</legend>{color('shaderColorA', 'First color')}{color('shaderColorB', 'Second color')}{range('shaderSpeed', 'Speed', .1, 3, .1)}{toggle('shaderAnimate', 'Animate (off shows a still)')}<small>Choose one as a song's background in the song editor (Gradient, Aurora or Bokeh lights). They render at reduced resolution and frame rate, and fall back to a still if the GPU cannot keep up.</small></fieldset>
    </div>{preview('style', 'The sample uses the projection style; backgrounds show on the outputs.')}</div>}
    {tab === 'system' && <div className="settings-body settings-single"><div className="settings-fields">{isTauri() ? <SystemSettings onError={setError}/> : <p>System settings are available in the desktop app.</p>}</div></div>}
    {extra && <div className="settings-body settings-single">{extra.content}</div>}
  </section></div>;
}

type AppInfo = { version: string; data_dir: string; last_backup_ms: number | null };
type VideoDecoding = { choice: string; available: { name: string; label: string }[] };

/** Software decoding, automatic hardware decoding, or one accelerator found on this computer. */
function VideoDecodingSettings({ onError }: { onError: (message: string) => void }) {
  const [decoding, setDecoding] = useState<VideoDecoding | null>(null);
  useEffect(() => { void invoke<VideoDecoding>('get_video_decoding').then(setDecoding).catch(e => onError(String(e))); }, [onError]);
  const choose = (choice: string) => {
    void invoke('set_video_decoder', { choice }).then(() => setDecoding(d => d && { ...d, choice })).catch(e => onError(String(e)));
  };
  const options: [string, string][] = [['software', 'Software (CPU)'], ...(decoding?.available.length ? [['auto', 'Automatic (hardware when possible)'] as [string, string]] : []), ...(decoding?.available.map(d => [d.name, d.label] as [string, string]) ?? [])];
  // A saved accelerator that is missing now still shows, so the choice is visible.
  if (decoding && !options.some(([name]) => name === decoding.choice)) options.push([decoding.choice, `${decoding.choice} (not available here)`]);
  return <fieldset><legend>Video decoding</legend>
    <label>Decoder<select aria-label="Video decoder" disabled={!decoding} value={decoding?.choice ?? 'software'} onChange={e => choose(e.target.value)}>{options.map(([name, label]) => <option key={name} value={name}>{label}</option>)}</select></label>
    <small>{!decoding ? 'Looking for hardware decoders…' : decoding.available.length ? 'Hardware decoding takes video work off the processor. It applies to videos started after the change; a video the hardware cannot decode plays in software instead. Check videos on the church PC after changing this.' : 'No hardware decoder was found on this computer, so videos decode in software.'}</small>
  </fieldset>;
}

/** Version, backups and logs: what the operator needs when something goes wrong. */
function SystemSettings({ onError }: { onError: (message: string) => void }) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [status, setStatus] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => { void invoke<AppInfo>('get_app_info').then(setInfo).catch(e => onError(String(e))); }, [onError]);
  const exportBackup = async () => {
    setBusy(true); setStatus('');
    try { const path = await invoke<string | null>('export_backup'); if (path) setStatus(`Backup saved to ${path}`); }
    catch (e) { onError(String(e)); }
    finally { setBusy(false); }
  };
  return <>
    <fieldset><legend>About</legend><small>PALTO {info?.version}</small><small>Data folder: {info?.data_dir}</small></fieldset>
    <fieldset><legend>Backups</legend>
      <button disabled={busy} onClick={() => void exportBackup()}>{busy ? 'Exporting…' : 'Export backup…'}</button>
      <button onClick={() => void invoke('open_backups_folder').catch(e => onError(String(e)))}>Open backups folder</button>
      <small>{status || `A backup holds the library, services and settings, plus a list of the media files they use (not the files themselves). One is also made automatically each week${info?.last_backup_ms ? `; the last was ${new Date(info.last_backup_ms).toLocaleString()}` : ''}.`}</small>
    </fieldset>
    <VideoDecodingSettings onError={onError}/>
    <fieldset><legend>Troubleshooting</legend>
      <button onClick={() => void invoke('open_logs_folder').catch(e => onError(String(e)))}>Open logs folder</button>
      <small>The log files show what happened if something goes wrong during a service.</small>
    </fieldset>
  </>;
}
