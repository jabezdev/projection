import { useEffect, useRef, useState, type CSSProperties } from 'react';

/** The operator's three resizable columns (30/45/35 at first), with keyboard-operable dividers. */
export function useColumns() {
  const initializedWidths = useRef(false);
  const [columnWidths, setColumnWidths] = useState([300, 450, 350]);
  const widthsRef = useRef(columnWidths);
  const resizeFrame = useRef<number | null>(null);
  useEffect(() => { widthsRef.current = columnWidths; }, [columnWidths]);
  const panesRef = useRef<HTMLDivElement>(null);
  const resizeRef = useRef<{ index: number; x: number; widths: number[] } | null>(null);
  useEffect(() => {
    const element = panesRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setColumnWidths(current => {
      const available = Math.max(0, element.clientWidth - 10);
      const minimum = Math.min(140, available / 3);
      if (!initializedWidths.current) { initializedWidths.current = true; return [available * 30 / 110, available * 45 / 110, available * 35 / 110]; }
      const right = Math.max(minimum, Math.min(current[2], available - minimum * 2));
      const left = Math.max(minimum, Math.min(current[0], available - right - minimum));
      const next = [left, available - left - right, right];
      return next.every((width, index) => width === current[index]) ? current : next;
    }));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  function resizeColumns(widths: number[], index: number, delta: number) {
    const total = widths[index] + widths[index + 1];
    const minimum = Math.min(140, total / 2);
    const left = Math.max(minimum,
      Math.min(total - minimum, widths[index] + delta));
    const next = [...widths];
    next[index] = left; next[index + 1] = total - left;
    return next;
  }
  function finishResize() {
    if (!resizeRef.current) return;
    if (resizeFrame.current !== null) cancelAnimationFrame(resizeFrame.current);
    resizeFrame.current = null;
    panesRef.current?.classList.remove('is-resizing');
    const widths = widthsRef.current;
    panesRef.current?.style.setProperty('--operator-columns', `${widths[0]}px 5px minmax(0, 1fr) 5px ${widths[2]}px`);
    setColumnWidths([...widths]);
    resizeRef.current = null;
  }
  const resizeDivider = (index: number) => <div className="column-divider" role="separator"
    aria-label={index === 0 ? 'Resize session and lyrics columns' : 'Resize lyrics and controls columns'}
    aria-orientation="vertical" aria-valuenow={Math.round(columnWidths[index])} aria-valuemin={140}
    aria-valuemax={Math.round(columnWidths[index] + columnWidths[index + 1] - 140)} tabIndex={0}
    onPointerDown={event => {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      resizeRef.current = { index, x: event.clientX, widths: [...widthsRef.current] };
      panesRef.current?.classList.add('is-resizing');
    }}
    onPointerMove={event => {
      const drag = resizeRef.current;
      if (!drag) return;
      const widths = resizeColumns(drag.widths, index, event.clientX - drag.x);
      widthsRef.current = widths;
      if (resizeFrame.current === null) resizeFrame.current = requestAnimationFrame(() => {
        resizeFrame.current = null;
        const current = widthsRef.current;
        panesRef.current?.style.setProperty('--operator-columns', `${current[0]}px 5px minmax(0, 1fr) 5px ${current[2]}px`);
      });
      // Rebase at the clamped position so reversing direction responds immediately.
      resizeRef.current = { index, x: event.clientX, widths };
    }}
    onPointerUp={finishResize} onLostPointerCapture={finishResize}
    onPointerCancel={finishResize}
    onKeyDown={event => {
      if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
      event.preventDefault();
      setColumnWidths(current => resizeColumns(current, index, event.key === 'ArrowRight' ? 16 : -16));
    }}/>;
  const panesStyle = { '--operator-columns': `${columnWidths[0]}px 5px minmax(0, 1fr) 5px ${columnWidths[2]}px`, gridTemplateColumns: 'var(--operator-columns)' } as CSSProperties;
  return { panesRef, panesStyle, resizeDivider };
}
