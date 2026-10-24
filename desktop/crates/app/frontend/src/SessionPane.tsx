import { useState, type CSSProperties, type MutableRefObject } from 'react';
import { Icon } from './Icons';
import { PresentationCues } from './PresentationEditor';
import ScheduleVideo from './ScheduleVideo';
import { defaultVideoOptions, type ProjectModel, type SessionItem, type VideoOptions } from './models';
import { arrangedParts, itemSong, partCode, partColor, slides } from './song';
import type { ProgramState, ProjectHeader } from './useEngine';
import { CaptureCue } from './CaptureEditor';

type Props = {
  project: ProjectModel | null;
  /** The projection is still loading the current cue's media. */
  pending?: boolean;
  projectRef: MutableRefObject<ProjectModel | null>;
  projects: ProjectHeader[];
  program: ProgramState | null;
  run: (work: () => Promise<unknown>) => Promise<void>;
  persist: (project: ProjectModel) => Promise<void>;
  onOpenSettings: () => void;
  onLoadService: (projectId: number) => void;
  onOpenService: (mode: 'new' | 'edit') => void;
  onOpenLibrary: () => void;
  onNewSong: () => void;
  onEditSong: (item: SessionItem) => void;
  onEditPresentation: (item: SessionItem) => void;
  onEditCapture: (item: SessionItem) => void;
  onEditPart: (part: { itemId: number; partId: string; label: string; lyrics: string }) => void;
  onCue: (item: SessionItem, index?: number) => Promise<unknown>;
  onVideoOptions: (id: number, options: VideoOptions) => void;
};

