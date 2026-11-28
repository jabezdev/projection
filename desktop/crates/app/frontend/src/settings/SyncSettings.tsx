import { useCallback, useEffect, useState } from 'react';
import { invoke } from '../api/tauri';
import { useSyncStatus } from './useSyncStatus';
import type { Conflict, SyncStatus } from '../api/models';

const STATES: Record<SyncStatus['state'], string> = { off: 'Not connected', idle: 'Up to date', syncing: 'Syncing…', offline: 'Offline', error: 'Needs attention' };

const when = (ms?: number) => ms ? new Date(ms).toLocaleString(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' }) : 'never';

export default function SyncSettings() {
  const [status, load] = useSyncStatus();
  const [conflicts, setConflicts] = useState<Conflict[]>([]);
  const [comparing, setComparing] = useState<Conflict | null>(null);
  const [address, setAddress] = useState('');
  const [code, setCode] = useState('');
  const [name, setName] = useState('Projection computer');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const loadConflicts = useCallback(() => invoke<Conflict[]>('list_sync_conflicts').then(setConflicts, () => {}), []);
  useEffect(() => { void loadConflicts(); }, [loadConflicts, status?.conflicts]);
  const run = async (command: string, args?: Record<string, unknown>) => {
    setError(''); setBusy(true);
    try { await invoke(command, args); await load(); await loadConflicts(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  if (!status) return <div className="settings-fields"><p>Loading…</p></div>;

  if (!status.paired) return <div className="settings-fields sync-settings">
    {error && <p role="alert" className="ui-error">{error}</p>}
    <fieldset><legend>Connect to the team library</legend>
      <small>Songs, services and templates are shared with the team's web library. PALTO keeps its own copy, so a service never waits on the internet.</small>
      <label>Team address<input placeholder="happy-cat-123.convex.cloud" value={address} onChange={e => setAddress(e.target.value)}/></label>
      <label>Pairing code<input placeholder="ABCD-EFGH" value={code} onChange={e => setCode(e.target.value.toUpperCase())}/></label>
      <label>This computer's name<input value={name} onChange={e => setName(e.target.value)}/></label>
      <button className="primary-action" disabled={busy || !address.trim() || !code.trim()} onClick={() => void run('pair_sync', { deployment: address, code, deviceName: name })}>Connect</button>
      <small>An admin or operator gets the address and a code from the web library: Team → Projection computers.</small>
    </fieldset>
  </div>;

  return <div className="settings-fields sync-settings">
    {error && <p role="alert" className="ui-error">{error}</p>}
    <fieldset><legend>Team library</legend>
      <div className={`sync-state is-${status.state}`}><strong>{STATES[status.state]}</strong><small>Last synced {when(status.lastSyncedMs)}</small></div>
      {status.error && <small className="sync-error">{status.error}</small>}
      <small>{status.pending ? `${status.pending} change${status.pending > 1 ? 's' : ''} waiting to go up.` : 'Every change here is in the team library.'}
        {status.uploads ? ` ${status.uploads} media file${status.uploads > 1 ? 's' : ''} to upload.` : ''}{status.downloads ? ` ${status.downloads} to download.` : ''}</small>
      {status.liveService && <small>This computer is running a service live, so the team can't edit it until it ends.</small>}
      <div className="sync-actions">
        <button disabled={busy || status.offline} onClick={() => void run('sync_now')}>Sync now</button>
        <label className="setting-check"><input type="checkbox" checked={status.offline} onChange={e => void run('set_sync_offline', { offline: e.target.checked })}/>Work offline</label>
      </div>
      <small>Media files up to 100 MB are shared. Uploads pause while a video or song plays.</small>
    </fieldset>
    <fieldset><legend>Needs attention {conflicts.length ? `(${conflicts.length})` : ''}</legend>
      {!conflicts.length && <small>Nothing. When something is changed here and online at the same time, it shows up here and the service carries on.</small>}
      <ul className="sync-conflicts">{conflicts.map(c => <li key={c.id}>
        <span><strong>{c.title}</strong><small>{c.label}</small></span>
        <button onClick={() => setComparing(c)}>Compare…</button>
      </li>)}</ul>
    </fieldset>
    <fieldset><legend>This computer</legend>
      <small>{status.deviceName} · {status.deployment}</small>
      <button className="danger-action" disabled={busy} onClick={() => void run('unpair_sync')}>Disconnect</button>
      <small>Everything stays on this computer. To stop it for good, also remove it on the web: Team → Projection computers.</small>
    </fieldset>
    {comparing && <CompareDialog conflict={comparing} onClose={() => setComparing(null)} onKeep={mine => { setComparing(null); void run('resolve_sync_conflict', { conflictId: comparing.id, keepMine: mine }); }}/>}
  </div>;
}

function CompareDialog({ conflict, onKeep, onClose }: { conflict: Conflict; onKeep: (mine: boolean) => void; onClose: () => void }) {
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}>
    <section className="compact-modal compare-modal" role="dialog" aria-modal="true" aria-label="Compare changes">
      <header>{conflict.title}</header>
      <p className="field-hint">{conflict.label}. It was changed online while it was also changed here. For now both use the online version.</p>
      <div className="compare-columns">
        <div><span className="eyebrow">Here</span><Shown value={conflict.mine}/></div>
        <div><span className="eyebrow">Online</span><Shown value={conflict.theirs}/></div>
      </div>
      <div className="modal-actions"><button onClick={onClose}>Decide later</button><button onClick={() => onKeep(false)}>Keep online</button><button className="primary-action" autoFocus onClick={() => onKeep(true)}>Keep mine</button></div>
    </section>
  </div>;
}

function Shown({ value }: { value: unknown }) {
  if (value === null || value === undefined || value === '') return <p className="compare-value is-empty">Empty</p>;
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return <p className="compare-value">{String(value)}</p>;
  const lyrics = typeof value === 'object' && value && 'lyrics' in value ? String((value as { lyrics: unknown }).lyrics) : null;
  return <pre className="compare-value">{lyrics ?? JSON.stringify(value, null, 2)}</pre>;
}
