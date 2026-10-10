export type VideoOptions = { confidence: boolean; looping: boolean; start_ms: number; end_ms: number | null; volume: number; rate_percent: number };
export const defaultVideoOptions: VideoOptions = { confidence: false, looping: false, start_ms: 0, end_ms: null, volume: 100, rate_percent: 100 };
export type MediaKind = 'image' | 'video' | 'audio';
export type Media = { id: number; title: string; kind: MediaKind; path: string };
export type SessionItemKind =
  | { kind: 'text'; data: { steps: string[] } }
  | { kind: MediaKind; data: { path: string; options?: VideoOptions } }
  | { kind: 'presentation'; data: { path: string; slides?: string[] } };
export type SessionItem = { id: number; title: string | null; kind: SessionItemKind; song?: string; background?: string };
export type SongHeader = { id: number; title: string };
export type SongModel = SongHeader & { lyrics: string };
export type ProjectModel = { id: number; name: string; session: { items: SessionItem[] } };
let lastItemId = 0;
export function newItemId() { lastItemId = Math.max(lastItemId + 1, Date.now() * 1000); return lastItemId; }
