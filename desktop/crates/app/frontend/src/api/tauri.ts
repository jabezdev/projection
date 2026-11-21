import { invoke as nativeInvoke } from '@tauri-apps/api/core';
import { uiOnlyEngine } from '../demo/engine';
import { uiOnlyEvents } from '../demo/events';
import type { UiOnlyListener } from '../demo/types';
export { startDemo } from '../demo/store';
export type UnlistenFn = () => void;

// Minimal event shape used by our UI.
export type TauriEvent<T> = {
  payload: T;
};

export function isTauri(): boolean {
  if (typeof window === 'undefined') return false;
  const w = window as unknown as Record<string, unknown>;
  // Tauri injects globals; exact shape differs a bit between versions.
  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (!isTauri()) {
    return uiOnlyEngine().invoke<T>(command, args);
  }

  return nativeInvoke<T>(command, args);
}

/** Sends `bytes` as the raw request body (desktop app only), with string headers. */
export async function invokeBytes<T = unknown>(command: string, bytes: Uint8Array, headers: Record<string, string> = {}): Promise<T> {
  return nativeInvoke<T>(command, bytes, { headers });
}

export async function listen<T>(
  event: string,
  handler: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  if (!isTauri()) {
    if (event !== 'engine:event') {
      const relay = (e: Event) => handler({ payload: (e as CustomEvent<T>).detail });
      uiOnlyEvents.addEventListener(event, relay);
      return () => uiOnlyEvents.removeEventListener(event, relay);
    }

    return uiOnlyEngine().listen(handler as unknown as UiOnlyListener);
  }

  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(event, handler);
}
