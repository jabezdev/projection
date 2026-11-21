import { useState, type CSSProperties, type DragEvent, type MutableRefObject } from 'react';
import { Icon } from '../ui/Icons';
import type { LiveService, Service, SessionItem } from '../api/models';
import { occurrences, sectionCode, sectionColor, sectionLabel } from '../song/song';
import type { ProgramState } from '../api/useEngine';
import ServiceMenu from './ServiceMenu';
import { ItemThumb } from '../library/Thumbs';
import { Background } from '../song/SlidePreview';

type Props = {
  live: LiveService | null;
  pending?: boolean;
  serviceRef: MutableRefObject<Service | null>;
  services: Service[];
  program: ProgramState | null;
  run: (work: () => Promise<unknown>) => Promise<void>;
  persist: (service: Service) => Promise<void>;
  onOpenSettings: () => void;
  onLoadService: (serviceId: string) => Promise<boolean>;
  onEditService: (itemId?: string) => void;
  onOpenLibrary: () => void;
  onServicesChanged: () => Promise<void>;
  confirm: (message: string, action: string) => Promise<boolean>;
  onError: (message: string) => void;
  onEditPart: (part: { itemId: string; occurrence: number; label: string; lyrics: string }) => void;
  onCue: (item: SessionItem, index?: number) => Promise<unknown>;
};
type Group = { section: SessionItem | null; items: SessionItem[] };

function groupItems(items: SessionItem[]): Group[] {
  const groups: Group[] = [{ section: null, items: [] }];
  for (const item of items) {
    if (item.kind.kind === 'section') groups.push({ section: item, items: [] });
    else groups[groups.length - 1].items.push(item);
  }
  return groups.filter(g => g.section || g.items.length);
}

export default function SessionPane({ live, pending, serviceRef, services, program, run, persist, onOpenSettings, onLoadService, onEditService, onOpenLibrary, onServicesChanged, confirm, onError, onEditPart, onCue }: Props) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dragged, setDragged] = useState<string | null>(null);
  const [drop, setDrop] = useState<{ id: string; before: boolean } | null>(null);
  const toggle = (id: string) => setCollapsed(old => { const next = new Set(old); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  const activeId = program?.active_item_id;

  function move(targetId: string, before: boolean) {
    const current = serviceRef.current;
    if (!current || dragged === null || dragged === targetId) return;
    const items = current.items;
    const header = (i: number, list = items) => list[i].content.kind === 'header';
    const from = items.findIndex(i => i.id === dragged);
    if (from < 0) return;
    let end = from + 1;
    if (header(from)) while (end < items.length && !header(end)) end++;
    const block = items.slice(from, end);
    if (block.some(i => i.id === targetId)) return;
    const rest = [...items.slice(0, from), ...items.slice(end)];
    let at = rest.findIndex(i => i.id === targetId);
    if (at < 0) return;
    if (!before) {
      at++;
      if (header(at - 1, rest) && block[0].content.kind === 'header') while (at < rest.length && !header(at, rest)) at++;
    }
    void run(() => persist({ ...current, items: [...rest.slice(0, at), ...block, ...rest.slice(at)] }));
  }
  const dragProps = (id: string) => ({
    draggable: true,
    onDragStart: (e: DragEvent) => { setDragged(id); e.dataTransfer.setData('text/plain', String(id)); e.dataTransfer.effectAllowed = 'move'; },
    onDragEnd: () => { setDragged(null); setDrop(null); },
    onDragOver: (e: DragEvent<HTMLElement>) => { if (dragged === null) return; e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); setDrop({ id, before: e.clientY < r.top + r.height / 2 }); },
    onDrop: (e: DragEvent) => { e.preventDefault(); if (drop) move(drop.id, drop.before); setDragged(null); setDrop(null); },
  });
  const dropClass = (id: string) => drop?.id === id ? (drop.before ? ' drop-above' : ' drop-below') : '';

  return <section className="session-pane">
    <ServiceMenu live={live} services={services} persist={persist} onLoad={onLoadService} onEdit={() => onEditService()} onListChanged={onServicesChanged} confirm={confirm} onError={onError}/>
    <div className="pane-scroll run-list">
      {!live?.items.length && <p className="empty-panel">{live ? 'This service is empty. Add items with the pencil beside its name.' : 'Choose or create a service above.'}</p>}
      {live && groupItems(live.items).map(group => {
        const section = group.section;
        const hidden = section ? collapsed.has(section.id) : false;
        const holdsLive = group.items.some(i => i.id === activeId);
        return <div className={`run-group ${section ? 'has-section' : ''}`} key={section?.id ?? 'loose'}>
          {section && <header className={`run-section${dropClass(section.id)}${dragged === section.id ? ' is-dragging' : ''}`} {...dragProps(section.id)}>
            <button className="run-section-toggle" aria-expanded={!hidden} onClick={() => toggle(section.id)}>
              <Icon name="chevron"/><span>{section.title || 'Section'}</span><small className="tabular">{group.items.length}</small>
              {hidden && holdsLive && <span className="lamp" title="An item in this section is on air"/>}
            </button>
            <button className="icon-button run-edit" aria-label={`Edit section ${section.title}`} title="Rename in the service editor" onClick={() => onEditService(section.id)}><Icon name="edit"/></button>
          </header>}
          {!hidden && <div className="run-items">{group.items.map(item => <RunItem key={item.id} item={item} active={item.id === activeId} pending={!!pending && item.id === activeId}
            step={item.id === activeId ? program?.active_step_index : null} dragging={dragged === item.id} dropClass={dropClass(item.id)} dragProps={dragProps(item.id)}
            onCue={index => void run(() => onCue(item, index))} onEdit={() => onEditService(item.id)} onEditPart={onEditPart}/>)}</div>}
        </div>;
      })}
    </div>
    <div className="session-foot"><button onClick={onOpenSettings} aria-label="Open settings"><Icon name="settings"/>Settings</button><button onClick={onOpenLibrary}><Icon name="library"/>Library</button></div>
  </section>;
}

