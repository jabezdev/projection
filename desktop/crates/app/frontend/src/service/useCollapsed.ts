import { useCallback, useState } from 'react';

const KEY = 'palto.collapsedSections';

function read(): Set<string> {
  try { return new Set(JSON.parse(window.localStorage.getItem(KEY) ?? '[]') as string[]); } catch { return new Set(); }
}

export function useCollapsed(): [Set<string>, (id: string, collapsed: boolean) => void] {
  const [ids, setIds] = useState(read);
  const set = useCallback((id: string, collapsed: boolean) => setIds(current => {
    const next = new Set(current);
    if (collapsed) next.add(id); else next.delete(id);
    try { window.localStorage.setItem(KEY, JSON.stringify([...next].slice(-500))); } catch { return next; }
    return next;
  }), []);
  return [ids, set];
}
