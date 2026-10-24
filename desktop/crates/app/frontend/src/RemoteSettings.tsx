import { useCallback, useEffect, useState } from 'react';
import { invoke } from './tauri';

type RemoteStatus = {
  lan_enabled: boolean; port: number; listening: boolean; error: string | null; remote_clients: number;
  name_url: string; address_urls: string[]; qr_svg: string | null;
  pairing_code: string | null; pairing_seconds_left: number;
  devices: { id: string; name: string; paired_ms: number; last_seen_ms: number }[];
  osc: { enabled: boolean; port: number; allowed: string[]; listening: boolean; error: string | null; last: [string, string, number, boolean] | null };
};
type Settings = { lan_enabled: boolean; port: number; osc_enabled: boolean; osc_port: number; osc_allowed: string[] };

const settingsOf = (s: RemoteStatus): Settings => ({ lan_enabled: s.lan_enabled, port: s.port, osc_enabled: s.osc.enabled, osc_port: s.osc.port, osc_allowed: s.osc.allowed });
const ago = (ms: number) => ms < 60_000 ? `${Math.round(ms / 1000)} s ago` : ms < 3_600_000 ? `${Math.round(ms / 60_000)} min ago` : `${Math.round(ms / 3_600_000)} h ago`;

/** LAN access, pairing phones and tablets, and OSC for Companion and similar controllers. */
export default function RemoteSettings() {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [loadedAt, setLoadedAt] = useState(0);
  const [port, setPort] = useState('');
  const [oscPort, setOscPort] = useState('');
  const [allowed, setAllowed] = useState('');
  const [error, setError] = useState('');
  const load = useCallback(() => invoke<RemoteStatus>('get_remote_status').then(s => { setStatus(s); setLoadedAt(Date.now()); return s; }), []);
  useEffect(() => {
    void load().then(s => { setPort(String(s.port)); setOscPort(String(s.osc.port)); setAllowed(s.osc.allowed.join('\n')); }).catch(e => setError(String(e)));
    const timer = window.setInterval(() => void load().catch(() => {}), 1000);
    return () => window.clearInterval(timer);
  }, [load]);
  if (!status) return <div className="settings-fields"><p>Loading…</p>{error && <p role="alert" className="ui-error">{error}</p>}</div>;

  const save = async (patch: Partial<Settings>) => {
    setError('');
    try { await invoke('set_remote_settings', { settings: { ...settingsOf(status), ...patch } }); await load(); }
    catch (e) { setError(String(e)); }
  };
  const run = (command: string, args?: Record<string, unknown>) => void invoke(command, args).then(load).catch(e => setError(String(e)));
  const last = status.osc.last;
  return <div className="settings-fields remote-settings">
    {error && <p role="alert" className="ui-error">{error}</p>}
    <fieldset><legend>Network access</legend>
      <label className="setting-check"><input type="checkbox" checked={status.lan_enabled} onChange={e => void save({ lan_enabled: e.target.checked })}/>Allow phones and other computers on this network</label>
      <label>Port<input aria-label="Remote port" inputMode="numeric" value={port} onChange={e => setPort(e.target.value)} onBlur={() => { if (Number(port) !== status.port) void save({ port: Number(port) }); }}/></label>
      <small>{status.error ?? (!status.lan_enabled ? 'Off: only programs on this computer can connect.' : status.listening ? `On. ${status.remote_clients ? `${status.remote_clients} device${status.remote_clients > 1 ? 's' : ''} connected.` : 'No devices connected.'} Devices must be paired first.` : 'Starting…')}</small>
      {status.lan_enabled && <small>If phones cannot connect, check that Windows treats this network as Private (Settings → Network → Properties).</small>}
    </fieldset>
    {status.lan_enabled && <fieldset><legend>Phone remote</legend>
      <small>Open this on the phone (same Wi-Fi):</small>
      <code className="remote-url">{status.name_url}</code>
      {status.address_urls[0] && <code className="remote-url">{status.address_urls[0]}</code>}
      {status.address_urls.length > 1 && <details><summary><small>Other addresses ({status.address_urls.length - 1})</small></summary>{status.address_urls.slice(1).map(url => <code key={url} className="remote-url">{url}</code>)}</details>}
      {status.qr_svg && <div className="remote-qr" aria-label={`QR code for ${status.address_urls[0]}`} dangerouslySetInnerHTML={{ __html: status.qr_svg }}/>}
      <small>Scan the code with the phone's camera. Some phones do not open palto.local addresses; use the numbered address then.</small>
    </fieldset>}
    {status.lan_enabled && <fieldset><legend>Pairing</legend>
      {status.pairing_code
        ? <><div className="pairing-code" aria-label="Pairing code">{status.pairing_code}</div><small>Enter this code on the device. It works for {Math.ceil(status.pairing_seconds_left / 60)} more minute{status.pairing_seconds_left > 60 ? 's' : ''}.</small><button onClick={() => run('stop_pairing')}>Stop pairing</button></>
        : <><button onClick={() => run('start_pairing')}>Pair a device</button><small>Shows a code for 5 minutes. Each device pairs once and is remembered.</small></>}
      {status.devices.length > 0 && <ul className="remote-devices">{status.devices.map(d => <li key={d.id}><span>{d.name}<small>Last used {ago(Math.max(0, loadedAt - d.last_seen_ms))}</small></span><button onClick={() => run('remove_remote_device', { id: d.id })}>Remove</button></li>)}</ul>}
    </fieldset>}
    <fieldset><legend>OSC (Companion and controllers)</legend>
      <label className="setting-check"><input type="checkbox" checked={status.osc.enabled} onChange={e => void save({ osc_enabled: e.target.checked })}/>Accept OSC messages</label>
      <fieldset className="setting-group" disabled={!status.osc.enabled}>
        <label>UDP port<input aria-label="OSC port" inputMode="numeric" value={oscPort} onChange={e => setOscPort(e.target.value)} onBlur={() => { if (Number(oscPort) !== status.osc.port) void save({ osc_port: Number(oscPort) }); }}/></label>
        <label>Other computers allowed to send (IP addresses, one per line)<textarea aria-label="OSC allowed computers" rows={3} value={allowed} onChange={e => setAllowed(e.target.value)} onBlur={() => void save({ osc_allowed: allowed.split(/\s+/).filter(Boolean) })}/></label>
        <small>Send <code>/palto/next</code>, <code>/palto/previous</code>, <code>/palto/next_item</code>, <code>/palto/previous_item</code>, <code>/palto/live</code>, <code>/palto/black</code>, <code>/palto/clear</code>, <code>/palto/logo</code>, <code>/palto/play_pause</code> or <code>/palto/stop</code>. A value of 0 (a button release) is ignored. This computer can always send; other computers need LAN access on and their address listed.</small>
        <small>{status.osc.error ?? (status.osc.enabled ? (last ? `Last message: ${last[1]} from ${last[0]}, ${ago(last[2])}${last[3] ? ' (ignored: not on the allowed list)' : ''}.` : 'No messages yet.') : '')}</small>
      </fieldset>
    </fieldset>
  </div>;
}
