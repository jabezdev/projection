import { useCallback, useRef, useState, type ReactNode } from 'react';

type Request = { message: string; action: string; resolve: (ok: boolean) => void };

/** `confirm(message, action)` resolves true when the operator confirms; render `dialog` once. */
export function useConfirm(): [ReactNode, (message: string, action: string) => Promise<boolean>] {
  const [request, setRequest] = useState<Request | null>(null);
  const requestRef = useRef<Request | null>(null);
  const confirm = useCallback((message: string, action: string) => new Promise<boolean>(resolve => {
    requestRef.current?.resolve(false);
    const next = { message, action, resolve };
    requestRef.current = next;
    setRequest(next);
  }), []);
  const answer = (ok: boolean) => { request?.resolve(ok); requestRef.current = null; setRequest(null); };
  const dialog = request && <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') answer(false); }}>
    <section className="compact-modal confirm-modal" role="alertdialog" aria-modal="true" aria-label="Confirm">
      <p>{request.message}</p>
      <div className="modal-actions"><button autoFocus onClick={() => answer(false)}>Cancel</button><button className="primary-action" onClick={() => answer(true)}>{request.action}</button></div>
    </section>
  </div>;
  return [dialog, confirm];
}
