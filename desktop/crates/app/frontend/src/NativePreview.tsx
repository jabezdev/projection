import { useEffect, useState } from 'react';
import { invoke, listen } from './tauri';
import { useVideoPoster } from './useVideoPoster';

type Target = 'projection' | 'confidence' | 'style';
/** A still rendered by the native output engine, refreshed when its scene settles. */
export function NativePreview({ target, label }: { target: Target; label: string }) {
  const [url, setUrl] = useState('');
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    let current = '';
    let requested = 0;
    const load = () => {
      // Only the newest request may replace the image.
      const request = ++requested;
      invoke<ArrayBuffer>('get_output_preview', { target }).then(bytes => {
        if (disposed || request !== requested) return;
        const next = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }));
        if (current) URL.revokeObjectURL(current);
        current = next;
        setUrl(next);
      }).catch(() => { /* Not rendered yet; an event follows when it is. */ });
    };
    void listen<{ target: string }>('output:preview', ({ payload }) => { if (payload.target === target) load(); })
      .then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; })
      .catch(console.error);
    load();
    return () => { disposed = true; cleanup?.(); if (current) URL.revokeObjectURL(current); };
  }, [target]);
  return url ? <img className="native-preview" src={url} alt={label} draggable={false}/> : <div className="native-preview is-empty" role="img" aria-label={label}/>;
}

/** A video's poster in place of playback, so previews never decode video. */
export function VideoPosterPreview({ path }: { path: string }) {
  const poster = useVideoPoster(path);
  return poster ? <img className="native-preview" src={poster} alt="Confidence video" draggable={false}/> : <div className="native-preview is-empty" role="img" aria-label="Confidence video"/>;
}
