import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { isTauri } from '../api/tauri';

const DIVIDERS = 10;
const MIN_COLUMN = 140;
const DEFAULT: Fractions = [0.3, 0.2];
const STORAGE_KEY = 'palto.operator-columns.maximized';

type Fractions = [number, number];

function loadSaved(): Fractions | null {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    return Array.isArray(value) && value.length === 2 && value.every(n => typeof n === 'number' && n > 0) && value[0] + value[1] < 1 ? [value[0], value[1]] : null;
  } catch { return null; }
}

function save(fractions: Fractions) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(fractions)); } catch {  }
}

function useMaximized(): boolean | null {
  const [maximized, setMaximized] = useState<boolean | null>(null);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    if (isTauri()) {
      void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
        const window = getCurrentWindow();
        const check = async () => {
          const value = (await window.isMaximized()) || (await window.isFullscreen());
          if (!disposed) setMaximized(value);
        };
        await check();
        const unlisten = await window.onResized(() => void check());
        if (disposed) unlisten(); else cleanup = unlisten;
      }).catch(() => { if (!disposed) setMaximized(false); });
    } else {
      const check = () => setMaximized(Boolean(document.fullscreenElement)
        || (window.outerWidth >= screen.availWidth - 2 && window.outerHeight >= screen.availHeight - 2));
      check();
      window.addEventListener('resize', check);
      cleanup = () => window.removeEventListener('resize', check);
    }
    return () => { disposed = true; cleanup?.(); };
  }, []);
  return maximized;
}

function widths(fractions: Fractions, available: number): [number, number] {
  const minimum = Math.min(MIN_COLUMN, available / 3);
  const right = Math.max(minimum, Math.min(fractions[1] * available, available - minimum * 2));
  const left = Math.max(minimum, Math.min(fractions[0] * available, available - right - minimum));
  return [left, right];
}

export function useColumns() {
  const maximized = useMaximized();
  const [fractions, setFractions] = useState<Fractions>(DEFAULT);
  const [available, setAvailable] = useState(0);
  const panesRef = useRef<HTMLDivElement>(null);
  const fractionsRef = useRef(fractions);
  const maximizedRef = useRef(maximized);
  const resizeFrame = useRef<number | null>(null);
  const resizeRef = useRef<{ index: number; x: number; fractions: Fractions } | null>(null);
  useEffect(() => { fractionsRef.current = fractions; }, [fractions]);
  useEffect(() => { maximizedRef.current = maximized; }, [maximized]);

  const [snappedFor, setSnappedFor] = useState<boolean | null>(null);
  if (maximized !== snappedFor) {
    setSnappedFor(maximized);
    if (maximized != null) setFractions(maximized ? loadSaved() ?? DEFAULT : DEFAULT);
  }

  useEffect(() => {
    const element = panesRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setAvailable(Math.max(0, element.clientWidth - DIVIDERS)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = (value: Fractions) => {
    const [left, right] = widths(value, available);
    return `${left}px 5px minmax(0, 1fr) 5px ${right}px`;
  };

  function resized(value: Fractions, index: number, delta: number): Fractions {
    if (available <= 0) return value;
    const [left, right] = widths(value, available);
    const middle = available - left - right;
    const minimum = Math.min(MIN_COLUMN, available / 3);
    if (index === 0) {
      const next = Math.max(minimum, Math.min(left + middle - minimum, left + delta));
      return [next / available, right / available];
    }
    const next = Math.max(minimum, Math.min(right + middle - minimum, right - delta));
    return [left / available, next / available];
  }

  function commit(value: Fractions) {
    setFractions(value);
    if (maximizedRef.current) save(value);
  }

  function finishResize() {
    if (!resizeRef.current) return;
    if (resizeFrame.current !== null) cancelAnimationFrame(resizeFrame.current);
    resizeFrame.current = null;
    panesRef.current?.classList.remove('is-resizing');
    resizeRef.current = null;
    commit(fractionsRef.current);
  }

  const [leftPx, rightPx] = widths(fractions, available);
  const valueNow = (index: number) => Math.round(index === 0 ? leftPx : rightPx);
  const resizeDivider = (index: number) => <div className="column-divider" role="separator"
    aria-label={index === 0 ? 'Resize session and lyrics columns' : 'Resize lyrics and controls columns'}
    aria-orientation="vertical" aria-valuenow={valueNow(index)} aria-valuemin={MIN_COLUMN}
    aria-valuemax={Math.round(available - (index === 0 ? rightPx : leftPx) - MIN_COLUMN)} tabIndex={0}
    onDoubleClick={() => commit(DEFAULT)}
    onPointerDown={event => {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      resizeRef.current = { index, x: event.clientX, fractions: fractionsRef.current };
      panesRef.current?.classList.add('is-resizing');
    }}
    onPointerMove={event => {
      const drag = resizeRef.current;
      if (!drag) return;
      const next = resized(drag.fractions, index, event.clientX - drag.x);
      fractionsRef.current = next;
      if (resizeFrame.current === null) resizeFrame.current = requestAnimationFrame(() => {
        resizeFrame.current = null;
        panesRef.current?.style.setProperty('--operator-columns', columns(fractionsRef.current));
      });
      resizeRef.current = { index, x: event.clientX, fractions: next };
    }}
    onPointerUp={finishResize} onLostPointerCapture={finishResize}
    onPointerCancel={finishResize}
    onKeyDown={event => {
      if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
      event.preventDefault();
      commit(resized(fractionsRef.current, index, event.key === 'ArrowRight' ? 16 : -16));
    }}/>;
  const panesStyle = { '--operator-columns': columns(fractions), gridTemplateColumns: 'var(--operator-columns)' } as CSSProperties;
  return { panesRef, panesStyle, resizeDivider };
}
