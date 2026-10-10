import { useEffect, useState } from 'react';
import { invoke, listen, isTauri } from './tauri';
type Size = { width: number; height: number };
export function useOutputSizes(enabled: boolean) {
  const [sizes, setSizes] = useState({ projection: 16 / 9, confidence: 1.5 });
  useEffect(() => {
    if (!enabled || !isTauri()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    const update = (label: string, size: Size) => { if (!disposed && size.width > 0 && size.height > 0) setSizes(old => ({ ...old, [label]: size.width / size.height })); };
    void (async () => {
      const unlisten = await listen<Size & { label: string }>('output:resized', ({ payload }) => update(payload.label, payload));
      if (disposed) { unlisten(); return; }
      cleanup = unlisten;
      const initial = await invoke<Record<string, Size>>('get_output_sizes');
      Object.entries(initial).forEach(([label, size]) => update(label, size));
    })().catch(console.error);
    return () => { disposed = true; cleanup?.(); };
  }, [enabled]);
  return sizes;
}
