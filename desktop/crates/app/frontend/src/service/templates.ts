import { newId, type Service, type ServiceItem, type ServiceTemplate } from '../api/models.ts';

export const defaultParts = (template: ServiceTemplate) => template.parts.filter(p => !p.optional).map(p => p.id);

export function nextOccurrence(template: Pick<ServiceTemplate, 'weekday' | 'startMinutes'>, from = new Date()): number {
  const d = new Date(from);
  const minutes = template.startMinutes ?? 600;
  d.setHours(Math.floor(minutes / 60), minutes % 60, 0, 0);
  if (template.weekday !== undefined) {
    d.setDate(d.getDate() + ((template.weekday - d.getDay() + 7) % 7));
    if (d.getTime() < from.getTime()) d.setDate(d.getDate() + 7);
  } else if (d.getTime() < from.getTime()) d.setDate(d.getDate() + 1);
  return d.getTime();
}

export function freshItem(item: ServiceItem): ServiceItem {
  const copy = structuredClone(item);
  copy.id = newId();
  if (copy.content.kind === 'song') delete copy.content.versionId;
  return copy;
}

export function partItems(template: ServiceTemplate, parts: string[]): ServiceItem[] {
  return template.parts.filter(p => parts.includes(p.id)).flatMap(p => [{ id: newId(), title: p.title, ...(p.notes ? { notes: p.notes } : {}), content: { kind: 'header' as const } }, ...p.items.map(freshItem)]);
}

export function templateFromService(id: string, name: string, service: Service, keepSongs: boolean): ServiceTemplate {
  const parts: ServiceTemplate['parts'] = [];
  for (const item of service.items) {
    if (item.content.kind === 'header') { parts.push({ id: newId(), title: item.title ?? '', ...(item.notes ? { notes: item.notes } : {}), items: [] }); continue; }
    if (item.content.kind === 'song' && !keepSongs) continue;
    if (!parts.length) parts.push({ id: newId(), title: 'Start', items: [] });
    parts[parts.length - 1].items.push(freshItem(item));
  }
  return { id, name, title: service.title, parts };
}

export function sectionsOf(items: ServiceItem[]) {
  const starts = items.flatMap((item, i) => item.content.kind === 'header' ? [i] : []);
  return starts.map((start, n) => ({ header: items[start], start, end: starts[n + 1] ?? items.length }));
}

export function withoutEmptySections(items: ServiceItem[]) {
  const empty = new Set(sectionsOf(items).filter(s => s.end === s.start + 1).map(s => s.header.id));
  return items.filter(i => !empty.has(i.id));
}

export function insertPart(items: ServiceItem[], template: ServiceTemplate, partId: string): { items: ServiceItem[]; first?: string } {
  const want = template.parts.findIndex(p => p.id === partId);
  if (want < 0) return { items };
  const rank = new Map(template.parts.map((p, i) => [p.title, i]));
  let at = 0;
  for (const section of sectionsOf(items)) {
    const r = rank.get(section.header.title ?? '');
    if (r !== undefined && r < want) at = section.end;
  }
  const added = partItems(template, [partId]);
  return { items: [...items.slice(0, at), ...added, ...items.slice(at)], first: added[0]?.id };
}
