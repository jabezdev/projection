import { useState } from 'react';
import type { Service, ServiceTemplate } from '../api/models';

type Props = { service: Service; templates: ServiceTemplate[]; onSave: (templateId: string | null, name: string, keepSongs: boolean) => Promise<void>; onClose: () => void };

export default function SaveTemplateDialog({ service, templates, onSave, onClose }: Props) {
  const [target, setTarget] = useState('');
  const [name, setName] = useState(service.title);
  const [keepSongs, setKeepSongs] = useState(false);
  const replacing = templates.find(t => t.id === target);
  const sections = service.items.filter(i => i.content.kind === 'header').length;
  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}>
    <section className="compact-modal" role="dialog" aria-modal="true" aria-label="Save as template">
      <header>Save as template</header>
      <div className="settings-fields new-service-fields">
        <label>Save to<select value={target} onChange={e => { setTarget(e.target.value); const t = templates.find(x => x.id === e.target.value); if (t) setName(t.name); }}>
          <option value="">A new template</option>
          {templates.map(t => <option key={t.id} value={t.id}>Replace “{t.name}”</option>)}
        </select></label>
        <label>Template name<input autoFocus value={name} onChange={e => setName(e.target.value)}/></label>
        <label className="setting-check wide"><input type="checkbox" checked={keepSongs} onChange={e => setKeepSongs(e.target.checked)}/>Keep the songs too</label>
        <small className="wide">Each of the {sections} section{sections === 1 ? '' : 's'} becomes a part, with its videos, slides and text. {keepSongs ? 'Songs are kept as well.' : 'Songs are left out, since they change every week.'}{replacing ? ` The parts of “${replacing.name}” are replaced.` : ''}</small>
      </div>
      <div className="modal-actions"><button onClick={onClose}>Cancel</button><button className="primary-action" onClick={() => void onSave(target || null, name, keepSongs)}>Save template</button></div>
    </section>
  </div>;
}
