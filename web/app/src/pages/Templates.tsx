import { api } from "@magi/backend/api";
import { newId, type ServiceTemplate, type TemplatePart } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import { useSafe } from "../lib/errors";
import { useMe } from "../lib/me";

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const time = (minutes?: number) => (minutes === undefined ? "" : `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`);
const minutesOf = (value: string) => (value ? Number(value.slice(0, 2)) * 60 + Number(value.slice(3, 5)) : undefined);

export function Templates() {
  const me = useMe();
  const safe = useSafe();
  const templates = useQuery(api.templates.list);
  const save = useMutation(api.templates.save);
  const [open, setOpen] = useState<string | null>(null);
  if (templates === undefined) return <p className="text-muted">Loading…</p>;
  const create = () => {
    const template: ServiceTemplate = { id: newId(), name: "New template", parts: [{ id: newId(), title: "Welcoming", items: [] }] };
    void safe(save)({ template: template as never }).then(() => setOpen(template.id));
  };
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <Link to="/services" className="text-sm text-muted hover:text-fg">← Services</Link>
        <h1 className="mr-auto text-xl font-semibold">Templates</h1>
        {me.can.arrange && <button className="btn" onClick={create}>New template</button>}
      </div>
      <p className="text-sm text-muted">A template is the usual order of a service. When you plan a service you pick which parts to include, and you can still add, remove or move anything afterwards.</p>
      {templates.length === 0 && <p className="text-muted">No templates yet.</p>}
      {templates.map((t) => (
        <TemplateCard key={t.id} template={t} open={open === t.id} onToggle={() => setOpen(open === t.id ? null : t.id)} editable={me.can.arrange} />
      ))}
    </div>
  );
}

function TemplateCard({ template, open, onToggle, editable }: { template: ServiceTemplate; open: boolean; onToggle: () => void; editable: boolean }) {
  const safe = useSafe();
  const save = useMutation(api.templates.save);
  const remove = useMutation(api.templates.remove);
  const [draft, setDraft] = useState(template);
  useEffect(() => setDraft(template), [template]);
  const commit = (next: ServiceTemplate) => { setDraft(next); void safe(save)({ template: next as never }); };
  const setPart = (id: string, patch: Partial<TemplatePart>) => commit({ ...draft, parts: draft.parts.map((p) => (p.id === id ? { ...p, ...patch } : p)) });
  const move = (index: number, by: number) => {
    const parts = [...draft.parts];
    const [part] = parts.splice(index, 1);
    parts.splice(index + by, 0, part);
    commit({ ...draft, parts });
  };
  const summary = `${draft.parts.length} parts${draft.weekday !== undefined ? ` · ${DAYS[draft.weekday]}s` : ""}${draft.startMinutes !== undefined ? ` at ${time(draft.startMinutes)}` : ""}`;

  return (
    <section className="card space-y-3">
      <button className="flex w-full items-baseline gap-3 text-left" onClick={onToggle} aria-expanded={open}>
        <span className="font-semibold">{draft.name}</span>
        <span className="text-sm text-muted">{summary}</span>
        <span className="ml-auto text-muted">{open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <div className="space-y-3">
          <fieldset disabled={!editable} className="flex flex-wrap gap-2">
            <input aria-label="Template name" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} onBlur={() => commit(draft)} />
            <input aria-label="Service title" placeholder="Service title" value={draft.title ?? ""} onChange={(e) => setDraft({ ...draft, title: e.target.value || undefined })} onBlur={() => commit(draft)} />
            <select aria-label="Usual day" value={draft.weekday ?? ""} onChange={(e) => commit({ ...draft, weekday: e.target.value === "" ? undefined : Number(e.target.value) })}>
              <option value="">Any day</option>
              {DAYS.map((d, i) => <option key={d} value={i}>{d}</option>)}
            </select>
            <input aria-label="Usual time" type="time" value={time(draft.startMinutes)} onChange={(e) => commit({ ...draft, startMinutes: minutesOf(e.target.value) })} />
          </fieldset>
          <ol className="divide-y divide-line rounded-lg border border-line">
            {draft.parts.map((part, i) => (
              <li key={part.id} className="space-y-1 px-3 py-2">
                <div className="flex flex-wrap items-center gap-2">
                  <input className="min-w-0 flex-1 border-transparent bg-transparent px-0 font-medium" aria-label="Part name" disabled={!editable} value={part.title}
                    onChange={(e) => setDraft({ ...draft, parts: draft.parts.map((p) => (p.id === part.id ? { ...p, title: e.target.value } : p)) })} onBlur={() => commit(draft)} />
                  <label className="flex items-center gap-1 text-xs text-muted" title="Optional parts start unticked when you plan a service">
                    <input type="checkbox" disabled={!editable} checked={!!part.optional} onChange={(e) => setPart(part.id, { optional: e.target.checked || undefined })} />Optional
                  </label>
                  {editable && <>
                    <button className="btn" disabled={i === 0} title="Move up" onClick={() => move(i, -1)}>↑</button>
                    <button className="btn" disabled={i === draft.parts.length - 1} title="Move down" onClick={() => move(i, 1)}>↓</button>
                    <button className="btn" title="Remove part" onClick={() => commit({ ...draft, parts: draft.parts.filter((p) => p.id !== part.id) })}>✕</button>
                  </>}
                </div>
                {part.items.length > 0 && (
                  <ul className="space-y-0.5 pl-3 text-sm text-muted">
                    {part.items.map((item) => (
                      <li key={item.id} className="flex items-center gap-2">
                        <span className="mr-auto">{item.title || item.content.kind}</span>
                        {editable && <button className="text-xs hover:text-fg" title="Remove from template" onClick={() => setPart(part.id, { items: part.items.filter((x) => x.id !== item.id) })}>✕</button>}
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ol>
          {editable && (
            <div className="flex flex-wrap gap-2">
              <button className="btn" onClick={() => commit({ ...draft, parts: [...draft.parts, { id: newId(), title: "New part", items: [] }] })}>Add part</button>
              <span className="grow" />
              <button className="btn" onClick={() => confirm(`Delete the “${draft.name}” template? Services already planned keep their items.`) && safe(remove)({ templateId: draft.id })}>Delete template</button>
            </div>
          )}
          <p className="text-xs text-muted">Items that come with a part, like a countdown video or an offering slide, are added by saving a service as a template.</p>
        </div>
      )}
    </section>
  );
}
