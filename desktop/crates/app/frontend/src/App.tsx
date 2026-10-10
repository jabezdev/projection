import ScheduleVideo from './ScheduleVideo';
import LyricStream from './LyricStream';
import OutputSettings from './OutputSettings';
import { NativePreview } from './NativePreview';
import { useOutputSizes } from './useOutputSizes';
import PresentationEditor, { PresentationCues } from './PresentationEditor';
import { defaultOutputConfig, useOutputState } from './outputState';
import { invoke, listen } from './tauri';
import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';
import SongEditor from './SongEditor';
import { Icon } from './Icons';
import LibraryPanel from './LibraryPanel';
import ServiceBuilder from './ServiceBuilder';
import { defaultVideoOptions, newItemId, type Media, type SessionItem, type ProjectModel, type SongHeader, type SongModel, type VideoOptions } from './models';
import { arrangedParts, decodeSong, partCode, partColor, slides, songSteps, type SongDocument } from './song';


type BackgroundKind = 'image' | 'video';

type Background = {
  kind: BackgroundKind;
  path: string;
};

type ProgramState = {
  display?: 'logo' | 'black' | 'clear' | 'live';
  mode: 'manual' | 'automated' | 'external';
  active_item_id: number | null;
  next_item_id: number | null;
  active_step_index: number | null;
  active_text: string;
  background: Background | null;
};

type ProjectHeader = { id: number; name: string; updated_at_ms: number };

type EngineSnapshot = {
  program_state: ProgramState;
  version: number;
  project: ProjectModel | null;
};

