import LyricStream from './LyricStream';
import OutputSettings from './OutputSettings';
import OutputControls from './OutputControls';
import PresentationEditor from './PresentationEditor';
import PartEditor, { type PartEdit } from './PartEditor';
import SessionPane from './SessionPane';
import { defaultOutputConfig, useOutputState } from './outputState';
import { invoke } from './tauri';
import { useCallback, useEffect, useRef, useState } from 'react';
import SongEditor from './SongEditor';
import LibraryPanel from './LibraryPanel';
import QuickSearch from './QuickSearch';
import ControlsSettings from './ControlsSettings';
import RemoteSettings from './RemoteSettings';
import { useConfirm } from './ConfirmDialog';
import { useInputBindings, useKeyboardInput } from './input';
import { useOutputsLoading, useOutputsShown } from './useOutputs';
import ServiceBuilder from './ServiceBuilder';
import { useColumns } from './useColumns';
import { useEngine, type ProjectHeader } from './useEngine';
import { newItemId, type Media, type SessionItem, type ProjectModel, type SongHeader, type SongModel, type VideoOptions } from './models';
import { decodeSong, itemSong, songSteps, type SongDocument } from './song';

function App() {
  const [outputConfig, setOutputConfig] = useOutputState('config', defaultOutputConfig);
  const { project, projectRef, projects, setProjects, program, media, setMedia, error, setError, run, persist, saveQueue } = useEngine();
  const { panesRef, panesStyle, resizeDivider } = useColumns();
  const bindings = useInputBindings();
  useKeyboardInput(bindings, setError);
  const outputsShown = useOutputsShown();
  const outputsLoading = useOutputsLoading();
  const [confirmDialog, confirm] = useConfirm();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [presentationEditor, setPresentationEditor] = useState<SessionItem | 'new' | null>(null);
  const streamRef = useRef<HTMLDivElement>(null);
  const [library, setLibrary] = useState<SongHeader[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [editor, setEditor] = useState<{ id?: number; libraryId?: number; title?: string; song?: SongDocument } | null>(null);
  const [partEditor, setPartEditor] = useState<PartEdit | null>(null);
  const [serviceEditor, setServiceEditor] = useState<'new' | 'edit' | null>(null);
  const [quickSearch, setQuickSearch] = useState(false);
  const display = program?.display || 'live';
  const updateVideoOptions = useCallback((id: number, options: VideoOptions) => {
    const current = projectRef.current;
    if (!current) return;
    void persist({ ...current, session: { items: current.session.items.map(item => item.id === id && (item.kind.kind === 'video' || item.kind.kind === 'audio') ? { ...item, kind: { ...item.kind, data: { ...item.kind.data, options } } } : item) } }).catch(console.error);
  }, [persist, projectRef]);
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
  async function openQuickSearch() { await refreshCatalog(); setQuickSearch(true); }
  const openQuickSearchRef = useRef(openQuickSearch);
  useEffect(() => { openQuickSearchRef.current = openQuickSearch; });
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === 'k') { event.preventDefault(); void run(() => openQuickSearchRef.current()); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [run]);
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
    <div ref={panesRef} className="operator-panes" style={panesStyle}>
      <SessionPane project={project} pending={outputsLoading.projection} projectRef={projectRef} projects={projects} program={program} run={run} persist={persist}
        onOpenSettings={() => setSettingsOpen(true)}
        onLoadService={projectId => void run(async () => {
          // While the audience sees the outputs, loading a service changes what they see.
          if (outputsShown && project && project.id !== projectId && !(await confirm('The outputs are shown. Load another service? Its first item goes on air.', 'Load service'))) return;
          await saveQueue.current; await invoke('load_project', { projectId });
        })}
        onOpenService={mode => void run(() => openService(mode))} onOpenLibrary={() => void run(openLibrary)} onNewSong={() => setEditor({})}
        onEditSong={item => setEditor({ id: item.id, title: item.title || '', song: itemSong(item) })} onEditPresentation={setPresentationEditor} onEditPart={setPartEditor}
        onCue={selectFromSchedule} onVideoOptions={updateVideoOptions}/>
      {resizeDivider(0)}
      <section className="lyrics-pane" aria-label="Continuous lyrics"><div ref={streamRef} className="pane-scroll lyric-stream">
        <LyricStream items={project?.session.items} activeId={program?.active_item_id} activeStep={program?.active_step_index} onSelect={cueItem} onVideoOptions={updateVideoOptions}/>

      </div></section>
      {resizeDivider(1)}
      <OutputControls display={display} loading={outputsLoading} onDisplay={value => void run(() => invoke('set_display', { display: value }))} onError={setError}/>
    </div>
    {settingsOpen && <OutputSettings config={outputConfig} onChange={setOutputConfig} onClose={() => setSettingsOpen(false)} extraTabs={[{ id: 'controls', label: 'Controls', content: <ControlsSettings bindings={bindings}/> }, { id: 'remote', label: 'Remote', content: <RemoteSettings/> }]}/>}
    {presentationEditor && <PresentationEditor images={media} item={presentationEditor === 'new' ? undefined : presentationEditor} onClose={() => setPresentationEditor(null)} onSave={async item => {
      const current = projectRef.current; if (!current) throw new Error('Select a service first.');
      await persist({ ...current, session: { items: presentationEditor === 'new' ? [...current.session.items, item] : current.session.items.map(old => old.id === item.id ? item : old) } });
      setPresentationEditor(null);
    }}/>}
    {partEditor && <PartEditor part={partEditor} onClose={() => setPartEditor(null)} onSave={part => run(async () => { await editPart(part.itemId, part.partId, part.lyrics); setPartEditor(null); })}/>}
    {editor && <SongEditor backgrounds={media} title={editor.title} song={editor.song} onClose={() => setEditor(null)} onSave={async (title, song) => { if (editor.id != null) { const current = projectRef.current; if (!current) return; await persist({ ...current, session: { items: current.session.items.map(item => item.id === editor.id ? { ...item, title, song: JSON.stringify(song), background: song.background, kind: { kind: 'text', data: { steps: songSteps(song) } } } : item) } }); } else if (editor.libraryId != null) { await invoke('update_song', { song: { id: editor.libraryId, title, lyrics: JSON.stringify(song) } }); await openLibrary(); } else { await invoke('create_song', { title, lyrics: JSON.stringify(song) }); if (libraryOpen) await openLibrary(); } setEditor(null); }}/ >}
    {serviceEditor && <ServiceBuilder name={serviceEditor === 'edit' ? project?.name : undefined} items={serviceEditor === 'edit' ? project?.session.items : undefined} currentItems={project?.session.items || []} songs={library} media={media} confirmRemove={outputsShown ? title => confirm(`The outputs are shown. Remove ${title} from the service?`, 'Remove') : undefined} onClose={() => setServiceEditor(null)} onSave={async (name, items) => {
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
    {quickSearch && <QuickSearch songs={library} media={media} canAdd={!!project} onClose={() => setQuickSearch(false)} onAdd={result => result.kind === 'song' ? addSong(result.song.id) : addMedia(result.media)}/>}
    {confirmDialog}
    {libraryOpen && <LibraryPanel onCatalogChanged={refreshCatalog} onNewPresentation={() => { setLibraryOpen(false); setPresentationEditor('new'); }} songs={library} media={media} canAdd={!!project} onClose={() => setLibraryOpen(false)} onAddSong={addSong} onAddMedia={addMedia} onNewSong={() => { setLibraryOpen(false); setEditor({}); }} onEditSong={async id => { const saved = await invoke<SongModel>('get_song', { id }); setLibraryOpen(false); setEditor({ libraryId: id, title: saved.title, song: decodeSong(saved.lyrics) }); }} onImported={item => setMedia(old => [...old, item])}/ >}

  </div>;
}
export default App;
