import { useEffect, useRef, useState } from 'react';
import { Icon } from '../ui/Icons';
import { invoke } from '../api/tauri';
import { newItemId, type ProjectModel } from '../api/models';
import type { ProjectHeader } from '../api/useEngine';
import { ItemThumb } from '../library/Thumbs';

type Props = {
  project: ProjectModel | null;
  projects: ProjectHeader[];
  /** Saves the loaded service (its rename goes through the engine). */
  persist: (project: ProjectModel) => Promise<void>;
  onLoad: (projectId: number) => Promise<boolean>;
  onEdit: () => void;
  onListChanged: () => Promise<void>;
  confirm: (message: string, action: string) => Promise<boolean>;
  onError: (message: string) => void;
};

/** The service selector: the loaded service's name, a list to switch or manage services, and the edit button. */
export default function ServiceMenu({ project, projects, persist, onLoad, onEdit, onListChanged, confirm, onError }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [menu, setMenu] = useState<number | null>(null);
  const [renaming, setRenaming] = useState<{ id: number; name: string } | null>(null);
  const [covers, setCovers] = useState<Map<number, ProjectModel>>(new Map());
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) close(); };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); if (renaming) setRenaming(null); else close(); } };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape, true);
    return () => { document.removeEventListener('pointerdown', away); document.removeEventListener('keydown', escape, true); };
  });
  // Each service's first item is its cover.
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    void Promise.all(projects.map(p => invoke<ProjectModel>('get_project', { projectId: p.id }).catch(() => null)))
      .then(all => { if (!disposed) setCovers(new Map(all.filter((p): p is ProjectModel => !!p).map(p => [p.id, p]))); });
    return () => { disposed = true; };
  }, [open, projects]);
  function close() { setOpen(false); setMenu(null); setRenaming(null); setQuery(''); }
  async function act(work: () => Promise<void>) { try { await work(); } catch (e) { onError(String(e)); } }
  async function saveService(id: number, change: (p: ProjectModel) => ProjectModel) {
    if (project?.id === id) await persist(change(project));
    else await invoke('save_project', { project: change(await invoke<ProjectModel>('get_project', { projectId: id })) });
    await onListChanged();
  }
  async function create(name: string, from: ProjectModel | null) {
    const created = await invoke<ProjectModel>('create_project', { name });
    const items = (from?.session.items ?? []).map(item => ({ ...structuredClone(item), id: newItemId() }));
    if (items.length) await invoke('save_project', { project: { ...created, session: { items } } });
    await onListChanged();
    return created;
  }
  const q = query.trim().toLowerCase();
  const list = projects.filter(p => !q || p.name.toLowerCase().includes(q));
  return <div className="service-menu" ref={root}>
    <div className="service-selector">
      <button className="service-name" aria-haspopup="dialog" aria-expanded={open} onClick={() => open ? close() : setOpen(true)}>
        <span>{project?.name || 'Select a service'}</span><Icon name="chevron"/>
      </button>
      <button className="service-edit" aria-label="Edit service" title="Edit service" disabled={!project} onClick={() => { close(); onEdit(); }}><Icon name="edit"/></button>
    </div>
    {open && <div className="service-popover" role="dialog" aria-label="Services">
      <input autoFocus aria-label="Find a service" placeholder="Find a service…" value={query} onChange={e => setQuery(e.target.value)}/>
      <div className="service-list">
        {list.map(p => { const cover = covers.get(p.id)?.session.items[0]; return <div key={p.id} className={`service-row ${p.id === project?.id ? 'is-current' : ''}`}>
          {renaming?.id === p.id
            ? <form className="service-rename" onSubmit={e => { e.preventDefault(); const name = renaming.name.trim(); if (!name) return; setRenaming(null); void act(() => saveService(p.id, s => ({ ...s, name }))); }}>
                <input autoFocus aria-label="Service name" value={renaming.name} onChange={e => setRenaming({ id: p.id, name: e.target.value })} onBlur={() => setRenaming(null)}/>
              </form>
            : <button className="service-row-main" onClick={() => { if (p.id === project?.id) { close(); return; } void act(async () => { if (await onLoad(p.id)) close(); }); }}>
                {cover ? <ItemThumb item={cover}/> : <span className="thumb"><span className="thumb-tile">Empty</span></span>}
                <span className="service-row-text"><strong>{p.name}</strong><small>{covers.get(p.id) ? `${covers.get(p.id)!.session.items.length} items` : ''}{p.id === project?.id ? ' · loaded' : ''}</small></span>
              </button>}
          <button className="icon-button service-row-more" aria-label={`More for ${p.name}`} aria-haspopup="menu" aria-expanded={menu === p.id} onClick={() => setMenu(menu === p.id ? null : p.id)}><Icon name="more"/></button>
          {menu === p.id && <div className="service-row-menu" role="menu">
            <button role="menuitem" onClick={() => { setMenu(null); setRenaming({ id: p.id, name: p.name }); }}>Rename</button>
            <button role="menuitem" onClick={() => { setMenu(null); void act(async () => { const source = covers.get(p.id) ?? await invoke<ProjectModel>('get_project', { projectId: p.id }); await create(`${p.name} (copy)`, source); }); }}>Duplicate</button>
            <button role="menuitem" className="danger" disabled={p.id === project?.id} title={p.id === project?.id ? 'Load another service first' : undefined} onClick={() => { setMenu(null); void act(async () => { if (!(await confirm(`Delete “${p.name}”? This cannot be undone.`, 'Delete'))) return; await invoke('delete_project', { projectId: p.id }); await onListChanged(); }); }}>Delete…</button>
          </div>}
        </div>; })}
        {!list.length && <p className="empty-panel">{q ? 'No service matches.' : 'No services yet.'}</p>}
      </div>
      <div className="service-popover-foot">
        <button onClick={() => void act(async () => { const created = await create('New service', null); if (await onLoad(created.id)) { close(); onEdit(); } })}><Icon name="plus"/>New service</button>
        <button disabled={!project} onClick={() => void act(async () => { if (!project) return; const created = await create(`${project.name} (copy)`, project); if (await onLoad(created.id)) { close(); onEdit(); } })}><Icon name="copy"/>Copy of this one</button>
      </div>
    </div>}
  </div>;
}
