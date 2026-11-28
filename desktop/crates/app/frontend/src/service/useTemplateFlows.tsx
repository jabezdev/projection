import { useState, type ReactNode } from 'react';
import { invoke } from '../api/tauri';
import type { MediaFile, Service, ServiceItem, ServiceTemplate, Song } from '../api/models';
import NewServiceDialog from './NewServiceDialog';
import TemplatesDialog from './TemplatesDialog';
import SaveTemplateDialog from './SaveTemplateDialog';

type Options = { onCreated: (service: Service) => Promise<void>; onError: (message: string) => void };
type Open = { kind: 'new' } | { kind: 'edit'; id?: string } | { kind: 'save'; service: Service } | null;

export function useTemplateFlows({ onCreated, onError }: Options): { dialogs: ReactNode; newService: () => void; editTemplates: () => void; saveAsTemplate: (service: Service) => void } {
  const [open, setOpen] = useState<Open>(null);
  const [templates, setTemplates] = useState<ServiceTemplate[]>([]);
  const [names, setNames] = useState<{ songs: Song[]; media: MediaFile[] }>({ songs: [], media: [] });
  const load = async () => setTemplates(await invoke<ServiceTemplate[]>('list_templates'));
  const show = (next: Open) => void load().then(async () => {
    if (next?.kind === 'edit') setNames({ songs: await invoke<Song[]>('list_songs', { includeArchived: true }), media: await invoke<MediaFile[]>('list_media') });
    setOpen(next);
  }).catch(e => onError(String(e)));
  const act = (work: () => Promise<void>) => work().catch(e => onError(String(e)));
  const describe = (item: ServiceItem) => {
    if (item.title) return item.title;
    const c = item.content;
    if (c.kind === 'song') return names.songs.find(s => s.id === c.songId)?.title ?? 'A song';
    if (c.kind === 'media') return names.media.find(m => m.id === c.mediaId)?.title ?? 'A media file';
    if (c.kind === 'text') return c.steps[0]?.split('\n')[0] || 'Text';
    return c.kind === 'capture' ? 'Screen capture' : c.kind === 'presentation' ? 'Slides' : 'Item';
  };
  const dialogs = <>
    {open?.kind === 'new' && <NewServiceDialog templates={templates} onClose={() => setOpen(null)} onEditTemplates={() => show({ kind: 'edit' })}
      onCreate={(templateId, parts, title, scheduledAtMs) => act(async () => {
        const service = await invoke<Service>('create_service_from_template', { templateId, parts, title, scheduledAtMs });
        setOpen(null);
        await onCreated(service);
      })}/>}
    {open?.kind === 'edit' && <TemplatesDialog templates={templates} initialId={open.id} describe={describe} onClose={() => setOpen(null)}
      onSave={template => act(async () => { await invoke('save_template', { template }); await load(); })}
      onDelete={template => act(async () => { await invoke('delete_template', { templateId: template.id }); await load(); })}/>}
    {open?.kind === 'save' && <SaveTemplateDialog service={open.service} templates={templates} onClose={() => setOpen(null)}
      onSave={(templateId, name, keepSongs) => act(async () => {
        const saved = await invoke<ServiceTemplate>('save_service_as_template', { serviceId: open.service.id, templateId, name, keepSongs });
        await load();
        setOpen({ kind: 'edit', id: saved.id });
      })}/>}
  </>;
  return { dialogs, newService: () => show({ kind: 'new' }), editTemplates: () => show({ kind: 'edit' }), saveAsTemplate: service => show({ kind: 'save', service }) };
}
