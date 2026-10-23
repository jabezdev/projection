import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, listen } from './tauri';
import type { Media, ProjectModel } from './models';
import type { PlaybackState } from './playback';

export type DisplayMode = 'logo' | 'black' | 'clear' | 'live';
export type Background = { kind: 'image' | 'video'; path: string };
export type ProgramState = {
  display?: DisplayMode;
  mode: 'manual' | 'automated' | 'external';
  active_item_id: number | null;
  next_item_id: number | null;
  active_step_index: number | null;
  active_text: string;
  background: Background | null;
};
export type ProjectHeader = { id: number; name: string; updated_at_ms: number };
type EngineSnapshot = { program_state: ProgramState; version: number; project: ProjectModel | null; playback?: PlaybackState | null };

// Each state event carries the complete state of one domain; replace, don't merge.
type EngineEvent =
  | { type: 'project_changed'; payload: { project: ProjectModel | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: ProgramState; version: number } }
  | { type: 'playback_changed'; payload: { playback: PlaybackState | null; revision: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

/**
 * The operator's view of the engine: the loaded service, what is on air,
 * the service list and media library, and an ordered save queue. The UI
 * shows what the engine reports; local edits are saved through `persist`.
 */
export function useEngine() {
  const [project, setProject] = useState<ProjectModel | null>(null);
  const [projects, setProjects] = useState<ProjectHeader[]>([]);
  const [program, setProgram] = useState<ProgramState | null>(null);
  const [playback, setPlayback] = useState<PlaybackState | null>(null);
  const [media, setMedia] = useState<Media[]>([]);
  const [error, setError] = useState('');
  const saveQueue = useRef(Promise.resolve());
  const projectRef = useRef(project);
  const pendingSaves = useRef(0);
  const run = useCallback(async (work: () => Promise<unknown>) => { setError(''); try { await work(); } catch (e) { setError(String(e)); } }, [setError]);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    (async () => {
      const unlisten = await listen<EngineEvent>('engine:event', ({ payload }) => {
        if (disposed) return;
        if (payload.type === 'project_changed' && !pendingSaves.current) { setProject(payload.payload.project); projectRef.current = payload.payload.project; }
        if (payload.type === 'program_changed') setProgram(payload.payload.program_state);
        if (payload.type === 'playback_changed') setPlayback(payload.payload.playback);
        if (payload.type === 'error') setError(payload.payload.message);
      });
      if (disposed) { unlisten(); return; }
      cleanup = unlisten;
      const [snapshot, headers, assets] = await Promise.all([invoke<EngineSnapshot>('get_engine_snapshot'), invoke<ProjectHeader[]>('list_projects'), invoke<Media[]>('list_media')]);
      if (disposed) return;
      setProgram(snapshot.program_state); setProject(snapshot.project); projectRef.current = snapshot.project; setPlayback(snapshot.playback ?? null);
      setProjects(headers); setMedia(assets);
      const warnings = await invoke<string[] | undefined>('get_startup_warnings');
      if (!disposed && warnings?.length) setError(warnings.join(' '));
    })().catch(e => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; cleanup?.(); };
  }, []);
  const persist = useCallback((updated: ProjectModel) => {
    projectRef.current = updated; setProject(updated); pendingSaves.current++;
    const task = saveQueue.current.then(() => invoke('save_project', { project: updated })).then(() => {});
    saveQueue.current = task.catch(e => setError(String(e))).finally(() => { pendingSaves.current--; });
    return task;
  }, [setError]);
  return { project, projectRef, projects, setProjects, program, playback, media, setMedia, error, setError, run, persist, saveQueue };
}
