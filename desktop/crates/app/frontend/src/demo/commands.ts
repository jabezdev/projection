import type { UiOnlyPair, UiOnlyProgramState } from './types';
import { saveUiOnlyStore } from './store';
import { emitUiOnly } from './events';
import { UI_ONLY_MONITORS, UiOnlyEngine } from './engine';
import { isPast, newId, type MediaFile, type Service, type ServiceItem, type Song } from '../api/models';
import { dropHistory, historyOf, recordService, recordSong, stepService, stepSong } from './history';

function freeze(engine: UiOnlyEngine, service: Service): Service {
  dropHistory(service.id);
  if (!engine.store.frozen.includes(service.id)) engine.store.frozen.push(service.id);
  return { ...service, items: service.items.map(item => item.content.kind === 'song' && !item.content.versionId ? { ...item, content: { ...item.content, versionId: engine.store.songs[item.content.songId]?.current.id } } : item) };
}

function store(engine: UiOnlyEngine, service: Service) {
  const frozen = engine.store.frozen.includes(service.id) ? freeze(engine, service) : service;
  engine.store.services[service.id] = frozen;
  return frozen;
}

function writeSong(engine: UiOnlyEngine, song: Song, track = true): Song {
  const old = engine.store.songs[song.id];
  if (!old) throw new Error('Song not found');
  let current = { ...song.current, id: old.current.id, number: old.current.number };
  const sameContent = JSON.stringify([current.sections, current.arrangement, current.background]) === JSON.stringify([old.current.sections, old.current.arrangement, old.current.background]);
  if (!sameContent && pinned(engine, old.current.id)) { engine.store.versions[old.current.id] = old.current; current = { ...current, id: newId(), number: old.current.number + 1 }; }
  engine.store.songs[song.id] = { ...song, current };
  if (track) recordSong(old, engine.store.songs[song.id]);
  return engine.store.songs[song.id];
}

function itemName(engine: UiOnlyEngine, items: ServiceItem[]) {
  return (id: string) => {
    const item = items.find(i => i.id === id);
    if (!item) return 'an item';
    if (item.title?.trim()) return item.title.trim();
    const c = item.content;
    if (c.kind === 'song') return engine.store.songs[c.songId]?.title ?? 'a song';
    if (c.kind === 'media') return engine.store.media.find(m => m.id === c.mediaId)?.title ?? 'a media item';
    if (c.kind === 'text') return c.steps[0]?.split('\n')[0] || 'a text item';
    return `a ${c.kind}`;
  };
}

function pinned(engine: UiOnlyEngine, versionId: string) {
  return Object.values(engine.store.services).some(s => s.items.some(i => i.content.kind === 'song' && i.content.versionId === versionId));
}

function refresh(engine: UiOnlyEngine) {
  engine.bumpVersion();
  saveUiOnlyStore(engine.store);
  const step = engine.store.program_state.active_step_index;
  engine.setActiveItem(engine.store.program_state.active_item_id);
  const item = engine.items().find(i => i.id === engine.store.program_state.active_item_id);
  if (item?.kind.kind === 'presentation' && step != null) { const paths = item.kind.data.slides || [item.kind.data.path]; const index = Math.min(step, paths.length - 1); engine.store.program_state.active_step_index = index; engine.store.program_state.background = { kind: 'image', path: paths[index] }; }
  if (item?.kind.kind === 'text' && step != null && item.kind.data.steps.length) {
    const index = Math.min(step, item.kind.data.steps.length - 1);
    engine.store.program_state.active_step_index = index;
    engine.store.program_state.active_text = item.kind.data.steps[index];
  }
  engine.persistAndBroadcast();
}

