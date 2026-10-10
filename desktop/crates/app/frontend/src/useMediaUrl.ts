import { useEffect, useState } from 'react';
import { invoke, isTauri } from './tauri';
import { mediaSource } from './media';
const resources = new Map<string, Promise<string>>();
function resolveResource(path: string) {
  let pending = resources.get(path);
  if (!pending) {
    pending = invoke<string>('resolve_media_path', { path }).then(mediaSource);
    resources.set(path, pending);
    void pending.catch(() => resources.delete(path));
  }
  return pending;
}
export function useMediaResource(path: string) {
  const [resolved, setResolved] = useState({ path: '', url: '', error: '' });
  useEffect(() => {
    let disposed = false;
    if (path && isTauri()) {
      void resolveResource(path).then(url => { if (!disposed) setResolved({ path, url, error: '' }); }).catch(error => { if (!disposed) setResolved({ path, url: '', error: String(error) }); });
    }
    return () => { disposed = true; };
  }, [path]);
  return !path ? { url: '', error: '' } : !isTauri() ? { url: mediaSource(path), error: '' } : resolved.path === path ? resolved : { url: '', error: '' };
}

export function useMediaUrl(path: string) { return useMediaResource(path).url; }