function RunItem({ item, active, pending, step, dragging, dropClass, dragProps, onCue, onEdit, onEditPart }: {
  item: SessionItem; active: boolean; pending: boolean; step?: number | null; dragging: boolean; dropClass: string;
  dragProps: ReturnType<() => Record<string, unknown>>; onCue: (index?: number) => void; onEdit: () => void; onEditPart: Props['onEditPart'];
}) {
  const kind = item.kind;
  return <article className={`run-item${active ? ' is-current' : ''}${pending ? ' is-pending' : ''}${dragging ? ' is-dragging' : ''}${dropClass}`} title={pending ? 'Selected; the projection shows it once its media has loaded' : undefined} {...dragProps}>
    <div className="run-item-head">
      <button className="run-item-cue" aria-label={`Put ${item.title || kind.kind} on air`} onClick={() => onCue(0)}>
        <ItemThumb item={item}/><strong>{item.title || kind.kind}</strong>
      </button>
      {active && <span className="lamp" aria-label="On air"/>}
      <button className="icon-button run-edit" aria-label={`Edit ${item.title || 'item'}`} title="Edit in the service editor" onClick={onEdit}><Icon name="edit"/></button>
    </div>
    {kind.kind === 'text' && <div className="part-keys" aria-label={`${item.title} parts`}>{occurrences(item).map(({ key, section, lines, start }, occurrence) => {
      const on = active && step != null && step >= start && step < start + lines.length;
      const code = sectionCode(section);
      return <span className="part-key-wrap" key={key}>
        <button className={`part-key${on ? ' is-active' : ''}`} style={{ '--part-color': sectionColor(section) } as CSSProperties} aria-pressed={on} disabled={!lines.length}
          aria-label={`Cue ${code}, ${item.title}, occurrence ${occurrence + 1}`} onClick={() => onCue(start)}
          onContextMenu={e => { if (!item.song) return; e.preventDefault(); onEditPart({ itemId: item.id, occurrence, label: `${item.title} · ${sectionLabel(section)}`, lyrics: section.lyrics }); }}
          title={`${sectionLabel(section)}${item.song ? ' · right-click to edit the words' : ''}`}>{code}</button>
      </span>;
    })}</div>}
    {kind.kind === 'presentation' && <div className="slide-keys">{(kind.data.slides || [kind.data.path]).map((path, index) => <button key={index} className={`slide-key${active && step === index ? ' is-active' : ''}`} aria-pressed={active && step === index} aria-label={`Cue slide ${index + 1} of ${item.title}`} onClick={() => onCue(index)}>
      <span className="thumb"><Background path={path} fit="contain"/></span><span className="slide-key-number tabular">{index + 1}</span>
    </button>)}</div>}
  </article>;
}
