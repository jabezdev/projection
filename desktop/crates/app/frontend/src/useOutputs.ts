import { useEffect, useState } from 'react';
import { invoke, isTauri, listen } from './tauri';

/** Whether the outputs are shown (the Screens switch), kept current. */
export function useOutputsShown(): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void listen<boolean>('output:shown', ({ payload }) => { if (!disposed) setShown(payload); }).then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; });
    void invoke<{ shown: boolean }>('get_output_displays').then(displays => { if (!disposed) setShown(displays.shown); }).catch(() => {});
    return () => { disposed = true; cleanup?.(); };
  }, []);
  return shown;
}

/** Outputs waiting for new media to load; the previous picture is still on air meanwhile. */
export function useOutputsLoading(): Record<string, boolean> {
  const [loading, setLoading] = useState<Record<string, boolean>>({});
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void listen<{ target: string; loading: boolean }>('output:status', ({ payload }) => { if (!disposed) setLoading(old => ({ ...old, [payload.target]: payload.loading })); })
      .then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; });
    return () => { disposed = true; cleanup?.(); };
  }, []);
  return loading;
}
