import { useCallback, useEffect, useState } from 'react';
import { invoke, listen } from './tauri';

export type UpdateState =
  | { state: 'idle' } | { state: 'checking' } | { state: 'up_to_date'; checked_ms: number }
  | { state: 'downloading'; version: string; received: number; total: number | null }
  | { state: 'ready'; version: string; notes: string } | { state: 'failed'; message: string } | { state: 'unsupported' };
type UpdateStatus = UpdateState & { current_version: string; earlier: string[]; can_install: boolean };

const mb = (bytes: number) => `${(bytes / 2 ** 20).toFixed(0)} MB`;

/** Check, read the notes, install (never while outputs are shown), or reinstall an earlier version. */
export default function UpdateSettings({ onError }: { onError: (message: string) => void }) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [earlier, setEarlier] = useState('');
  const load = useCallback(() => void invoke<UpdateStatus>('get_update_status').then(setStatus).catch(e => onError(String(e))), [onError]);
  useEffect(() => {
    load();
    const timer = window.setInterval(load, 2000);
    let unlisten: (() => void) | undefined;
    void listen('update:state', load).then(u => { unlisten = u; });
    return () => { window.clearInterval(timer); unlisten?.(); };
  }, [load]);
  if (!status) return null;
  const busy = status.state === 'checking' || status.state === 'downloading';
  const line = {
    idle: 'PALTO checks for updates a few minutes after it starts and every few hours.',
    checking: 'Checking…',
    up_to_date: status.state === 'up_to_date' ? `Up to date (checked ${new Date(status.checked_ms).toLocaleTimeString()}).` : '',
    downloading: status.state === 'downloading' ? `Downloading ${status.version}… ${mb(status.received)}${status.total ? ` of ${mb(status.total)}` : ''}` : '',
    ready: status.state === 'ready' ? `PALTO ${status.version} is ready to install.` : '',
    failed: status.state === 'failed' ? `The last check failed: ${status.message}` : '',
    unsupported: 'Updates are published for the Windows version only.',
  }[status.state];
  const choice = earlier || status.earlier[0] || '';
  return <fieldset><legend>Updates</legend>
    <small>Installed: PALTO {status.current_version}</small>
    <small>{line}</small>
    {status.state === 'ready' && <>
      {status.notes && <pre className="update-notes">{status.notes}</pre>}
      <button className="primary-action" disabled={!status.can_install} onClick={() => void invoke('install_update').catch(e => onError(String(e)))}>Install and restart</button>
      {!status.can_install && <small>Hide the outputs first: updates never install during a service.</small>}
    </>}
    {status.state !== 'unsupported' && status.state !== 'ready' && <button disabled={busy} onClick={() => void invoke('check_for_update').then(load).catch(e => onError(String(e)))}>Check for updates</button>}
    {status.earlier.length > 0 && <>
      <label>Earlier version<select aria-label="Earlier version" value={choice} onChange={e => setEarlier(e.target.value)}>{status.earlier.map(v => <option key={v} value={v}>PALTO {v}</option>)}</select></label>
      <button disabled={!status.can_install} onClick={() => void invoke('reinstall_version', { version: choice }).catch(e => onError(String(e)))}>Reinstall PALTO {choice}</button>
      <small>Goes back to an earlier version if an update causes trouble. The installer asks before replacing the newer version.</small>
    </>}
  </fieldset>;
}
