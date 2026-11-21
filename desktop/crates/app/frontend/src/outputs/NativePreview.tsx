import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri, listen } from '../api/tauri';
import { useVideoPoster } from '../library/useVideoPoster';

type Target = 'projection' | 'confidence' | 'style' | 'confidence-style';
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
        // The browser-only mode renders nothing; keep the empty frame.
        if (disposed || request !== requested || !(bytes instanceof ArrayBuffer || ArrayBuffer.isView(bytes))) return;
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

type Rect = { x: number; y: number; width: number; height: number };

/** Anything HTML draws above the page, which a native monitor would cover. */
const OVERLAYS = '.modal-backdrop, [aria-modal="true"], [data-covers-monitors]';

/**
 * A real-time view of an output: a native window the engine presents into,
 * kept over this element. HTML cannot draw over it, so it hides while a
 * dialog is open, while the element is scrolled or clipped out of view and
 * while the page is hidden; the still preview underneath shows meanwhile,
 * and stays when live monitors are unavailable (or outside the desktop app).
 */
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
    // Resize callbacks run after layout, before paint: placing the monitor
    // here keeps it a frame closer to a column being dragged.
    const resize = new ResizeObserver(() => { if (frame) cancelAnimationFrame(frame); update(); });
    resize.observe(node);
    // Dialogs open and layouts shift through DOM changes.
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

/** `node`'s area in physical pixels, or null unless it is wholly visible. */
function visibleRect(node: HTMLElement): Rect | null {
  if (document.visibilityState !== 'visible' || document.querySelector(OVERLAYS)) return null;
  const box = node.getBoundingClientRect();
  if (box.width < 2 || box.height < 2) return null;
  // A partly clipped monitor would spill over the UI around it.
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
