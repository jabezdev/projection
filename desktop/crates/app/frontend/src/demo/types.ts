import type { TauriEvent } from '../api/tauri';
import type { LiveService, MediaFile, Service, ServiceTemplate, Song, SongVersion } from '../api/models';
import type { ProgramState } from '../api/useEngine';

export type UiOnlyProgramState = ProgramState;

export type UiOnlyEngineSnapshot = {
  program_state: UiOnlyProgramState;
  version: number;
  service: LiveService | null;
};

export type UiOnlyEngineEvent =
  | { type: 'service_changed'; payload: { service: LiveService | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: UiOnlyProgramState; version: number } }
  | { type: 'playback_changed'; payload: { playback: UiOnlyPlayback | null; revision: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

export type UiOnlyPlayback = {
  session_id: number; generation: number; item_id: string; path: string; kind: 'video' | 'audio';
  paused: boolean; suspended: boolean; phase: 'playing' | 'paused' | 'ended';
  position_ms: number; duration_ms: number | null; position_at_ms: number;
  volume: number; muted: boolean; rate_percent: number; looping: boolean;
  start_ms: number; end_ms: number | null; confidence: boolean; error: string | null;
};

export type UiOnlyListener = (evt: TauriEvent<UiOnlyEngineEvent>) => void;

export type UiOnlyPair = [number, number];
export type UiOnlyOutput = { monitor: number | null; shown: boolean; fullscreen: boolean; size: UiOnlyPair | null; aspect: UiOnlyPair | null };

export type UiOnlyStore = {
  media: MediaFile[];
  songs: Record<string, Song>;
  versions: Record<string, SongVersion>;
  version: number;
  active_service_id: string | null;
  program_state: UiOnlyProgramState;
  services: Record<string, Service>;
  frozen: string[];
  templates?: Record<string, ServiceTemplate>;
};
