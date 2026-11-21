import type { TauriEvent } from '../api/tauri';
import type { Media } from '../api/models';

export type UiOnlyBackgroundKind = 'image' | 'video' | 'shader';
export type UiOnlyBackground = { kind: UiOnlyBackgroundKind; path: string };

export type UiOnlyProgramState = {
  display?: 'logo' | 'black' | 'clear' | 'live';
  mode: 'manual' | 'automated' | 'external';
  active_item_id: number | null;
  next_item_id: number | null;
  active_step_index: number | null;
  active_text: string;
  background: UiOnlyBackground | null;
};

export type UiOnlySessionItemKind =
  | { kind: 'text'; data: { steps: string[] } }
  | { kind: 'image'; data: { path: string } }
  | { kind: 'video'; data: { path: string; options?: Record<string, unknown> } }
  | { kind: 'audio'; data: { path: string; options?: Record<string, unknown> } }
  | { kind: 'presentation'; data: { path: string; slides?: string[] } }
  | { kind: 'capture'; data: Record<string, unknown> }
  | { kind: 'section'; data: Record<string, never> };

export type UiOnlySessionItem = {
  id: number;
  title: string | null;
  kind: UiOnlySessionItemKind;
  song?: string;
  background?: string;
};

export type UiOnlyProjectModel = {
  id: number;
  name: string;
  session: { items: UiOnlySessionItem[] };
};

export type UiOnlyProjectHeader = { id: number; name: string; updated_at_ms: number };

export type UiOnlyEngineSnapshot = {
  program_state: UiOnlyProgramState;
  version: number;
  project: UiOnlyProjectModel | null;
};

export type UiOnlyEngineEvent =
  | { type: 'project_changed'; payload: { project: UiOnlyProjectModel | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: UiOnlyProgramState; version: number } }
  | { type: 'playback_changed'; payload: { playback: UiOnlyPlayback | null; revision: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

/** Browser-only mode: a simulated media session, so the transport can be laid out. */
export type UiOnlyPlayback = {
  session_id: number; generation: number; item_id: number; path: string; kind: 'video' | 'audio';
  paused: boolean; suspended: boolean; phase: 'playing' | 'paused' | 'ended';
  position_ms: number; duration_ms: number | null; position_at_ms: number;
  volume: number; muted: boolean; rate_percent: number; looping: boolean;
  start_ms: number; end_ms: number | null; confidence: boolean; error: string | null;
};

export type UiOnlyListener = (evt: TauriEvent<UiOnlyEngineEvent>) => void;

export type UiOnlyPair = [number, number];
export type UiOnlyOutput = { monitor: number | null; shown: boolean; fullscreen: boolean; size: UiOnlyPair | null; aspect: UiOnlyPair | null };

export type UiOnlyStore = {
  media?: Media[];
  songs?: Record<string, { id: number; title: string; lyrics: string }>;
  version: number;
  active_project_id: number | null;
  program_state: UiOnlyProgramState;
  projects: Record<string, UiOnlyProjectModel>;
  project_updated_at: Record<string, number>;
};