// Each state event carries the complete state of one domain; replace, don't merge.
type EngineEvent =
  | { type: 'project_changed'; payload: { project: ProjectModel | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: ProgramState; version: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

function App() {
  const [outputConfig, setOutputConfig] = useOutputState('config', defaultOutputConfig);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [presentationEditor, setPresentationEditor] = useState<SessionItem | 'new' | null>(null);
  const outputSizes = useOutputSizes(true);
  const streamRef = useRef<HTMLDivElement>(null);
  const initializedWidths = useRef(false);
  const [columnWidths, setColumnWidths] = useState([300, 450, 350]);
  const widthsRef = useRef(columnWidths);
  const resizeFrame = useRef<number | null>(null);
  useEffect(() => { widthsRef.current = columnWidths; }, [columnWidths]);
  const panesRef = useRef<HTMLDivElement>(null);
  const resizeRef = useRef<{ index: number; x: number; widths: number[] } | null>(null);
  useEffect(() => {
    const element = panesRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setColumnWidths(current => {
      const available = Math.max(0, element.clientWidth - 10);
      const minimum = Math.min(140, available / 3);
      if (!initializedWidths.current) { initializedWidths.current = true; return [available * 30 / 110, available * 45 / 110, available * 35 / 110]; }
      const right = Math.max(minimum, Math.min(current[2], available - minimum * 2));
      const left = Math.max(minimum, Math.min(current[0], available - right - minimum));
      const next = [left, available - left - right, right];
      return next.every((width, index) => width === current[index]) ? current : next;
    }));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  function resizeColumns(widths: number[], index: number, delta: number) {
    const total = widths[index] + widths[index + 1];
    const minimum = Math.min(140, total / 2);
    const left = Math.max(minimum,
      Math.min(total - minimum, widths[index] + delta));
    const next = [...widths];
    next[index] = left; next[index + 1] = total - left;
    return next;
  }
  function finishResize() {
    if (!resizeRef.current) return;
    if (resizeFrame.current !== null) cancelAnimationFrame(resizeFrame.current);
    resizeFrame.current = null;
    panesRef.current?.classList.remove('is-resizing');
    const widths = widthsRef.current;
    panesRef.current?.style.setProperty('--operator-columns', `${widths[0]}px 5px minmax(0, 1fr) 5px ${widths[2]}px`);
    setColumnWidths([...widths]);
    resizeRef.current = null;
  }
  const resizeDivider = (index: number) => <div className="column-divider" role="separator"
    aria-label={index === 0 ? 'Resize session and lyrics columns' : 'Resize lyrics and controls columns'}
    aria-orientation="vertical" aria-valuenow={Math.round(columnWidths[index])} aria-valuemin={140}
    aria-valuemax={Math.round(columnWidths[index] + columnWidths[index + 1] - 140)} tabIndex={0}
    onPointerDown={event => {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      resizeRef.current = { index, x: event.clientX, widths: [...widthsRef.current] };
      panesRef.current?.classList.add('is-resizing');
    }}
    onPointerMove={event => {
      const drag = resizeRef.current;
      if (!drag) return;
      const widths = resizeColumns(drag.widths, index, event.clientX - drag.x);
      widthsRef.current = widths;
      if (resizeFrame.current === null) resizeFrame.current = requestAnimationFrame(() => {
        resizeFrame.current = null;
        const current = widthsRef.current;
        panesRef.current?.style.setProperty('--operator-columns', `${current[0]}px 5px minmax(0, 1fr) 5px ${current[2]}px`);
      });
      // Rebase at the clamped position so reversing direction responds immediately.
      resizeRef.current = { index, x: event.clientX, widths };
    }}
    onPointerUp={finishResize} onLostPointerCapture={finishResize}
    onPointerCancel={finishResize}
    onKeyDown={event => {
      if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
      event.preventDefault();
      setColumnWidths(current => resizeColumns(current, index, event.key === 'ArrowRight' ? 16 : -16));
    }}/>

  const [project, setProject] = useState<ProjectModel | null>(null);
  const [projects, setProjects] = useState<ProjectHeader[]>([]);
  const [program, setProgram] = useState<ProgramState | null>(null);
  const [library, setLibrary] = useState<SongHeader[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [editor, setEditor] = useState<{ id?: number; libraryId?: number; title?: string; song?: SongDocument } | null>(null);
  const [partEditor, setPartEditor] = useState<{ itemId: number; partId: string; label: string; lyrics: string } | null>(null);
  const [savingPart, setSavingPart] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [dragged, setDragged] = useState<number | null>(null);
  const [error, setError] = useState('');
  const [serviceEditor, setServiceEditor] = useState<'new' | 'edit' | null>(null);
  const [media, setMedia] = useState<Media[]>([]);
  const saveQueue = useRef(Promise.resolve());
  const projectRef = useRef(project);
  const pendingSaves = useRef(0);
  const run = useCallback(async (work: () => Promise<unknown>) => { setError(''); try { await work(); } catch (e) { setError(String(e)); } }, []);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    (async () => {
      const unlisten = await listen<EngineEvent>('engine:event', ({ payload }) => {
        if (disposed) return;
        if (payload.type === 'project_changed' && !pendingSaves.current) { setProject(payload.payload.project); projectRef.current = payload.payload.project; }
        if (payload.type === 'program_changed') setProgram(payload.payload.program_state);
        if (payload.type === 'error') setError(payload.payload.message);
      });
      if (disposed) { unlisten(); return; }
      cleanup = unlisten;
      const [snapshot, headers, assets] = await Promise.all([invoke<EngineSnapshot>('get_engine_snapshot'), invoke<ProjectHeader[]>('list_projects'), invoke<Media[]>('list_media')]);
      if (disposed) return;
      setProgram(snapshot.program_state); setProject(snapshot.project); projectRef.current = snapshot.project;
      setProjects(headers); setMedia(assets);
      const warnings = await invoke<string[] | undefined>('get_startup_warnings');
      if (!disposed && warnings?.length) setError(warnings.join(' '));
    })().catch(e => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; cleanup?.(); };
  }, []);
  const display = program?.display || 'live';
  const persist = useCallback((updated: ProjectModel) => {
    projectRef.current = updated; setProject(updated); pendingSaves.current++;
    const task = saveQueue.current.then(() => invoke('save_project', { project: updated })).then(() => {});
    saveQueue.current = task.catch(e => setError(String(e))).finally(() => { pendingSaves.current--; });
    return task;
  }, []);
  const updateVideoOptions = useCallback((id: number, options: VideoOptions) => {
    const current = projectRef.current;
    if (!current) return;
    void persist({ ...current, session: { items: current.session.items.map(item => item.id === id && (item.kind.kind === 'video' || item.kind.kind === 'audio') ? { ...item, kind: { ...item.kind, data: { ...item.kind.data, options } } } : item) } }).catch(console.error);
  }, [persist]);
  const scrollToCue = useCallback((id: number, index = 0) => {
    const stream = streamRef.current;
    const article = stream?.querySelector<HTMLElement>(`[data-item-id="${id}"]`);
    if (article) article.style.contentVisibility = 'visible';
    const target = article?.querySelector<HTMLElement>(`[data-step-index="${index}"]`) || article;
    if (!stream || !target) return;
    stream.scrollTo({ top: stream.scrollTop + target.getBoundingClientRect().top - stream.getBoundingClientRect().top - (stream.clientHeight - target.clientHeight) / 2, behavior: 'instant' });
    if (article) requestAnimationFrame(() => { article.style.contentVisibility = ''; });
  }, []);
  useEffect(() => {
    const stream = streamRef.current;
    if (!stream) return;
    const observer = new ResizeObserver(() => stream.style.setProperty('--stream-height', `${stream.clientHeight}px`));
    observer.observe(stream);
    return () => observer.disconnect();
  }, []);
  function itemSong(item: SessionItem): SongDocument {
    if (item.song) return decodeSong(item.song);
    const steps = item.kind.kind === 'text' ? item.kind.data.steps : [];
    return { format: 'magi.song.v1', slideSeparator: 'blank-line', background: item.background || '', parts: [{ id: 'legacy', type: 'Verse', number: 1, lyrics: steps.join('\n\n'), slideTexts: steps }], arrangement: ['legacy'] };
  }
  function editPart(itemId: number, partId: string, lyrics: string) {
    const current = projectRef.current;
    if (!current) return;
    const items = current.session.items.map(item => {
      if (item.id !== itemId) return item;
      const song = itemSong(item);
      song.parts = song.parts.map(part => part.id === partId ? { ...part, lyrics, slideTexts: undefined } : part);
      return { ...item, song: JSON.stringify(song), kind: { kind: 'text' as const, data: { steps: songSteps(song) } } };
    });
    return persist({ ...current, session: { items } });
  }
  const select = useCallback(async (item: SessionItem, index = 0) => { await invoke('cue', { itemId: item.id, index }); }, []);
  const selectFromSchedule = (item: SessionItem, index = 0) => { requestAnimationFrame(() => scrollToCue(item.id, index)); return select(item, index); };
  const cueItem = useCallback((item: SessionItem, index = 0) => { void run(() => select(item, index)); }, [run, select]);
  async function refreshCatalog() { const [songs, assets] = await Promise.all([invoke<SongHeader[]>('list_songs'), invoke<Media[]>('list_media')]); setLibrary(songs); setMedia(assets); }
  async function openLibrary() { await refreshCatalog(); setLibraryOpen(true); }
  async function openService(editor: 'new' | 'edit') { await refreshCatalog(); setServiceEditor(editor); }
  async function addMedia(item: Media) { const current = projectRef.current; if (!current) throw new Error('Select a service first.'); await persist({ ...current, session: { items: [...current.session.items, { id: newItemId(), title: item.title, kind: { kind: item.kind, data: { path: item.path } } }] } }); setLibraryOpen(false); }
  async function addSong(id: number) {
    const current = projectRef.current;
    if (!current) throw new Error('Create or select a service first.');
    const song = await invoke<SongModel>('get_song', { id });
    const doc = decodeSong(song.lyrics);
    await persist({ ...current, session: { items: [...current.session.items, { id: newItemId(), title: song.title, song: JSON.stringify(doc), background: doc.background, kind: { kind: 'text', data: { steps: songSteps(doc) } } }] } });
    setLibraryOpen(false);
  }
  return <div className="operator-shell">

    {error && <div role="alert" className="ui-error">{error}<button aria-label="Dismiss error" onClick={() => setError('')}>×</button></div>}
    <div ref={panesRef} className="operator-panes" style={{ '--operator-columns': `${columnWidths[0]}px 5px minmax(0, 1fr) 5px ${columnWidths[2]}px`, gridTemplateColumns: 'var(--operator-columns)' } as CSSProperties}>
      <section className="session-pane"><div className="operator-settings-bar"><button aria-label="Open output settings" onClick={() => setSettingsOpen(true)}><Icon name="settings"/><span>Settings</span></button></div><div className="session-tools" aria-label="Service and library controls"><div className="service-select-row"><select aria-label="Session / service" value={project?.id || ''} onChange={e => {
        // Capture the choice before awaiting: React restores a controlled select
        // to its current value until the engine publishes the loaded service.
        const projectId = Number(e.currentTarget.value);
        if (!Number.isSafeInteger(projectId) || projectId <= 0) return;
        void run(async () => { await saveQueue.current; await invoke('load_project', { projectId }); });
      }}><option value="" disabled>Select a service</option>{projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select><button className="icon-button" aria-label="Edit current service" disabled={!project} onClick={() => void run(() => openService('edit'))}><Icon name="edit"/></button></div><button onClick={() => void run(() => openService('new'))}><Icon name="plus"/> Service</button><button onClick={() => void run(openLibrary)}><Icon name="library"/> Library</button><button className="primary-action" onClick={() => setEditor({})}><Icon name="plus"/> Song</button></div><div className="pane-scroll">
        {!project?.session.items.length && <p className="empty-panel">{project ? 'Add songs from the library to begin.' : 'Create or select a service to begin.'}</p>}
        {project?.session.items.map((item) => <article className={`session-song ${program?.active_item_id === item.id ? 'is-current' : ''}`} key={item.id}>
          <header draggable onDragStart={e => { setDragged(item.id); e.dataTransfer.setData('text/plain', String(item.id)); }} onDragEnd={() => setDragged(null)} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); const current = projectRef.current; if (!current || dragged === null || dragged === item.id) return; const items = [...current.session.items]; const from = items.findIndex(i => i.id === dragged); const to = items.findIndex(i => i.id === item.id); if (from < 0 || to < 0) return; const [moved] = items.splice(from, 1); items.splice(to, 0, moved); void run(() => persist({ ...current, session: { items } })); setDragged(null); }}>
            <button className={`song-collapse ${collapsed.has(item.id) ? 'is-collapsed' : ''}`} aria-label={`${collapsed.has(item.id) ? 'Expand' : 'Collapse'} ${item.title}`} aria-expanded={!collapsed.has(item.id)} onClick={() => setCollapsed(old => { const next = new Set(old); if (next.has(item.id)) next.delete(item.id); else next.add(item.id); return next; })}>
              <span className="drag-handle" title="Drag to reorder"><Icon name="grip"/></span><Icon name="chevron"/><span className="song-name">{item.title || item.kind.kind}</span>
            </button>
            {item.kind.kind === 'video' && <button className="confidence-header-toggle" role="switch" aria-checked={!!item.kind.data.options?.confidence} aria-label={`Show ${item.title || 'video'} on confidence screen`} title="Show video on confidence screen (silent)" onDragStart={e => { e.preventDefault(); e.stopPropagation(); }} onClick={() => { if (item.kind.kind === 'video') updateVideoOptions(item.id, { ...defaultVideoOptions, ...item.kind.data.options, confidence: !item.kind.data.options?.confidence }); }}><span className="confidence-toggle-check" aria-hidden="true">{item.kind.data.options?.confidence ? '✓' : ''}</span><span>Confidence</span></button>}
            {(item.kind.kind === 'text' || item.kind.kind === 'presentation') && <button className="song-edit icon-button" aria-label={`Edit ${item.kind.kind === 'presentation' ? 'presentation' : 'song'} ${item.title}`} title={item.kind.kind === 'presentation' ? 'Edit presentation slides' : 'Edit song arrangement and background'} onClick={() => item.kind.kind === 'presentation' ? setPresentationEditor(item) : setEditor({ id: item.id, title: item.title || '', song: itemSong(item) })}><Icon name="edit"/></button>}
          </header>
          {!collapsed.has(item.id) && item.kind.kind === 'text' && <div className="part-cues" aria-label={`${item.title} arrangement`}>{(() => {
            let offset = 0;
            return arrangedParts(itemSong(item)).map((part, occurrence) => {
              const start = offset;
              const length = slides(part).length;
              offset += length;
              const active = program?.active_item_id === item.id && program.active_step_index != null && program.active_step_index >= start && program.active_step_index < start + length;
              return <div className={`part-cue ${active ? 'is-active' : ''}`} style={{ '--part-color': partColor(part) } as CSSProperties} key={`${part.id}-${occurrence}`}>
                <button className="part-cue-select" aria-label={`Cue ${partCode(part)}, ${item.title}, occurrence ${occurrence + 1}`} aria-pressed={active} disabled={!length} onClick={() => void run(() => selectFromSchedule(item, start))}>{partCode(part)}</button>
                <button className="part-cue-edit icon-button" aria-label={`Edit ${partCode(part)}, ${item.title}, occurrence ${occurrence + 1}`} onClick={() => setPartEditor({ itemId: item.id, partId: part.id, label: `${item.title} · ${part.type} ${part.number}`, lyrics: part.lyrics })}><Icon name="edit"/></button>
              </div>;
            });
          })()}</div>}
          {!collapsed.has(item.id) && item.kind.kind === 'presentation' && <PresentationCues item={item} active={program?.active_item_id === item.id} step={program?.active_step_index} onSelect={index => void run(() => selectFromSchedule(item, index))}/> }
          {!collapsed.has(item.id) && item.kind.kind === 'audio' && <button className="schedule-audio-cue" onClick={() => void run(() => selectFromSchedule(item))}><Icon name="audio"/>Audio<Icon name="live"/></button>}
          {!collapsed.has(item.id) && item.kind.kind === 'video' && <ScheduleVideo title={item.title || 'Untitled video'} path={item.kind.data.path} active={program?.active_item_id === item.id} onSelect={() => void run(() => selectFromSchedule(item))}/>}

        </article>)}
      </div></section>
      {resizeDivider(0)}
      <section className="lyrics-pane" aria-label="Continuous lyrics"><div ref={streamRef} className="pane-scroll lyric-stream">
        <LyricStream items={project?.session.items} activeId={program?.active_item_id} activeStep={program?.active_step_index} onSelect={cueItem} onVideoOptions={updateVideoOptions}/>

      </div></section>
      {resizeDivider(1)}
      <aside className="operator-controls" aria-label="Output controls and previews">
        <div className="display-controls">{(['logo', 'black', 'clear', 'live'] as const).map(value => <button key={value} aria-pressed={display === value} onClick={() => void run(() => invoke('set_display', { display: value }))}><Icon name={value}/><span>{value.charAt(0).toUpperCase() + value.slice(1)}</span></button>)}</div>
        <div className="preview-stack">
          <section aria-label="Projection preview"><div className="preview-label">Projection</div><div className="preview-viewport"><div className="output-preview-frame" style={{ '--output-aspect': outputSizes.projection } as CSSProperties}><NativePreview target="projection" label="Projection output"/></div></div></section>
          <section aria-label="Confidence preview"><div className="preview-label">Confidence</div><div className="preview-viewport"><div className="output-preview-frame confidence-preview-surface" style={{ '--output-aspect': outputSizes.confidence } as CSSProperties}><NativePreview target="confidence" label="Confidence output"/></div></div></section>
        </div>

      </aside>
    </div>
    {settingsOpen && <OutputSettings config={outputConfig} onChange={setOutputConfig} onClose={() => setSettingsOpen(false)}/>}
    {presentationEditor && <PresentationEditor images={media} item={presentationEditor === 'new' ? undefined : presentationEditor} onClose={() => setPresentationEditor(null)} onSave={async item => {
      const current = projectRef.current; if (!current) throw new Error('Select a service first.');
      await persist({ ...current, session: { items: presentationEditor === 'new' ? [...current.session.items, item] : current.session.items.map(old => old.id === item.id ? item : old) } });
      setPresentationEditor(null);
    }}/>}
    {partEditor && <div className="modal-backdrop" onKeyDown={e => {
      if (e.key === 'Escape' && !savingPart) setPartEditor(null);
      if (e.key === 'Tab') {
        const targets = e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), textarea');
        const first = targets[0], last = targets[targets.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    }}><form className="compact-modal part-edit-modal" role="dialog" aria-modal="true" aria-label={`Edit ${partEditor.label}`} onSubmit={e => { e.preventDefault(); setSavingPart(true); void run(async () => { try { await editPart(partEditor.itemId, partEditor.partId, partEditor.lyrics); setPartEditor(null); } finally { setSavingPart(false); } }); }}>
      <header><h2>{partEditor.label}</h2><button type="button" aria-label="Close part editor" disabled={savingPart} onClick={() => setPartEditor(null)}>×</button></header>
      <textarea autoFocus aria-label="Part lyrics" value={partEditor.lyrics} onChange={e => setPartEditor({ ...partEditor, lyrics: e.target.value })}/>
      <small>A blank line creates a new slide. Changes apply to every occurrence of this part.</small>
      <div className="modal-actions"><button type="button" disabled={savingPart} onClick={() => setPartEditor(null)}>Cancel</button><button className="primary-action" disabled={savingPart}>{savingPart ? 'Saving…' : 'Save part'}</button></div>
    </form></div>}
    {editor && <SongEditor backgrounds={media} title={editor.title} song={editor.song} onClose={() => setEditor(null)} onSave={async (title, song) => { if (editor.id != null) { const current = projectRef.current; if (!current) return; await persist({ ...current, session: { items: current.session.items.map(item => item.id === editor.id ? { ...item, title, song: JSON.stringify(song), background: song.background, kind: { kind: 'text', data: { steps: songSteps(song) } } } : item) } }); } else if (editor.libraryId != null) { await invoke('update_song', { song: { id: editor.libraryId, title, lyrics: JSON.stringify(song) } }); await openLibrary(); } else { await invoke('create_song', { title, lyrics: JSON.stringify(song) }); if (libraryOpen) await openLibrary(); } setEditor(null); }}/ >}
    {serviceEditor && <ServiceBuilder name={serviceEditor === 'edit' ? project?.name : undefined} items={serviceEditor === 'edit' ? project?.session.items : undefined} currentItems={project?.session.items || []} songs={library} media={media} onClose={() => setServiceEditor(null)} onSave={async (name, items) => {
      await saveQueue.current;
      if (serviceEditor === 'edit' && projectRef.current) {
        const current = projectRef.current;
        await persist({ ...current, name, session: { items } });
        setProjects(old => old.map(p => p.id === current.id ? { ...p, name } : p));
      } else {
        const created = await invoke<ProjectModel>('create_project', { name });
        await invoke('save_project', { project: { ...created, session: { items } } });
        await invoke('load_project', { projectId: created.id });
        setProjects(await invoke<ProjectHeader[]>('list_projects'));
      }
      setServiceEditor(null);
    }}/ >}
    {libraryOpen && <LibraryPanel onNewPresentation={() => { setLibraryOpen(false); setPresentationEditor('new'); }} songs={library} media={media} canAdd={!!project} onClose={() => setLibraryOpen(false)} onAddSong={addSong} onAddMedia={addMedia} onNewSong={() => { setLibraryOpen(false); setEditor({}); }} onEditSong={async id => { const saved = await invoke<SongModel>('get_song', { id }); setLibraryOpen(false); setEditor({ libraryId: id, title: saved.title, song: decodeSong(saved.lyrics) }); }} onImported={item => setMedia(old => [...old, item])}/ >}

  </div>;
}
export default App;
