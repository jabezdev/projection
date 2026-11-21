import type { UiOnlyPair, UiOnlyProgramState, UiOnlyProjectHeader, UiOnlyProjectModel } from './types';
import { saveUiOnlyStore } from './store';
import { emitUiOnly } from './events';
import { UI_ONLY_MONITORS, UiOnlyEngine } from './engine';
import type { Media } from '../api/models';

export async function runDemoCommand<T = unknown>(engine: UiOnlyEngine, command: string, args?: Record<string, unknown>): Promise<T> {
  const now = Date.now();

  try {
    switch (command) {
      case 'list_media': return (engine.store.media || []) as T;
      case 'create_media': {
        const media = { id: now, title: String(args?.title || ''), kind: args?.kind, path: String(args?.path || '') } as Media;
        engine.store.media = [...(engine.store.media || []), media]; saveUiOnlyStore(engine.store); return media as T;
      }
      case 'list_songs': return Object.values(engine.store.songs || {}).map(({ id, title }) => ({ id, title })) as T;
      case 'get_song': {
        const song = engine.store.songs?.[String(args?.id)];
        if (!song) throw new Error('Song not found');
        return song as T;
      }
      case 'update_song': {
        const song = args?.song as { id: number; title: string; lyrics: string };
        if (!song || !engine.store.songs?.[song.id]) throw new Error('Song not found');
        engine.store.songs[song.id] = song; saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'create_song': {
        const song = { id: now, title: String(args?.title || ''), lyrics: String(args?.lyrics || '') };
        engine.store.songs = { ...engine.store.songs, [song.id]: song };
        saveUiOnlyStore(engine.store);
        return song as T;
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

      case 'get_engine_snapshot': {
        return engine.snapshot() as T;
      }

      case 'list_projects': {
        const headers: UiOnlyProjectHeader[] = Object.values(engine.store.projects).map((p) => ({
          id: p.id,
          name: p.name,
          updated_at_ms: engine.store.project_updated_at[String(p.id)] || now,
        }));
        headers.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
        return headers as T;
      }

      case 'get_project': {
        const project = engine.store.projects[String(args?.projectId)];
        if (!project) throw new Error('Service not found');
        return structuredClone(project) as T;
      }
      case 'delete_project': {
        const id = Number(args?.projectId);
        if (engine.store.active_project_id === id) throw new Error('This service is loaded. Load another service before deleting it.');
        delete engine.store.projects[String(id)]; delete engine.store.project_updated_at[String(id)];
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'delete_song': {
        if (engine.store.songs) delete engine.store.songs[String(args?.id)];
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'delete_media': {
        engine.store.media = (engine.store.media || []).filter(m => m.id !== Number(args?.id));
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      case 'rename_media': {
        const title = String(args?.title ?? '').trim();
        if (!title) throw new Error('Enter a title.');
        engine.store.media = (engine.store.media || []).map(m => m.id === Number(args?.id) ? { ...m, title } : m);
        saveUiOnlyStore(engine.store); return undefined as T;
      }
      // Browser-only mode cannot read files from disk.
      case 'import_media_files': return [] as T;

      case 'create_project': {
        const name = String(args?.name ?? 'Untitled');
        const id = now + Math.floor(Math.random() * 1000);
        const project: UiOnlyProjectModel = { id, name, session: { items: [] } };
        engine.store.projects[String(id)] = project;
        engine.store.project_updated_at[String(id)] = now;
        engine.bumpVersion();
        saveUiOnlyStore(engine.store);
        return project as T;
      }

      case 'save_project': {
        const project = args?.project as UiOnlyProjectModel | undefined;
        if (!project || typeof project.id !== 'number') {
          throw new Error('save_project requires { project }');
        }
        engine.store.projects[String(project.id)] = project;
        engine.store.project_updated_at[String(project.id)] = now;
        engine.bumpVersion();
        saveUiOnlyStore(engine.store);
        if (engine.store.active_project_id === project.id) {
          const step = engine.store.program_state.active_step_index;
          engine.setActiveItem(engine.store.program_state.active_item_id);
          const item = project.session.items.find(item => item.id === engine.store.program_state.active_item_id);
          if (item?.kind.kind === 'presentation' && step != null) { const paths = item.kind.data.slides || [item.kind.data.path]; const index = Math.min(step, paths.length - 1); engine.store.program_state.active_step_index = index; engine.store.program_state.background = { kind: 'image', path: paths[index] }; }
          if (item?.kind.kind === 'text' && step != null && item.kind.data.steps.length) {
            const index = Math.min(step, item.kind.data.steps.length - 1);
            engine.store.program_state.active_step_index = index;
            engine.store.program_state.active_text = item.kind.data.steps[index];
          }
          engine.persistAndBroadcast();
        }
        return undefined as T;
      }

      case 'load_project': {
        const projectId = Number(args?.projectId);
        if (!Number.isFinite(projectId)) throw new Error('load_project requires { projectId }');
        engine.store.active_project_id = projectId;
        const project = engine.activeProject();
        if (project && project.session.items.length > 0) {
          engine.setActiveItem(project.session.items[0].id);
        } else {
          engine.setActiveItem(null);
        }
        engine.bumpVersion();
        engine.persistAndBroadcast();
        return undefined as T;
      }

      case 'cue': {
        engine.setActiveItem(Number(args?.itemId));
        const item = engine.activeProject()?.session.items.find(item => item.id === Number(args?.itemId));
        const index = Number(args?.index ?? 0);
        if (item?.kind.kind === 'text' && index < item.kind.data.steps.length) { engine.store.program_state.active_step_index = index; engine.store.program_state.active_text = item.kind.data.steps[index]; }
        if (item?.kind.kind === 'presentation') { const paths = item.kind.data.slides || [item.kind.data.path]; if (paths[index]) { engine.store.program_state.active_step_index = index; engine.store.program_state.background = { kind: 'image', path: paths[index] }; } }
        engine.bumpVersion(); engine.persistAndBroadcast(); return undefined as T;
      }
      case 'go_to_item': {
        const itemId = Number(args?.itemId);
        if (!Number.isFinite(itemId)) throw new Error('go_to_item requires { itemId }');
        engine.setActiveItem(itemId);
        engine.bumpVersion();
        engine.persistAndBroadcast();
        return undefined as T;
      }

      case 'go_to_step': {
        const index = Number(args?.index);
        if (!Number.isFinite(index)) throw new Error('go_to_step requires { index }');
        const project = engine.activeProject();
        const activeId = engine.store.program_state.active_item_id;
        const item =
          project && activeId != null ? project.session.items.find((it) => it.id === activeId) || null : null;

        if (item?.kind.kind === 'presentation') {
          const images = item.kind.data.slides || [item.kind.data.path];
          const indexClamped = Math.max(0, Math.min(index, images.length - 1));
          engine.store.program_state.active_step_index = indexClamped;
          engine.store.program_state.background = { kind: 'image', path: images[indexClamped] };
          engine.bumpVersion(); engine.persistAndBroadcast();
        }
        if (item?.kind.kind === 'text') {
          const steps = item.kind.data.steps || [];
          const clamped = Math.max(0, Math.min(index, Math.max(0, steps.length - 1)));
          engine.store.program_state.active_step_index = steps.length > 0 ? clamped : null;
          engine.store.program_state.active_text = steps.length > 0 ? steps[clamped] || '' : '';
          engine.bumpVersion();
          engine.persistAndBroadcast();
        }
        return undefined as T;
      }

      case 'nav_next':
      case 'nav_prev': {
        const dir = command === 'nav_next' ? 1 : -1;
        const project = engine.activeProject();
        if (!project) return undefined as T;

        const items = project.session.items || [];
        if (items.length === 0) return undefined as T;

        const activeId = engine.store.program_state.active_item_id;
        if (activeId == null) {
          engine.setActiveItem(items[0].id);
        } else {
          const activeIndex = items.findIndex((it) => it.id === activeId);
          const item = activeIndex >= 0 ? items[activeIndex] : null;
          if (item?.kind.kind === 'text') {
            const steps = item.kind.data.steps || [];
            if (steps.length > 0) {
              const current = engine.store.program_state.active_step_index ?? 0;
              const nextStep = Math.max(0, Math.min(current + dir, steps.length - 1));
              engine.store.program_state.active_step_index = nextStep;
              engine.store.program_state.active_text = steps[nextStep] || '';
            }
          } else if (activeIndex >= 0) {
            // Neighbouring items skip section headings.
            let target = activeIndex + dir;
            while (target >= 0 && target < items.length && items[target].kind.kind === 'section') target += dir;
            if (target >= 0 && target < items.length) engine.setActiveItem(items[target].id);
          }
        }

        engine.bumpVersion();
        engine.persistAndBroadcast();
        return undefined as T;
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

      // Media playback needs the desktop engine; the browser-only mode has none.

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
