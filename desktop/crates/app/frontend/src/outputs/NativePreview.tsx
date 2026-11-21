import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri, listen } from '../api/tauri';
import { useVideoPoster } from '../library/useVideoPoster';

type Target = 'projection' | 'confidence' | 'style' | 'confidence-style';
export function NativePreview({ target, label }: { target: Target; label: string }) {
  const [url, setUrl] = useState('');
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    let current = '';
    let requested = 0;
    const load = () => {
      const request = ++requested;
      invoke<ArrayBuffer>('get_output_preview', { target }).then(bytes => {
        if (disposed || request !== requested || !(bytes instanceof ArrayBuffer || ArrayBuffer.isView(bytes))) return;
        const next = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }));
        if (current) URL.revokeObjectURL(current);
        current = next;
        setUrl(next);
      }).catch(() => {  });
    };
    void listen<{ target: string }>('output:preview', ({ payload }) => { if (payload.target === target) load(); })
      .then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; })
      .catch(console.error);
    load();
    return () => { disposed = true; cleanup?.(); if (current) URL.revokeObjectURL(current); };
  }, [target]);
  return url ? <img className="native-preview" src={url} alt={label} draggable={false}/> : <div className="native-preview is-empty" role="img" aria-label={label}/>;
}

export function VideoPosterPreview({ path }: { path: string }) {
  const poster = useVideoPoster(path);
  return poster ? <img className="native-preview" src={poster} alt="Confidence video" draggable={false}/> : <div className="native-preview is-empty" role="img" aria-label="Confidence video"/>;
}

type Rect = { x: number; y: number; width: number; height: number };

const OVERLAYS = '.modal-backdrop, [aria-modal="true"], [data-covers-monitors]';

export function LiveMonitor({ target, label }: { target: 'projection' | 'confidence'; label: string }) {
  const element = useRef<HTMLDivElement>(null);
  const [live, setLive] = useState(isTauri);
  useEffect(() => {
    const node = element.current;
    if (!node || !live) return;
    let disposed = false;
    let frame = 0;
    let sent = 'unsent';
    const send = (rect: Rect | null) => {
      const key = JSON.stringify(rect);
      if (key === sent) return;
      sent = key;
      invoke<boolean>('set_output_monitor', { target, rect }).then(available => {
        if (!available && !disposed) setLive(false);
      }).catch(console.error);
    };
    const update = () => {
      frame = 0;
      if (disposed) return;
      send(visibleRect(node));
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(update); };
    const resize = new ResizeObserver(() => { if (frame) cancelAnimationFrame(frame); update(); });
    resize.observe(node);
    const mutations = new MutationObserver(schedule);
    mutations.observe(document.body, { childList: true, subtree: true });
    window.addEventListener('resize', schedule);
    window.addEventListener('scroll', schedule, true);
    document.addEventListener('visibilitychange', schedule);
    schedule();
    return () => {
      disposed = true;
      if (frame) cancelAnimationFrame(frame);
      resize.disconnect();
      mutations.disconnect();
      window.removeEventListener('resize', schedule);
      window.removeEventListener('scroll', schedule, true);
      document.removeEventListener('visibilitychange', schedule);
      send(null);
    };
  }, [target, live]);
  return <div ref={element} className="live-monitor"><NativePreview target={target} label={label}/></div>;
}

function visibleRect(node: HTMLElement): Rect | null {
  if (document.visibilityState !== 'visible' || document.querySelector(OVERLAYS)) return null;
  const box = node.getBoundingClientRect();
  if (box.width < 2 || box.height < 2) return null;
  const inside = (outer: DOMRect) => box.left >= outer.left - 0.5 && box.top >= outer.top - 0.5 && box.right <= outer.right + 0.5 && box.bottom <= outer.bottom + 0.5;
  if (!inside(new DOMRect(0, 0, window.innerWidth, window.innerHeight))) return null;
  for (let parent = node.parentElement; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (style.display === 'none' || style.visibility === 'hidden') return null;
    if (style.overflow !== 'visible' && !inside(parent.getBoundingClientRect())) return null;
  }
  const scale = window.devicePixelRatio || 1;
  const x = Math.round(box.left * scale), y = Math.round(box.top * scale);
  return { x, y, width: Math.round(box.right * scale) - x, height: Math.round(box.bottom * scale) - y };
}
