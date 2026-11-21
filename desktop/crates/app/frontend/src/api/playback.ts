import { useEffect, useState } from 'react';
import { invoke, listen } from './tauri';

// Mirrors protocol::PlaybackState: the engine-owned foreground media transport.
export type PlaybackPhase = 'preparing' | 'ready' | 'playing' | 'paused' | 'ended' | 'failed';
export type PlaybackState = {
  session_id: number; generation: number; item_id: number; path: string; kind: 'video' | 'audio';
  paused: boolean; suspended: boolean; phase: PlaybackPhase;
  position_ms: number; duration_ms: number | null; position_at_ms: number;
  volume: number; muted: boolean; rate_percent: number; looping: boolean;
  start_ms: number; end_ms: number | null; confidence: boolean; error: string | null;
};
export type MediaAction =
  | { type: 'play' } | { type: 'pause' } | { type: 'stop' }
  | { type: 'seek'; payload: { position_ms: number } }
  | { type: 'set_muted'; payload: { muted: boolean } }
  | { type: 'set_volume'; payload: { volume: number } };
export type MediaDirective =
  | { type: 'open'; playback: PlaybackState }
  | { type: 'play' | 'pause' | 'close'; session_id: number }
  | { type: 'seek'; session_id: number; generation: number; position_ms: number }
  | { type: 'configure'; playback: PlaybackState };
export type PlaybackReport = { session_id: number; generation: number; phase: PlaybackPhase; position_ms: number; duration_ms: number | null; error: string | null };
type PlaybackEvent = { type: string; payload: { playback?: PlaybackState | null; revision?: number } };

export const running = (playback: PlaybackState) => !playback.paused && !playback.suspended;

/** Addressed to one session, so a late click cannot control a newer cue. */
export function sendMedia(sessionId: number, action: MediaAction) {
  return invoke('engine_command', { command: { type: 'media', payload: { session_id: sessionId, action } } });
}

/** The authoritative playback state, kept current from engine events. */
export function usePlayback(): PlaybackState | null {
  const [state, setState] = useState<{ playback: PlaybackState | null; revision: number }>({ playback: null, revision: -1 });
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    // Revisions only grow; anything older than what we hold is stale.
    const accept = (playback: PlaybackState | null, revision: number) => setState(current => revision > current.revision ? { playback, revision } : current);
    void (async () => {
      const unlisten = await listen<PlaybackEvent>('engine:event', ({ payload }) => {
        if (!disposed && payload.type === 'playback_changed') accept(payload.payload.playback ?? null, payload.payload.revision ?? 0);
      });
      if (disposed) { unlisten(); return; }
      cleanup = unlisten;
      const snapshot = await invoke<{ playback?: PlaybackState | null; playback_revision?: number }>('get_engine_snapshot');
      if (!disposed && snapshot) accept(snapshot.playback ?? null, snapshot.playback_revision ?? 0);
    })().catch(console.error);
    return () => { disposed = true; cleanup?.(); };
  }, []);
  return state.playback;
}
