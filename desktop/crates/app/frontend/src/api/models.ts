export type VideoOptions = { confidence: boolean; looping: boolean; start_ms: number; end_ms: number | null; volume: number; rate_percent: number };
export const defaultVideoOptions: VideoOptions = { confidence: false, looping: false, start_ms: 0, end_ms: null, volume: 100, rate_percent: 100 };
export type CaptureKind = 'screen' | 'window';
export type CaptureSource = { kind: CaptureKind; id: string; name: string };
export type CaptureFit = 'fit' | 'fill';
export type CaptureOptions = { projection: boolean; confidence: boolean; projection_fit: CaptureFit; confidence_fit: CaptureFit };
export const defaultCaptureOptions: CaptureOptions = { projection: true, confidence: false, projection_fit: 'fit', confidence_fit: 'fit' };

export type MediaKind = 'image' | 'video' | 'audio' | 'presentation';
export type MediaFile = { id: string; title: string; kind: MediaKind; hash?: string; path?: string };

export type SectionKind = 'verse' | 'pre_chorus' | 'chorus' | 'bridge' | 'intro' | 'outro' | 'tag' | 'other';
export type Section = { id: string; kind: SectionKind; number?: number; name?: string; color?: string; lyrics: string };
export type SectionPatch = Partial<Omit<Section, 'id'>>;
export type AuthorRole = 'words' | 'music' | 'translation';
export type Author = { name: string; role?: AuthorRole };
export type Credits = { authors?: Author[]; copyright?: string; ccli?: string; publisher?: string; year?: number };
export type SongVersion = { id: string; number: number; sections: Section[]; arrangement: string[]; background?: string };
export type Song = { id: string; title: string; archived: boolean; credits: Credits; key?: string; tags: string[]; current: SongVersion };

export type Entry = { id: string; sectionId?: string; variation?: SectionPatch };
export type SongItem = { songId: string; versionId?: string; key?: string; arrangement?: Entry[] };
export type ItemContent =
  | ({ kind: 'song' } & SongItem)
  | { kind: 'text'; steps: string[] }
  | { kind: 'media'; mediaId: string; options: VideoOptions }
  | { kind: 'presentation'; mediaId?: string; slides: string[] }
  | { kind: 'capture'; options: CaptureOptions; source?: CaptureSource }
  | { kind: 'header' };
export type ServiceItem = { id: string; title?: string; notes?: string; background?: string; content: ItemContent };
export type Service = { id: string; scheduledAtMs: number; title: string; leader?: string; notes?: string; endedAtMs?: number; items: ServiceItem[] };

export type LiveSong = { song_id: string; version_id: string; sections: Section[]; title?: string; credits?: Credits };
export type SessionItemKind =
  | { kind: 'text'; data: { steps: string[] } }
  | { kind: 'image'; data: { path: string } }
  | { kind: 'video' | 'audio'; data: { path: string; options?: VideoOptions } }
  | { kind: 'presentation'; data: { path: string; slides?: string[] } }
  | { kind: 'capture'; data: { source: CaptureSource; options?: CaptureOptions } }
  | { kind: 'section'; data: Record<string, never> };
export type SessionItem = { id: string; title: string | null; kind: SessionItemKind; song?: LiveSong; background?: string };
export type LiveService = { service: Service; items: SessionItem[] };

const ALPHABET = '0123456789abcdefghjkmnpqrstvwxyz';
export function newId() {
  let time = Date.now();
  let out = '';
  for (let i = 0; i < 10; i++) { out = ALPHABET[time % 32] + out; time = Math.floor(time / 32); }
  const random = crypto.getRandomValues(new Uint8Array(13));
  for (const byte of random) out += ALPHABET[byte % 32];
  return out;
}

export const startOfToday = () => { const d = new Date(); d.setHours(0, 0, 0, 0); return d.getTime(); };
export const isPast = (service: Service, todayStartsAt = startOfToday()) => service.endedAtMs != null || service.scheduledAtMs < todayStartsAt;

export type TemplatePart = { id: string; title: string; optional?: boolean; notes?: string; items: ServiceItem[] };
export type ServiceTemplate = { id: string; name: string; title?: string; weekday?: number; startMinutes?: number; parts: TemplatePart[] };

export type RecordKind = 'song' | 'service' | 'template' | 'media';
export type SyncState = 'off' | 'idle' | 'syncing' | 'offline' | 'error';
export type SyncStatus = { paired: boolean; deployment?: string; deviceName?: string; offline: boolean; state: SyncState; lastSyncedMs?: number; error?: string; pending: number; conflicts: number; uploads: number; downloads: number; liveService?: string };
export type Conflict = { id: string; kind: RecordKind; recordId: string; title: string; label: string; mine: unknown; theirs: unknown; keepMine: unknown; atMs: number };
