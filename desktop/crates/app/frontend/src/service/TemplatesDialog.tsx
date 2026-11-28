import { useState } from 'react';
import { Icon } from '../ui/Icons';
import { newId, type ServiceItem, type ServiceTemplate, type TemplatePart } from '../api/models';

type Props = {
  templates: ServiceTemplate[];
  initialId?: string;
  describe: (item: ServiceItem) => string;
  onSave: (template: ServiceTemplate) => Promise<void>;
  onDelete: (template: ServiceTemplate) => Promise<void>;
  onClose: () => void;
};

const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const clock = (minutes?: number) => minutes === undefined ? '' : `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
const minutesOf = (value: string) => value ? Number(value.slice(0, 2)) * 60 + Number(value.slice(3, 5)) : undefined;

export default function TemplatesDialog({ templates, initialId, describe, onSave, onDelete, onClose }: Props) {
  const [selected, setSelected] = useState(initialId ?? templates[0]?.id ?? '');
  const [edited, setEdited] = useState<ServiceTemplate | null>(null);
  const draft = edited?.id === selected ? edited : templates.find(t => t.id === selected) ?? null;
  const setDraft = setEdited;
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [dragged, setDragged] = useState<string | null>(null);
  const commit = (next: ServiceTemplate) => { setDraft(next); void onSave(next); };
  const setPart = (id: string, patch: Partial<TemplatePart>) => draft && commit({ ...draft, parts: draft.parts.map(p => p.id === id ? { ...p, ...patch } : p) });
  const moveTo = (id: string, target: string) => {
    if (!draft || id === target) return;
    const parts = draft.parts.filter(p => p.id !== id);
    const moving = draft.parts.find(p => p.id === id);
    if (!moving) return;
    parts.splice(parts.findIndex(p => p.id === target) + (draft.parts.findIndex(p => p.id === id) < draft.parts.findIndex(p => p.id === target) ? 1 : 0), 0, moving);
    commit({ ...draft, parts });
  };
  const toggleOpen = (id: string) => { const next = new Set(open); if (next.has(id)) next.delete(id); else next.add(id); setOpen(next); };
  async function create() {
    const template: ServiceTemplate = { id: newId(), name: 'New template', parts: [{ id: newId(), title: 'Welcoming', items: [] }] };
    await onSave(template);
    setSelected(template.id);
  }

  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}>
    <section className="compact-modal templates-modal" role="dialog" aria-modal="true" aria-label="Service templates">
      <header>Service templates<button className="icon-button" aria-label="Close" onClick={onClose}><Icon name="close"/></button></header>
      <div className="template-picker">
        <select aria-label="Template" value={selected} onChange={e => setSelected(e.target.value)}>
          {templates.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
        </select>
        <button onClick={() => void create()}><Icon name="plus"/>New template</button>
      </div>
      {!draft ? <p className="field-hint">No templates yet. A template is the usual order of a service; each part becomes a section.</p> : <>
        <div className="settings-fields template-fields">
          <label>Template name<input value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} onBlur={() => commit(draft)}/></label>
          <label>Service name<input placeholder={draft.name} value={draft.title ?? ''} onChange={e => setDraft({ ...draft, title: e.target.value || undefined })} onBlur={() => commit(draft)}/></label>
          <label>Usual day<select value={draft.weekday ?? ''} onChange={e => commit({ ...draft, weekday: e.target.value === '' ? undefined : Number(e.target.value) })}>
            <option value="">Any day</option>{DAYS.map((d, i) => <option key={d} value={i}>{d}</option>)}
          </select></label>
          <label>Usual time<input type="time" value={clock(draft.startMinutes)} onChange={e => commit({ ...draft, startMinutes: minutesOf(e.target.value) })}/></label>
        </div>
        <ol className="template-parts-list" aria-label="Parts">
          {draft.parts.map((part, index) => <li key={part.id} className={`template-part ${dragged === part.id ? 'is-dragging' : ''}`} draggable
            onDragStart={e => { setDragged(part.id); e.dataTransfer.effectAllowed = 'move'; }} onDragEnd={() => setDragged(null)}
            onDragOver={e => { if (dragged) e.preventDefault(); }} onDrop={e => { e.preventDefault(); if (dragged) moveTo(dragged, part.id); setDragged(null); }}>
            <div className="template-part-row">
              <Icon name="grip" className="drag-handle"/>
              <button className="icon-button" aria-label={open.has(part.id) ? `Hide items of ${part.title}` : `Show items of ${part.title}`} aria-expanded={open.has(part.id)} onClick={() => toggleOpen(part.id)} disabled={!part.items.length}><Icon name={open.has(part.id) ? 'chevron' : 'right'}/></button>
              <input aria-label="Part name" value={part.title} onChange={e => setDraft({ ...draft, parts: draft.parts.map(p => p.id === part.id ? { ...p, title: e.target.value } : p) })} onBlur={() => commit(draft)}/>
              <small>{part.items.length ? `${part.items.length} item${part.items.length > 1 ? 's' : ''}` : 'empty'}</small>
              <label className="setting-check" title="Optional parts start unticked when you plan a service"><input type="checkbox" checked={!!part.optional} onChange={e => setPart(part.id, { optional: e.target.checked || undefined })}/>Optional</label>
              <button className="icon-button" aria-label={`Move ${part.title} up`} disabled={index === 0} onClick={() => moveTo(part.id, draft.parts[index - 1].id)}><Icon name="up"/></button>
              <button className="icon-button" aria-label={`Move ${part.title} down`} disabled={index === draft.parts.length - 1} onClick={() => moveTo(part.id, draft.parts[index + 1].id)}><Icon name="down"/></button>
              <button className="icon-button" aria-label={`Remove ${part.title}`} onClick={() => commit({ ...draft, parts: draft.parts.filter(p => p.id !== part.id) })}><Icon name="close"/></button>
            </div>
            {open.has(part.id) && <ul className="template-part-items">{part.items.map(item => <li key={item.id}><span>{describe(item)}</span><button className="icon-button" aria-label={`Remove ${describe(item)} from the template`} onClick={() => setPart(part.id, { items: part.items.filter(i => i.id !== item.id) })}><Icon name="close"/></button></li>)}</ul>}
          </li>)}
        </ol>
        <div className="modal-actions">
          <button className="danger-action" onClick={() => void onDelete(draft).then(() => setSelected(templates.find(t => t.id !== draft.id)?.id ?? ''))}>Delete template</button>
          <span className="grow"/>
          <button onClick={() => commit({ ...draft, parts: [...draft.parts, { id: newId(), title: 'New part', items: [] }] })}><Icon name="plus"/>Add part</button>
          <button className="primary-action" onClick={onClose}>Done</button>
        </div>
        <p className="field-hint">To give a part fixed items, like a countdown video or an offering slide, build a service with them and use “Save as template” from the service menu.</p>
      </>}
    </section>
  </div>;
}
