import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, listen } from './tauri';
import { startOfToday, type LiveService, type MediaFile, type Service } from './models';
import type { PlaybackState } from './playback';

export type DisplayMode = 'logo' | 'black' | 'clear' | 'live';
export type Background = { kind: 'image' | 'video' | 'shader'; path: string };
export type ProgramState = {
  display?: DisplayMode;
  mode: 'manual' | 'automated' | 'external';
  active_item_id: string | null;
  next_item_id: string | null;
  active_step_index: number | null;
  active_text: string;
  background: Background | null;
};
type EngineSnapshot = { program_state: ProgramState; version: number; service: LiveService | null; playback?: PlaybackState | null };

type EngineEvent =
  | { type: 'service_changed'; payload: { service: LiveService | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: ProgramState; version: number } }
  | { type: 'playback_changed'; payload: { playback: PlaybackState | null; revision: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

export function useEngine() {
  const [live, setLive] = useState<LiveService | null>(null);
  const [services, setServices] = useState<Service[]>([]);
  const [program, setProgram] = useState<ProgramState | null>(null);
  const [playback, setPlayback] = useState<PlaybackState | null>(null);
  const [media, setMedia] = useState<MediaFile[]>([]);
  const [error, setError] = useState('');
  const saveQueue = useRef(Promise.resolve());
  const serviceRef = useRef<Service | null>(null);
  const pendingSaves = useRef(0);
  const run = useCallback(async (work: () => Promise<unknown>) => { setError(''); try { await work(); } catch (e) { setError(String(e)); } }, [setError]);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    (async () => {
      const unlisten = await listen<EngineEvent>('engine:event', ({ payload }) => {
        if (disposed) return;
        if (payload.type === 'service_changed') {
          serviceRef.current = payload.payload.service?.service ?? null;
          if (!pendingSaves.current) setLive(payload.payload.service);
        }
        if (payload.type === 'program_changed') setProgram(payload.payload.program_state);
        if (payload.type === 'playback_changed') setPlayback(payload.payload.playback);
        if (payload.type === 'error') setError(payload.payload.message);
      });
      if (disposed) { unlisten(); return; }
      cleanup = unlisten;
      await invoke('freeze_past_services', { todayStartsAtMs: startOfToday() });
      const [snapshot, list, assets] = await Promise.all([invoke<EngineSnapshot>('get_engine_snapshot'), invoke<Service[]>('list_services'), invoke<MediaFile[]>('list_media')]);
      if (disposed) return;
      setProgram(snapshot.program_state); setLive(snapshot.service); serviceRef.current = snapshot.service?.service ?? null; setPlayback(snapshot.playback ?? null);
      setServices(list); setMedia(assets);
      const warnings = await invoke<string[] | undefined>('get_startup_warnings');
      if (!disposed && warnings?.length) setError(warnings.join(' '));
    })().catch(e => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; cleanup?.(); };
  }, []);
  const persist = useCallback((updated: Service) => {
    serviceRef.current = updated; pendingSaves.current++;
    setServices(old => old.map(s => s.id === updated.id ? updated : s));
    const task = saveQueue.current.then(() => invoke('save_service', { service: updated })).then(() => {});
    saveQueue.current = task.catch(e => setError(String(e))).finally(() => {
      pendingSaves.current--;
      if (!pendingSaves.current) void invoke<EngineSnapshot>('get_engine_snapshot').then(s => setLive(s.service));
    });
    return task;
  }, [setError]);
  const adopt = useCallback((updated: Service) => {
    if (serviceRef.current?.id === updated.id) serviceRef.current = updated;
    setServices(old => old.map(s => s.id === updated.id ? updated : s));
    void saveQueue.current.then(() => invoke<EngineSnapshot>('get_engine_snapshot')).then(s => { if (!pendingSaves.current) setLive(s.service); });
  }, []);
  return { live, serviceRef, services, setServices, program, playback, media, setMedia, error, setError, run, persist, adopt, saveQueue };
}
