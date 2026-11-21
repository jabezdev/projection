import LyricStream from './service/LyricStream';
import NowPlaying from './outputs/NowPlaying';
import OutputSettings from './settings/OutputSettings';
import OutputControls from './outputs/OutputControls';
import PartEditor, { type PartEdit } from './song/PartEditor';
import SessionPane from './service/SessionPane';
import { defaultOutputConfig, useOutputState } from './settings/outputState';
import { invoke, listen } from './api/tauri';
import type { UpdateState } from './settings/UpdateSettings';
import { useCallback, useEffect, useRef, useState } from 'react';
import SongEditor from './song/SongEditor';
import LibraryModal from './library/LibraryModal';
import QuickSearch from './library/QuickSearch';
import ControlsSettings from './settings/ControlsSettings';
import RemoteSettings from './settings/RemoteSettings';
import { useConfirm } from './ui/ConfirmDialog';
import { useInputBindings, useKeyboardInput } from './api/input';
import { useOutputsLoading, useOutputsShown } from './outputs/useOutputs';
import ServiceEditor from './service/ServiceEditor';
import { OutputConfigContext } from './outputs/preview';
import { forgetSong } from './song/useSongDocs';
import { useColumns } from './ui/useColumns';
import { useEngine, type ProjectHeader } from './api/useEngine';
import { Icon } from './ui/Icons';
import { newItemId, type Media, type SessionItem, type SongHeader, type SongModel } from './api/models';
import { decodeSong, itemSong, librarySong, songSteps, type SongDocument } from './song/song';
import { songItem } from './service/serviceItems';

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
  const [updateReady, setUpdateReady] = useState<string | null>(null);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<UpdateState>('update:state', event => { if (event.payload.state === 'ready') setUpdateReady(event.payload.version); }).then(u => { unlisten = u; });
    return () => unlisten?.();
  }, []);
  const streamRef = useRef<HTMLDivElement>(null);
  const [library, setLibrary] = useState<SongHeader[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [editor, setEditor] = useState<{ libraryId?: number; title?: string; song?: SongDocument } | null>(null);
  const [partEditor, setPartEditor] = useState<PartEdit | null>(null);
  const [serviceEditor, setServiceEditor] = useState<{ itemId?: number } | null>(null);
  const [quickSearch, setQuickSearch] = useState(false);
  const display = program?.display || 'live';
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
      const edited = { ...song, editedInService: true };
      return { ...item, song: JSON.stringify(edited), kind: { kind: 'text' as const, data: { steps: songSteps(edited) } } };
    });
    return persist({ ...current, session: { items } });
  }
  const select = useCallback(async (item: SessionItem, index = 0) => { await invoke('cue', { itemId: item.id, index }); }, []);
  const selectFromSchedule = (item: SessionItem, index = 0) => { requestAnimationFrame(() => scrollToCue(item.id, index)); return select(item, index); };
  const cueItem = useCallback((item: SessionItem, index = 0) => { void run(() => select(item, index)); }, [run, select]);
  async function refreshCatalog() { const [songs, assets] = await Promise.all([invoke<SongHeader[]>('list_songs'), invoke<Media[]>('list_media')]); setLibrary(songs); setMedia(assets); }
  async function openLibrary() { await refreshCatalog(); setLibraryOpen(true); }
  async function openService(itemId?: number) { await refreshCatalog(); setServiceEditor({ itemId }); }
  async function refreshServices() { setProjects(await invoke<ProjectHeader[]>('list_projects')); }
  async function loadService(projectId: number) {
    if (outputsShown && project && project.id !== projectId && !(await confirm('The outputs are shown. Load another service? Its first item goes on air.', 'Load service'))) return false;
    await saveQueue.current; await invoke('load_project', { projectId });
    return true;
  }
  async function editLibrarySong(id: number) { forgetSong(id); const saved = await invoke<SongModel>('get_song', { id }); setEditor({ libraryId: id, title: saved.title, song: decodeSong(saved.lyrics) }); }
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
  async function addMedia(item: Media) { const current = projectRef.current; if (!current) throw new Error('Select a service first.'); await persist({ ...current, session: { items: [...current.session.items, { id: newItemId(), title: item.title, kind: { kind: item.kind, data: { path: item.path } } }] } }); }
  async function addSong(id: number) {
    const current = projectRef.current;
    if (!current) throw new Error('Create or select a service first.');
    const song = await invoke<SongModel>('get_song', { id });
    await persist({ ...current, session: { items: [...current.session.items, songItem(song)] } });
  }
  return <OutputConfigContext.Provider value={outputConfig}><div className="operator-shell">

    {updateReady && <div role="status" className="ui-notice">PALTO {updateReady} is ready to install: Settings → System.<button aria-label="Dismiss update notice" onClick={() => setUpdateReady(null)}><Icon name="close"/></button></div>}
    {error && <div role="alert" className="ui-error">{error}<button aria-label="Dismiss error" onClick={() => setError('')}><Icon name="close"/></button></div>}
    <div ref={panesRef} className="operator-panes" style={panesStyle}>
      <SessionPane project={project} pending={outputsLoading.projection} projectRef={projectRef} projects={projects} program={program} run={run} persist={persist}
        onOpenSettings={() => setSettingsOpen(true)}
        onLoadService={loadService} onEditService={itemId => void run(() => openService(itemId))} onOpenLibrary={() => void run(openLibrary)}
        onServicesChanged={refreshServices} confirm={confirm} onError={setError} onEditPart={setPartEditor}
        onCue={selectFromSchedule}/>
      {resizeDivider(0)}
      <section className="lyrics-pane" aria-label="Continuous lyrics"><div ref={streamRef} className="pane-scroll lyric-stream">
        <LyricStream items={project?.session.items} activeId={program?.active_item_id} activeStep={program?.active_step_index} onSelect={cueItem}/>
      </div><NowPlaying items={project?.session.items ?? []} nextItemId={program?.next_item_id}/></section>
      {resizeDivider(1)}
      <OutputControls display={display} loading={outputsLoading} onDisplay={value => void run(() => invoke('set_display', { display: value }))} onError={setError}/>
    </div>
    {settingsOpen && <OutputSettings config={outputConfig} onChange={setOutputConfig} onClose={() => setSettingsOpen(false)} extraTabs={[{ id: 'controls', label: 'Controls', content: <ControlsSettings bindings={bindings}/> }, { id: 'remote', label: 'Remote', content: <RemoteSettings/> }]}/>}
    {partEditor && <PartEditor part={partEditor} onClose={() => setPartEditor(null)} onSave={part => run(async () => { await editPart(part.itemId, part.partId, part.lyrics); setPartEditor(null); })}/>}
    {editor && <SongEditor backgrounds={media} title={editor.title} song={editor.song} heading={editor.libraryId != null ? 'Library song' : 'New library song'} onClose={() => setEditor(null)} onSave={async (title, song) => {
      const lyrics = JSON.stringify(librarySong(song));
      if (editor.libraryId != null) { await invoke('update_song', { song: { id: editor.libraryId, title, lyrics } }); forgetSong(editor.libraryId); }
      else await invoke('create_song', { title, lyrics });
      await refreshCatalog();
      setEditor(null);
    }}/>}
    {serviceEditor && project && <ServiceEditor project={project} persist={persist} songs={library} media={media} initialItemId={serviceEditor.itemId} activeItemId={program?.active_item_id}
      confirmRemove={outputsShown ? title => confirm(`The outputs are shown and ${title} is on air. Remove it from the service?`, 'Remove') : undefined}
      onCatalogChanged={refreshCatalog} onRenamed={name => setProjects(old => old.map(p => p.id === project.id ? { ...p, name } : p))} onClose={() => setServiceEditor(null)}/>}
    {quickSearch && <QuickSearch songs={library} media={media} canAdd={!!project} onClose={() => setQuickSearch(false)} onAdd={result => result.kind === 'song' ? addSong(result.song.id) : addMedia(result.media)}/>}
    {libraryOpen && <LibraryModal songs={library} media={media} confirm={confirm} onClose={() => setLibraryOpen(false)} onCatalogChanged={refreshCatalog} onNewSong={() => setEditor({})} onEditSong={id => void run(() => editLibrarySong(id))}/>}

    {confirmDialog}
  </div></OutputConfigContext.Provider>;
}
export default App;
