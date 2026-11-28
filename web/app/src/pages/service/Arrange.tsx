import { api } from "@magi/backend/api";
import { useMutation, useQuery } from "convex/react";
import { useState } from "react";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import type { Service, Item } from "./types";
import { ItemCard } from "./ItemCard";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, Radio, X } from "lucide-react";
import { AddItem } from "./AddItem";
import { OtherItem } from "./OtherItem";

export function Arrange({ service }: { service: Service }) {
  const [collapsed, setCollapsed] = useCollapsed();
  const last = service.items.length - 1;
  const groups = groupBySection(service.items);
  return (
    <div className="space-y-3">
      {service.live && <p className="flex items-center gap-2 rounded-lg border border-accent/50 bg-accent-soft px-3 py-2 text-sm"><Radio size={16} className="text-accent" />PALTO is running this service live. Changes wait until it ends.</p>}
      <SectionTools service={service} />
      {groups.map(({ header, items }) => {
        const hidden = header ? collapsed.has(header.id) : false;
        return (
          <div key={header?.id ?? "start"} className="space-y-3">
            {header && <SectionRow header={header} count={items.length} collapsed={hidden} onToggle={() => setCollapsed(header.id, !hidden)} isFirst={header === service.items[0]} isLast={service.items.indexOf(header) === last} />}
            {!hidden && items.map((item) => {
              const i = service.items.indexOf(item);
              return item.kind === "song"
                ? <ItemCard key={item.id} item={item} isFirst={i === 0} isLast={i === last} />
                : <OtherItem key={item.id} item={item} isFirst={i === 0} isLast={i === last} />;
            })}
          </div>
        );
      })}
      <AddItem serviceId={service.id} />
    </div>
  );
}

function groupBySection(items: Item[]) {
  const groups: { header: Item | null; items: Item[] }[] = [];
  for (const item of items) {
    if (item.kind === "header") groups.push({ header: item, items: [] });
    else if (groups.length) groups[groups.length - 1].items.push(item);
    else groups.push({ header: null, items: [item] });
  }
  return groups;
}

const COLLAPSED_KEY = "palto.collapsedSections";

function useCollapsed(): [Set<string>, (id: string, collapsed: boolean) => void] {
  const [ids, setIds] = useState<Set<string>>(() => {
    try { return new Set(JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]") as string[]); } catch { return new Set(); }
  });
  const set = (id: string, collapsed: boolean) => {
    const next = new Set(ids);
    if (collapsed) next.add(id); else next.delete(id);
    setIds(next);
    try { localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...next].slice(-500))); } catch { return; }
  };
  return [ids, set];
}

function SectionRow({ header, count, collapsed, onToggle, isFirst, isLast }: { header: Item; count: number; collapsed: boolean; onToggle: () => void; isFirst: boolean; isLast: boolean }) {
  const safe = useSafe();
  const updateItem = useMutation(api.services.updateItem);
  const moveItem = useMutation(api.services.moveItem);
  const removeSection = useMutation(api.serviceParts.removeSection);
  const itemId = header.id;
  const remove = () => {
    if (count === 0) return void safe(removeSection)({ itemId, withItems: false });
    const all = confirm(`Remove “${header.title}” and its ${count} item${count === 1 ? "" : "s"}?\n\nPress Cancel to keep the items and remove only the section heading.`);
    if (all) return void safe(removeSection)({ itemId, withItems: true });
    if (confirm(`Remove only the “${header.title}” heading? Its items join the section above.`)) void safe(removeSection)({ itemId, withItems: false });
  };
  return (
    <div className="flex items-center gap-1 border-b border-line pt-4 pb-1.5">
      <button className="btn-icon" aria-expanded={!collapsed} aria-label={collapsed ? `Show ${header.title}` : `Hide ${header.title}`} onClick={onToggle}>
        {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
      </button>
      <EditableText className="min-w-0 flex-1 border-transparent bg-transparent px-1 font-label text-base font-semibold tracking-[0.12em] uppercase focus:border-line" value={header.title} onSave={(title) => safe(updateItem)({ itemId, title })} />
      <span className="px-2 text-xs text-faint">{count === 0 ? "empty" : `${count} item${count === 1 ? "" : "s"}`}</span>
      <button className="btn-icon" disabled={isFirst} aria-label="Move section up" onClick={() => safe(moveItem)({ itemId, direction: -1 })}><ArrowUp size={15} /></button>
      <button className="btn-icon" disabled={isLast} aria-label="Move section down" onClick={() => safe(moveItem)({ itemId, direction: 1 })}><ArrowDown size={15} /></button>
      <button className="btn-icon" aria-label="Remove section" onClick={remove}><X size={15} /></button>
    </div>
  );
}

function SectionTools({ service }: { service: Service }) {
  const safe = useSafe();
  const templates = useQuery(api.templates.list);
  const addPart = useMutation(api.serviceParts.addPart);
  const addHeader = useMutation(api.services.addHeader);
  const removeEmpty = useMutation(api.serviceParts.removeEmptySections);
  const saveFromService = useMutation(api.templates.saveFromService);
  const titles = new Set(service.items.filter((i) => i.kind === "header").map((i) => i.title));
  const missing = (templates ?? []).flatMap((t) => t.parts.filter((p) => !titles.has(p.title)).map((p) => ({ template: t, part: p })));
  const empty = service.items.some((item, i) => item.kind === "header" && (i === service.items.length - 1 || service.items[i + 1].kind === "header"));
  const saveAsTemplate = () => {
    const name = prompt("Name of the template. Its sections and fixed items, like videos and slides, are kept; songs are left out.", service.title);
    if (name) void safe(saveFromService)({ serviceId: service.id, name, keepSongs: false });
  };
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <select className="max-w-56" aria-label="Add a section" value="" onChange={(e) => {
        const [templateId, partId] = e.target.value.split("|");
        if (templateId === "new") void safe(addHeader)({ serviceId: service.id, title: prompt("Section name") || "New section" });
        else if (partId) void safe(addPart)({ serviceId: service.id, templateId, partId });
      }}>
        <option value="">Add a section…</option>
        {missing.map(({ template, part }) => <option key={`${template.id}|${part.id}`} value={`${template.id}|${part.id}`}>{part.title}{templates && templates.length > 1 ? ` (${template.name})` : ""}</option>)}
        <option value="new|">New section…</option>
      </select>
      {empty && <button className="btn" onClick={() => safe(removeEmpty)({ serviceId: service.id })}>Remove empty sections</button>}
      <span className="grow" />
      <button className="btn" onClick={saveAsTemplate}>Save as template</button>
    </div>
  );
}
