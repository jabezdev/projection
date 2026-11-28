import { useState } from 'react';
import type { ServiceTemplate } from '../api/models';
import { defaultParts, nextOccurrence } from './templates';

type Props = {
  templates: ServiceTemplate[];
  onCreate: (templateId: string | null, parts: string[], title: string, scheduledAtMs: number) => Promise<void>;
  onEditTemplates: () => void;
  onClose: () => void;
};

const localInput = (ms: number) => { const d = new Date(ms); d.setMinutes(d.getMinutes() - d.getTimezoneOffset()); return d.toISOString().slice(0, 16); };

export default function NewServiceDialog({ templates, onCreate, onEditTemplates, onClose }: Props) {
  const [templateId, setTemplateId] = useState<string>(templates[0]?.id ?? '');
  const template = templates.find(t => t.id === templateId) ?? null;
  const [parts, setParts] = useState<string[]>(template ? defaultParts(template) : []);
  const [title, setTitle] = useState(template?.title ?? template?.name ?? 'Service');
  const [when, setWhen] = useState(() => localInput(template ? nextOccurrence(template) : Date.now()));
  const [busy, setBusy] = useState(false);
  const pick = (id: string) => {
    const next = templates.find(t => t.id === id) ?? null;
    setTemplateId(id); setParts(next ? defaultParts(next) : []);
    setTitle(next?.title ?? next?.name ?? 'Service');
    setWhen(localInput(next ? nextOccurrence(next) : Date.now()));
  };
  const toggle = (id: string) => setParts(parts.includes(id) ? parts.filter(p => p !== id) : [...parts, id]);
  async function create() {
    const ms = new Date(when).getTime();
    if (!Number.isFinite(ms) || busy) return;
    setBusy(true);
    try { await onCreate(template?.id ?? null, template ? template.parts.filter(p => parts.includes(p.id)).map(p => p.id) : [], title, ms); }
    finally { setBusy(false); }
  }
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}>
    <section className="compact-modal new-service-modal" role="dialog" aria-modal="true" aria-label="New service">
      <header>New service<button className="icon-button" aria-label="Close" onClick={onClose}>✕</button></header>
      <div className="settings-fields new-service-fields">
        <label>Start from<select autoFocus value={templateId} onChange={e => pick(e.target.value)}>
          {templates.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
          <option value="">Blank service</option>
        </select></label>
        <label>Date and time<input type="datetime-local" value={when} onChange={e => setWhen(e.target.value)}/></label>
        <label className="wide">Name<input value={title} onChange={e => setTitle(e.target.value)}/></label>
        {template && <fieldset className="wide template-parts"><legend>Parts this week</legend>
          {template.parts.map(part => <label key={part.id} className="setting-check"><input type="checkbox" checked={parts.includes(part.id)} onChange={() => toggle(part.id)}/>{part.title}{part.items.length > 0 && <small>{part.items.length} item{part.items.length > 1 ? 's' : ''}</small>}</label>)}
          <small>Untick what isn't happening this week. You can still add, remove and move anything later.</small>
        </fieldset>}
      </div>
      <div className="modal-actions"><button onClick={onEditTemplates}>Edit templates…</button><span className="grow"/><button onClick={onClose}>Cancel</button><button className="primary-action" disabled={busy} onClick={() => void create()}>Create</button></div>
    </section>
  </div>;
}