export async function runDemoCommand<T = unknown>(engine: UiOnlyEngine, command: string, args?: Record<string, unknown>): Promise<T> {
  const now = Date.now();
  const id = String(args?.id ?? '');

  try {
    switch (command) {
      case 'list_media': return engine.store.media as T;
      case 'create_media': {
        const media: MediaFile = { id: newId(), title: String(args?.title || ''), kind: args?.kind as MediaFile['kind'], path: String(args?.path || '') };
        engine.store.media = [...engine.store.media, media]; saveUiOnlyStore(engine.store); return media as T;
      }
      case 'delete_media': engine.store.media = engine.store.media.filter(m => m.id !== id); saveUiOnlyStore(engine.store); return undefined as T;
      case 'rename_media': {
        const title = String(args?.title ?? '').trim();
        if (!title) throw new Error('Enter a title.');
        engine.store.media = engine.store.media.map(m => m.id === id ? { ...m, title } : m);
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'import_media_files': return [] as T;

      case 'list_songs': return Object.values(engine.store.songs).filter(s => args?.includeArchived || !s.archived).sort((a, b) => a.title.localeCompare(b.title)) as T;
      case 'get_song': {
        const song = engine.store.songs[id];
        if (!song) throw new Error('Song not found');
        return structuredClone(song) as T;
      }
      case 'get_song_version': {
        const version = engine.store.versions[id] ?? Object.values(engine.store.songs).find(s => s.current.id === id)?.current;
        if (!version) throw new Error('Song version not found');
        return structuredClone(version) as T;
      }
      case 'create_song': {
        const draft = args?.song as Song;
        const song: Song = { ...draft, id: newId(), current: { ...draft.current, id: newId(), number: 1 } };
        engine.store.songs[song.id] = song;
        saveUiOnlyStore(engine.store);
        return song as T;
      }
      case 'update_song': {
        const song = writeSong(engine, args?.song as Song);
        refresh(engine);
        return song as T;
      }
      case 'archive_song': {
        const song = engine.store.songs[id];
        if (song) writeSong(engine, { ...song, archived: Boolean(args?.archived) });
        refresh(engine); return undefined as T;
      }

      case 'list_services': return Object.values(engine.store.services).sort((a, b) => b.scheduledAtMs - a.scheduledAtMs) as T;
      case 'get_service': {
        const service = engine.store.services[String(args?.serviceId)];
        if (!service) throw new Error('Service not found');
        return structuredClone(service) as T;
      }
      case 'create_service': {
        const service: Service = { id: newId(), title: String(args?.title || '').trim() || 'Service', scheduledAtMs: Number(args?.scheduledAtMs ?? now), items: [] };
        engine.store.services[service.id] = service;
        saveUiOnlyStore(engine.store);
        return service as T;
      }
      case 'save_service': {
        const incoming = args?.service as Service;
        const old = engine.store.services[incoming.id];
        const service = store(engine, incoming);
        if (old && !engine.store.frozen.includes(service.id)) recordService(old, service, itemName(engine, [...service.items, ...old.items]));
        if (engine.store.active_service_id === service.id) refresh(engine); else saveUiOnlyStore(engine.store);
        return undefined as T;
      }
      case 'edit_history': return historyOf(id) as T;
      case 'undo_edit':
      case 'redo_edit': {
        const redo = command === 'redo_edit';
        if (args?.record === 'song') {
          const song = engine.store.songs[id];
          const next = song && stepSong(song, redo);
          const saved = next ? writeSong(engine, next, false) : undefined;
          if (saved) refresh(engine);
          return { song: saved, history: historyOf(id) } as T;
        }
        const service = engine.store.services[id];
        const next = service && !engine.store.frozen.includes(id) ? stepService(service, redo) : null;
        if (next) { engine.store.services[id] = next; if (engine.store.active_service_id === id) refresh(engine); else saveUiOnlyStore(engine.store); }
        return { service: next ?? undefined, history: historyOf(id) } as T;
      }
      case 'end_service': {
        const service = engine.store.services[String(args?.serviceId)];
        if (service) engine.store.services[service.id] = freeze(engine, { ...service, endedAtMs: service.endedAtMs ?? now });
        refresh(engine); return undefined as T;
      }
      case 'freeze_past_services': {
        const cutoff = Number(args?.todayStartsAtMs ?? now);
        const due = Object.values(engine.store.services).filter(s => !engine.store.frozen.includes(s.id) && isPast(s, cutoff));
        for (const service of due) engine.store.services[service.id] = freeze(engine, service);
        if (due.length) refresh(engine);
        return due.length as T;
      }
      case 'delete_service': {
        const serviceId = String(args?.serviceId);
        if (engine.store.active_service_id === serviceId) throw new Error('This service is loaded. Load another service before deleting it.');
        delete engine.store.services[serviceId];
        dropHistory(serviceId);
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'load_service': {
        engine.store.active_service_id = String(args?.serviceId);
        engine.setActiveItem(engine.items()[0]?.id ?? null);
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }

      case 'engine_command': {
        const cmd = args?.command as { type: string; payload?: { action?: { type: string; payload?: Record<string, number | boolean> } } } | undefined;
        if (cmd?.type === 'media' && cmd.payload?.action) engine.mediaAction(cmd.payload.action);
        return undefined as T;
      }
      case 'get_media_autoplay': return engine.autoplay as T;
      case 'set_media_autoplay': engine.autoplay = Boolean(args?.enabled); return undefined as T;
      case 'set_display': {
        const display = args?.display as UiOnlyProgramState['display'];
        if (!display || !['logo', 'black', 'clear', 'live'].includes(display)) throw new Error('Invalid display mode');
        engine.store.program_state.display = display;
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }
      case 'get_engine_snapshot': return engine.snapshot() as T;

      case 'cue': {
        const itemId = String(args?.itemId);
        engine.setActiveItem(itemId);
        const item = engine.items().find(item => item.id === itemId);
        const index = Number(args?.index ?? 0);
        if (item?.kind.kind === 'text' && index < item.kind.data.steps.length) { engine.store.program_state.active_step_index = index; engine.store.program_state.active_text = item.kind.data.steps[index]; }
        if (item?.kind.kind === 'presentation') { const paths = item.kind.data.slides || [item.kind.data.path]; if (paths[index]) { engine.store.program_state.active_step_index = index; engine.store.program_state.background = { kind: 'image', path: paths[index] }; } }
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }
      case 'go_to_item': {
        engine.setActiveItem(String(args?.itemId));
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }
      case 'go_to_step': {
        const index = Number(args?.index);
        if (!Number.isFinite(index)) throw new Error('go_to_step requires { index }');
        const activeId = engine.store.program_state.active_item_id;
        const item = engine.items().find(it => it.id === activeId) ?? null;
        if (item?.kind.kind === 'presentation') {
          const images = item.kind.data.slides || [item.kind.data.path];
          const clamped = Math.max(0, Math.min(index, images.length - 1));
          engine.store.program_state.active_step_index = clamped;
          engine.store.program_state.background = { kind: 'image', path: images[clamped] };
        }
        if (item?.kind.kind === 'text') {
          const steps = item.kind.data.steps;
          const clamped = Math.max(0, Math.min(index, Math.max(0, steps.length - 1)));
          engine.store.program_state.active_step_index = steps.length > 0 ? clamped : null;
          engine.store.program_state.active_text = steps[clamped] || '';
        }
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }
      case 'nav_next':
      case 'nav_prev': {
        const dir = command === 'nav_next' ? 1 : -1;
        const items = engine.items();
        if (!items.length) return undefined as T;
        const activeId = engine.store.program_state.active_item_id;
        if (activeId == null) engine.setActiveItem(items[0].id);
        else {
          const activeIndex = items.findIndex(it => it.id === activeId);
          const item = activeIndex >= 0 ? items[activeIndex] : null;
          const steps = item?.kind.kind === 'text' ? item.kind.data.steps : [];
          const current = engine.store.program_state.active_step_index ?? 0;
          if (steps.length && current + dir >= 0 && current + dir < steps.length) {
            engine.store.program_state.active_step_index = current + dir;
            engine.store.program_state.active_text = steps[current + dir];
          } else if (activeIndex >= 0) {
            let target = activeIndex + dir;
            while (target >= 0 && target < items.length && items[target].kind.kind === 'section') target += dir;
            if (target >= 0 && target < items.length) engine.setActiveItem(items[target].id);
          }
        }
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }

      case 'get_output_displays': return engine.displays() as T;
      case 'set_output_display': {
        const output = String(args?.output);
        if (!(output in engine.screens)) throw new Error('Unknown output');
        const monitor = Number(args?.monitor);
        if (UI_ONLY_MONITORS[monitor]?.key !== args?.key) throw new Error('The connected displays changed. Choose the display again.');
        engine.screens[output] = {
          monitor,
          shown: engine.screens[output].shown,
          fullscreen: Boolean(args?.fullscreen),
          size: (args?.size as UiOnlyPair | null) ?? null,
          aspect: (args?.aspect as UiOnlyPair | null) ?? null,
        };
        return engine.displays() as T;
      }
      case 'set_output_shown': {
        const output = String(args?.output);
        if (!(output in engine.screens)) throw new Error('Unknown output');
        engine.screens[output] = { ...engine.screens[output], shown: Boolean(args?.shown) };
        emitUiOnly('output:shown', Object.values(engine.screens).some(screen => screen.shown));
        return engine.displays() as T;
      }

      default: {
        console.debug('[ui-only] unhandled invoke', command, args);
        return undefined as T;
      }
    }
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    console.warn('[ui-only] invoke error', command, message);
    engine.emit({
      type: 'error',
      payload: { source: command, message, recoverable: true },
    });
    throw e;
  }
}
