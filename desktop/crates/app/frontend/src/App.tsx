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
import SyncSettings from './settings/SyncSettings';
import { useSyncStatus } from './settings/useSyncStatus';
import { useConfirm } from './ui/ConfirmDialog';
import { useInputBindings, useKeyboardInput } from './api/input';
import { useOutputsLoading, useOutputsShown } from './outputs/useOutputs';
import ServiceEditor from './service/ServiceEditor';
import { OutputConfigContext } from './outputs/preview';
import { useColumns } from './ui/useColumns';
import { useEngine } from './api/useEngine';
import { Icon } from './ui/Icons';
import type { MediaFile, Service, SessionItem, Song, SongVersion } from './api/models';
import { mediaItem, songItem, withPartLyrics } from './service/serviceItems';

function App() {
  const [outputConfig, setOutputConfig] = useOutputState('config', defaultOutputConfig);
  const { live, serviceRef, services, setServices, program, media, setMedia, error, setError, run, persist, adopt, saveQueue } = useEngine();
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
  const [syncStatus] = useSyncStatus();
  const [settingsTab, setSettingsTab] = useState<string | undefined>(undefined);
  const refreshRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    void listen<string[]>('library:changed', () => { window.clearTimeout(timer); timer = window.setTimeout(() => void refreshRef.current().catch(() => {}), 400); }).then(u => { unlisten = u; });
    return () => { unlisten?.(); window.clearTimeout(timer); };
  }, []);
  const streamRef = useRef<HTMLDivElement>(null);
  const [library, setLibrary] = useState<Song[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [editor, setEditor] = useState<{ song?: Song } | null>(null);
  const [partEditor, setPartEditor] = useState<PartEdit | null>(null);
  const [serviceEditor, setServiceEditor] = useState<{ itemId?: string; service: Service } | null>(null);
  const [quickSearch, setQuickSearch] = useState(false);
  const display = program?.display || 'live';
  const scrollToCue = useCallback((id: string, index = 0) => {
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
  async function editPart(itemId: string, occurrence: number, lyrics: string) {
    const current = serviceRef.current;
    const versionId = live?.items.find(i => i.id === itemId)?.song?.version_id;
    if (!current || !versionId) return;
    const version = await invoke<SongVersion>('get_song_version', { id: versionId });
    const items = current.items.map(item => item.id === itemId && item.content.kind === 'song' ? { ...item, content: { ...withPartLyrics(item.content, version, occurrence, lyrics), kind: 'song' as const } } : item);
    return persist({ ...current, items });
  }
  const select = useCallback(async (item: SessionItem, index = 0) => { await invoke('cue', { itemId: item.id, index }); }, []);
  const selectFromSchedule = (item: SessionItem, index = 0) => { requestAnimationFrame(() => scrollToCue(item.id, index)); return select(item, index); };
  const cueItem = useCallback((item: SessionItem, index = 0) => { void run(() => select(item, index)); }, [run, select]);
  async function refreshCatalog() { const [songs, assets] = await Promise.all([invoke<Song[]>('list_songs'), invoke<MediaFile[]>('list_media')]); setLibrary(songs); setMedia(assets); }
  async function openLibrary() { await refreshCatalog(); setLibraryOpen(true); }
  async function openService(itemId?: string) { await refreshCatalog(); const service = serviceRef.current; if (service) setServiceEditor({ itemId, service }); }
  async function refreshServices() { setServices(await invoke<Service[]>('list_services')); }
  useEffect(() => { refreshRef.current = async () => { await Promise.all([refreshServices(), refreshCatalog()]); }; });
  async function loadService(serviceId: string) {
    if (outputsShown && live && live.service.id !== serviceId && !(await confirm('The outputs are shown. Load another service? Its first item goes on air.', 'Load service'))) return false;
    await saveQueue.current; await invoke('load_service', { serviceId });
    return true;
  }
  async function editLibrarySong(id: string) { setEditor({ song: await invoke<Song>('get_song', { id }) }); }
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
  async function addMedia(item: MediaFile) { const current = serviceRef.current; if (!current) throw new Error('Select a service first.'); await persist({ ...current, items: [...current.items, mediaItem(item)] }); }
  async function addSong(song: Song) {
    const current = serviceRef.current;
    if (!current) throw new Error('Create or select a service first.');
    await persist({ ...current, items: [...current.items, songItem(song)] });
  }
  return <OutputConfigContext.Provider value={outputConfig}><div className="operator-shell">

    {updateReady && <div role="status" className="ui-notice">PALTO {updateReady} is ready to install: Settings → System.<button aria-label="Dismiss update notice" onClick={() => setUpdateReady(null)}><Icon name="close"/></button></div>}
    {!!syncStatus?.conflicts && <div role="status" className="ui-notice">{syncStatus.conflicts} change{syncStatus.conflicts > 1 ? 's were' : ' was'} made here and online at the same time.<button onClick={() => { setSettingsTab('sync'); setSettingsOpen(true); }}>Review</button></div>}
    {error && <div role="alert" className="ui-error">{error}<button aria-label="Dismiss error" onClick={() => setError('')}><Icon name="close"/></button></div>}
    <div ref={panesRef} className="operator-panes" style={panesStyle}>
      <SessionPane live={live} pending={outputsLoading.projection} serviceRef={serviceRef} services={services} program={program} run={run} persist={persist}
        onOpenSettings={() => setSettingsOpen(true)}
        onLoadService={loadService} onEditService={itemId => void run(() => openService(itemId))} onOpenLibrary={() => void run(openLibrary)}
        onServicesChanged={refreshServices} confirm={confirm} onError={setError} onEditPart={setPartEditor}
        onCue={selectFromSchedule}/>
      {resizeDivider(0)}
      <section className="lyrics-pane" aria-label="Continuous lyrics"><div ref={streamRef} className="pane-scroll lyric-stream">
        <LyricStream items={live?.items} activeId={program?.active_item_id} activeStep={program?.active_step_index} onSelect={cueItem}/>
      </div><NowPlaying items={live?.items ?? []} nextItemId={program?.next_item_id}/></section>
      {resizeDivider(1)}
      <OutputControls display={display} loading={outputsLoading} onDisplay={value => void run(() => invoke('set_display', { display: value }))} onError={setError}/>
    </div>
    {settingsOpen && <OutputSettings config={outputConfig} onChange={setOutputConfig} initialTab={settingsTab} onClose={() => { setSettingsOpen(false); setSettingsTab(undefined); }} extraTabs={[{ id: 'controls', label: 'Controls', content: <ControlsSettings bindings={bindings}/> }, { id: 'remote', label: 'Remote', content: <RemoteSettings/> }, { id: 'sync', label: 'Cloud sync', content: <SyncSettings/> }]}/>}
    {partEditor && <PartEditor part={partEditor} onClose={() => setPartEditor(null)} onSave={part => run(async () => { await editPart(part.itemId, part.occurrence, part.lyrics); setPartEditor(null); })}/>}
    {editor && <SongEditor backgrounds={media} song={editor.song} heading={editor.song ? 'Library song' : 'New library song'} onClose={() => setEditor(null)} onStepped={refreshCatalog} onSave={async song => {
      if (editor.song) await invoke('update_song', { song });
      else await invoke('create_song', { song });
      await refreshCatalog();
      setEditor(null);
    }}/>}
    {serviceEditor && <ServiceEditor service={serviceEditor.service} persist={persist} adopt={adopt} songs={library} media={media} initialItemId={serviceEditor.itemId} activeItemId={program?.active_item_id}
      confirmRemove={outputsShown ? title => confirm(`The outputs are shown and ${title} is on air. Remove it from the service?`, 'Remove') : undefined}
      onCatalogChanged={refreshCatalog} onClose={() => setServiceEditor(null)}/>}
    {quickSearch && <QuickSearch songs={library} media={media} canAdd={!!live} onClose={() => setQuickSearch(false)} onAdd={result => result.kind === 'song' ? addSong(result.song) : addMedia(result.media)}/>}
    {libraryOpen && <LibraryModal songs={library} media={media} confirm={confirm} onClose={() => setLibraryOpen(false)} onCatalogChanged={refreshCatalog} onNewSong={() => setEditor({})} onEditSong={id => void run(() => editLibrarySong(id))}/>}

    {confirmDialog}
  </div></OutputConfigContext.Provider>;
}
export default App;
