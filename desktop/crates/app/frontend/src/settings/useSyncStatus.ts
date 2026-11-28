import { useCallback, useEffect, useState } from 'react';
import { invoke, listen } from '../api/tauri';
import type { SyncStatus } from '../api/models';

export function useSyncStatus() {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const load = useCallback(() => invoke<SyncStatus>('get_sync_status').then(setStatus, () => {}), []);
  useEffect(() => {
    void load();
    let unlisten: (() => void) | undefined;
    void listen<SyncStatus>('sync:status', e => setStatus(e.payload)).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, [load]);
  return [status, load] as const;
}
