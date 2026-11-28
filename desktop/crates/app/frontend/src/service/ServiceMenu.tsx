import { useEffect, useRef, useState } from 'react';
import { Icon } from '../ui/Icons';
import { invoke } from '../api/tauri';
import { isPast, type LiveService, type MediaFile, type Service, type Song } from '../api/models';
import { ItemThumb } from '../library/Thumbs';
import { catalog, resolveItem, type Catalog } from '../api/resolve';
import { copyItem } from './serviceItems';
import { useTemplateFlows } from './useTemplateFlows';

type Props = {
  live: LiveService | null;
  services: Service[];
  persist: (service: Service) => Promise<void>;
  onLoad: (serviceId: string) => Promise<boolean>;
  onEdit: () => void;
  onListChanged: () => Promise<void>;
  confirm: (message: string, action: string) => Promise<boolean>;
  onError: (message: string) => void;
};

const when = (ms: number) => new Date(ms).toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });

export default function ServiceMenu({ live, services, persist, onLoad, onEdit, onListChanged, confirm, onError }: Props) {
  const project = live?.service ?? null;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [menu, setMenu] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [library, setLibrary] = useState<Catalog | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const flows = useTemplateFlows({ onError, onCreated: async created => { await onListChanged(); if (await onLoad(created.id)) { close(); onEdit(); } } });
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) close(); };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); if (renaming) setRenaming(null); else close(); } };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape, true);
    return () => { document.removeEventListener('pointerdown', away); document.removeEventListener('keydown', escape, true); };
  });
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    void Promise.all([invoke<Song[]>('list_songs', { includeArchived: true }), invoke<MediaFile[]>('list_media')])
      .then(([songs, media]) => { if (!disposed) setLibrary(catalog(songs, media)); }).catch(() => {});
    return () => { disposed = true; };
  }, [open]);
  function close() { setOpen(false); setMenu(null); setRenaming(null); setQuery(''); }
  async function act(work: () => Promise<void>) { try { await work(); } catch (e) { onError(String(e)); } }
  async function saveService(id: string, change: (s: Service) => Service) {
    if (project?.id === id) await persist(change(project));
    else await invoke('save_service', { service: change(await invoke<Service>('get_service', { serviceId: id })) });
    await onListChanged();
  }
  async function create(title: string, from: Service | null) {
    const created = await invoke<Service>('create_service', { title, scheduledAtMs: Date.now() });
    const items = (from?.items ?? []).map(copyItem).map(item => item.content.kind === 'song' ? { ...item, content: { ...item.content, versionId: undefined } } : item);
    if (items.length) await invoke('save_service', { service: { ...created, items } });
    await onListChanged();
    return created;
  }
  const q = query.trim().toLowerCase();
  const list = services.filter(p => !q || p.title.toLowerCase().includes(q));
  return <div className="service-menu" ref={root}>
    <div className="service-selector">
      <button className="service-name" aria-haspopup="dialog" aria-expanded={open} onClick={() => open ? close() : setOpen(true)}>
        <span>{project?.title || 'Select a service'}</span><Icon name="chevron"/>
      </button>
      <button className="service-edit" aria-label="Edit service" title="Edit service" disabled={!project} onClick={() => { close(); onEdit(); }}><Icon name="edit"/></button>
    </div>
    {open && <div className="service-popover" role="dialog" aria-label="Services">
      <input autoFocus aria-label="Find a service" placeholder="Find a service…" value={query} onChange={e => setQuery(e.target.value)}/>
      <div className="service-list">
        {list.map(p => { const first = p.items[0]; const cover = first && library ? resolveItem(first, library) : null; return <div key={p.id} className={`service-row ${p.id === project?.id ? 'is-current' : ''}`}>
          {renaming?.id === p.id
            ? <form className="service-rename" onSubmit={e => { e.preventDefault(); const title = renaming.name.trim(); if (!title) return; setRenaming(null); void act(() => saveService(p.id, s => ({ ...s, title }))); }}>
                <input autoFocus aria-label="Service name" value={renaming.name} onChange={e => setRenaming({ id: p.id, name: e.target.value })} onBlur={() => setRenaming(null)}/>
              </form>
            : <button className="service-row-main" onClick={() => { if (p.id === project?.id) { close(); return; } void act(async () => { if (await onLoad(p.id)) close(); }); }}>
                {cover ? <ItemThumb item={cover}/> : <span className="thumb"><span className="thumb-tile">Empty</span></span>}
                <span className="service-row-text"><strong>{p.title}</strong><small>{when(p.scheduledAtMs)} · {p.items.length} items{isPast(p) ? ' · past' : ''}{p.id === project?.id ? ' · loaded' : ''}</small></span>
              </button>}
          <button className="icon-button service-row-more" aria-label={`More for ${p.title}`} aria-haspopup="menu" aria-expanded={menu === p.id} onClick={() => setMenu(menu === p.id ? null : p.id)}><Icon name="more"/></button>
          {menu === p.id && <div className="service-row-menu" role="menu">
            <button role="menuitem" onClick={() => { setMenu(null); setRenaming({ id: p.id, name: p.title }); }}>Rename</button>
            <button role="menuitem" onClick={() => { setMenu(null); void act(async () => { await create(`${p.title} (copy)`, p); }); }}>Duplicate</button>
            <button role="menuitem" onClick={() => { close(); flows.saveAsTemplate(p); }}>Save as template…</button>
            {!isPast(p) && <button role="menuitem" onClick={() => { setMenu(null); void act(async () => { if (!(await confirm(`End “${p.title}”? Its songs keep the words they have now, even if the library changes later.`, 'End service'))) return; await invoke('end_service', { serviceId: p.id }); await onListChanged(); }); }}>End service…</button>}
            <button role="menuitem" className="danger" disabled={p.id === project?.id} title={p.id === project?.id ? 'Load another service first' : undefined} onClick={() => { setMenu(null); void act(async () => { if (!(await confirm(`Delete “${p.title}”? This cannot be undone.`, 'Delete'))) return; await invoke('delete_service', { serviceId: p.id }); await onListChanged(); }); }}>Delete…</button>
          </div>}
        </div>; })}
        {!list.length && <p className="empty-panel">{q ? 'No service matches.' : 'No services yet.'}</p>}
      </div>
      <div className="service-popover-foot">
        <button onClick={() => { close(); flows.newService(); }}><Icon name="plus"/>New service…</button>
        <button disabled={!project} onClick={() => void act(async () => { if (!project) return; const created = await create(`${project.title} (copy)`, project); if (await onLoad(created.id)) { close(); onEdit(); } })}><Icon name="copy"/>Copy of this one</button>
        <button onClick={() => { close(); flows.editTemplates(); }}><Icon name="template"/>Templates…</button>
      </div>
    </div>}
    {flows.dialogs}
  </div>;
}
