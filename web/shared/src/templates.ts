import { newId } from "./sections";
import type { ItemRecord } from "./edits/types";

export type TemplatePart = { id: string; title: string; optional?: boolean; notes?: string; items: ItemRecord[] };
export type ServiceTemplate = { id: string; name: string; title?: string; weekday?: number; startMinutes?: number; parts: TemplatePart[] };

export const DEFAULT_TEMPLATE_ID = "sunday";

const part = (key: string, title: string, optional = false): TemplatePart => ({ id: `sunday-${key}`, title, ...(optional ? { optional } : {}), items: [] });

export const sundayTemplate = (): ServiceTemplate => ({
  id: DEFAULT_TEMPLATE_ID,
  name: "Sunday",
  title: "Sunday Service",
  weekday: 0,
  startMinutes: 600,
  parts: [
    part("welcoming", "Welcoming"),
    part("countdown", "Countdown"),
    part("worship", "Praise & Worship"),
    part("offering", "Tithes and Offering"),
    part("special-number", "Special Number", true),
    part("message", "Message"),
    part("special-program", "Special Program", true),
    part("closing", "Closing"),
  ],
});

export const defaultParts = (template: ServiceTemplate) => template.parts.filter((p) => !p.optional).map((p) => p.id);

export function freshItem(item: ItemRecord): ItemRecord {
  const copy = structuredClone(item);
  copy.id = newId();
  if (copy.content.kind === "song") delete copy.content.versionId;
  return copy;
}

export function templateItems(template: ServiceTemplate, parts: string[]): ItemRecord[] {
  return template.parts
    .filter((p) => parts.includes(p.id))
    .flatMap((p) => [{ id: newId(), title: p.title, ...(p.notes ? { notes: p.notes } : {}), content: { kind: "header" } }, ...p.items.map(freshItem)]);
}

export function templateFromService(id: string, name: string, title: string, items: ItemRecord[], keepSongs: boolean): ServiceTemplate {
  const parts: TemplatePart[] = [];
  for (const item of items) {
    if (item.content.kind === "header") {
      parts.push({ id: newId(), title: item.title ?? "", ...(item.notes ? { notes: item.notes } : {}), items: [] });
      continue;
    }
    if (item.content.kind === "song" && !keepSongs) continue;
    if (!parts.length) parts.push({ id: newId(), title: "Start", items: [] });
    parts[parts.length - 1].items.push(freshItem(item));
  }
  return { id, name, title, parts };
}

export function nextOccurrence(template: Pick<ServiceTemplate, "weekday" | "startMinutes">, from = new Date()): number {
  const d = new Date(from);
  const minutes = template.startMinutes ?? 600;
  d.setHours(Math.floor(minutes / 60), minutes % 60, 0, 0);
  if (template.weekday !== undefined) {
    d.setDate(d.getDate() + ((template.weekday - d.getDay() + 7) % 7));
    if (d.getTime() < from.getTime()) d.setDate(d.getDate() + 7);
  } else if (d.getTime() < from.getTime()) d.setDate(d.getDate() + 1);
  return d.getTime();
}