/** The left column: service choice, library buttons and the service's items with their part cues. */
export default function SessionPane({ project, pending, projectRef, projects, program, run, persist, onOpenSettings, onLoadService, onOpenService, onOpenLibrary, onNewSong, onEditSong, onEditPresentation, onEditCapture, onEditPart, onCue, onVideoOptions }: Props) {
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [dragged, setDragged] = useState<number | null>(null);
  return <section className="session-pane"><div className="operator-settings-bar"><button aria-label="Open output settings" onClick={onOpenSettings}><Icon name="settings"/><span>Settings</span></button></div><div className="session-tools" aria-label="Service and library controls"><div className="service-select-row"><select aria-label="Session / service" value={project?.id || ''} onChange={e => {
    // Capture the choice before awaiting: React restores a controlled select
    // to its current value until the engine publishes the loaded service.
    const projectId = Number(e.currentTarget.value);
    if (!Number.isSafeInteger(projectId) || projectId <= 0) return;
    onLoadService(projectId);
  }}><option value="" disabled>Select a service</option>{projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select><button className="icon-button" aria-label="Edit current service" disabled={!project} onClick={() => onOpenService('edit')}><Icon name="edit"/></button></div><button onClick={() => onOpenService('new')}><Icon name="plus"/> Service</button><button onClick={onOpenLibrary}><Icon name="library"/> Library</button><button className="primary-action" onClick={onNewSong}><Icon name="plus"/> Song</button></div><div className="pane-scroll">
    {!project?.session.items.length && <p className="empty-panel">{project ? 'Add songs from the library to begin.' : 'Create or select a service to begin.'}</p>}
    {project?.session.items.map((item) => <article className={`session-song ${program?.active_item_id === item.id ? `is-current${pending ? ' is-pending' : ''}` : ''}`} key={item.id} title={program?.active_item_id === item.id && pending ? 'Selected; the projection shows it once its media has loaded' : undefined}>
      <header draggable onDragStart={e => { setDragged(item.id); e.dataTransfer.setData('text/plain', String(item.id)); }} onDragEnd={() => setDragged(null)} onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); const current = projectRef.current; if (!current || dragged === null || dragged === item.id) return; const items = [...current.session.items]; const from = items.findIndex(i => i.id === dragged); const to = items.findIndex(i => i.id === item.id); if (from < 0 || to < 0) return; const [moved] = items.splice(from, 1); items.splice(to, 0, moved); void run(() => persist({ ...current, session: { items } })); setDragged(null); }}>
        <button className={`song-collapse ${collapsed.has(item.id) ? 'is-collapsed' : ''}`} aria-label={`${collapsed.has(item.id) ? 'Expand' : 'Collapse'} ${item.title}`} aria-expanded={!collapsed.has(item.id)} onClick={() => setCollapsed(old => { const next = new Set(old); if (next.has(item.id)) next.delete(item.id); else next.add(item.id); return next; })}>
          <span className="drag-handle" title="Drag to reorder"><Icon name="grip"/></span><Icon name="chevron"/><span className="song-name">{item.title || item.kind.kind}</span>
        </button>
        {item.kind.kind === 'video' && <button className="confidence-header-toggle" role="switch" aria-checked={!!item.kind.data.options?.confidence} aria-label={`Show ${item.title || 'video'} on confidence screen`} title="Show video on confidence screen (silent)" onDragStart={e => { e.preventDefault(); e.stopPropagation(); }} onClick={() => { if (item.kind.kind === 'video') onVideoOptions(item.id, { ...defaultVideoOptions, ...item.kind.data.options, confidence: !item.kind.data.options?.confidence }); }}><span className="confidence-toggle-check" aria-hidden="true">{item.kind.data.options?.confidence ? '✓' : ''}</span><span>Confidence</span></button>}
        {item.kind.kind === 'capture' && <button className="song-edit icon-button" aria-label={`Edit screen capture ${item.title}`} title="Edit what is captured and where it shows" onClick={() => onEditCapture(item)}><Icon name="edit"/></button>}
        {(item.kind.kind === 'text' || item.kind.kind === 'presentation') && <button className="song-edit icon-button" aria-label={`Edit ${item.kind.kind === 'presentation' ? 'presentation' : 'song'} ${item.title}`} title={item.kind.kind === 'presentation' ? 'Edit presentation slides' : 'Edit song arrangement and background'} onClick={() => item.kind.kind === 'presentation' ? onEditPresentation(item) : onEditSong(item)}><Icon name="edit"/></button>}
      </header>
      {!collapsed.has(item.id) && item.kind.kind === 'text' && <div className="part-cues" aria-label={`${item.title} arrangement`}>{(() => {
        let offset = 0;
        return arrangedParts(itemSong(item)).map((part, occurrence) => {
          const start = offset;
          const length = slides(part).length;
          offset += length;
          const active = program?.active_item_id === item.id && program.active_step_index != null && program.active_step_index >= start && program.active_step_index < start + length;
          return <div className={`part-cue ${active ? 'is-active' : ''}`} style={{ '--part-color': partColor(part) } as CSSProperties} key={`${part.id}-${occurrence}`}>
            <button className="part-cue-select" aria-label={`Cue ${partCode(part)}, ${item.title}, occurrence ${occurrence + 1}`} aria-pressed={active} disabled={!length} onClick={() => void run(() => onCue(item, start))}>{partCode(part)}</button>
            <button className="part-cue-edit icon-button" aria-label={`Edit ${partCode(part)}, ${item.title}, occurrence ${occurrence + 1}`} onClick={() => onEditPart({ itemId: item.id, partId: part.id, label: `${item.title} · ${part.type} ${part.number}`, lyrics: part.lyrics })}><Icon name="edit"/></button>
          </div>;
        });
      })()}</div>}
      {!collapsed.has(item.id) && item.kind.kind === 'presentation' && <PresentationCues item={item} active={program?.active_item_id === item.id} step={program?.active_step_index} onSelect={index => void run(() => onCue(item, index))}/> }
      {!collapsed.has(item.id) && item.kind.kind === 'audio' && <button className="schedule-audio-cue" onClick={() => void run(() => onCue(item))}><Icon name="audio"/>Audio<Icon name="live"/></button>}
      {!collapsed.has(item.id) && item.kind.kind === 'capture' && <CaptureCue item={item} active={program?.active_item_id === item.id} onSelect={() => void run(() => onCue(item))}/>}
      {!collapsed.has(item.id) && item.kind.kind === 'video' && <ScheduleVideo title={item.title || 'Untitled video'} path={item.kind.data.path} active={program?.active_item_id === item.id} onSelect={() => void run(() => onCue(item))}/>}
    </article>)}
  </div></section>;
}
