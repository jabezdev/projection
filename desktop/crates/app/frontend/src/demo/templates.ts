import sunday from '../../../../../fixtures/sunday-template.json';
import { newId, type Service, type ServiceTemplate } from '../api/models';
import { partItems, templateFromService } from '../service/templates';
import type { UiOnlyStore } from './types';

const OFFLINE_DEMO = { paired: false, offline: false, state: 'off', pending: 0, conflicts: 0, uploads: 0, downloads: 0 };

function templates(store: UiOnlyStore): Record<string, ServiceTemplate> {
  store.templates ??= { [sunday.id]: structuredClone(sunday) as ServiceTemplate };
  return store.templates;
}

export function runTemplateCommand(store: UiOnlyStore, command: string, args: Record<string, unknown> = {}): { done: boolean; value?: unknown } {
  const all = templates(store);
  switch (command) {
    case 'list_templates': return { done: true, value: Object.values(all).sort((a, b) => a.name.localeCompare(b.name)) };
    case 'save_template': { const t = args.template as ServiceTemplate; all[t.id] = structuredClone(t); return { done: true }; }
    case 'delete_template': delete all[String(args.templateId)]; return { done: true };
    case 'create_service_from_template': {
      const template = args.templateId ? all[String(args.templateId)] : undefined;
      const title = String(args.title ?? '').trim() || template?.title || template?.name || 'Service';
      const service: Service = { id: newId(), title, scheduledAtMs: Number(args.scheduledAtMs ?? Date.now()), items: template ? partItems(template, args.parts as string[]) : [] };
      store.services[service.id] = service;
      return { done: true, value: service };
    }
    case 'save_service_as_template': {
      const service = store.services[String(args.serviceId)];
      if (!service) throw new Error('Service not found');
      const id = args.templateId ? String(args.templateId) : newId();
      const made = templateFromService(id, String(args.name ?? '').trim() || service.title, service, Boolean(args.keepSongs));
      all[id] = all[id] ? { ...all[id], name: made.name, parts: made.parts } : made;
      return { done: true, value: all[id] };
    }
    case 'get_sync_status': return { done: true, value: OFFLINE_DEMO };
    case 'list_sync_conflicts': return { done: true, value: [] };
    case 'pair_sync': throw new Error('Cloud sync needs the PALTO app. This browser demo works offline only.');
    default: return { done: false };
  }
}
