import { useEffect, useState } from 'react';
import { invoke, isTauri } from './tauri';
import { mediaSource } from './media';
const posters = new Map<string, Promise<string>>();
export function useVideoPoster(path: string) {
  const [poster, setPoster] = useState({ path: '', url: '' });
  useEffect(() => {
    let disposed = false;
    if (path && isTauri()) {
      let pending = posters.get(path);
      if (!pending) {
        pending = invoke<string>('video_thumbnail', { path }).then(mediaSource).catch(error => { console.warn('Video thumbnail:', error); posters.delete(path); return ''; });
        posters.set(path, pending);
      }
      void pending.then(url => { if (!disposed) setPoster({ path, url }); });
    }
    return () => { disposed = true; };
  }, [path]);
  return poster.path === path ? poster.url : '';
}
