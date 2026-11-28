import { api } from "@magi/backend/api";
import { useMutation, useQuery } from "convex/react";
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";
import { useSafe } from "../lib/errors";
import { formatWhen, fromLocalInput, nextSundayMorning, startOfToday, toLocalInput, useMe } from "../lib/me";
import { defaultParts, nextOccurrence } from "@magi/shared";

export function Services() {
  const me = useMe();
  const services = useQuery(api.services.list);
  const freezePast = useMutation(api.services.freezePast);
  useEffect(() => { void freezePast({ todayStartsAtMs: startOfToday() }).catch(() => {}); }, [freezePast]);
  const today = startOfToday();
  const isPast = (s: { scheduledAtMs: number; endedAtMs?: number }) => s.endedAtMs !== undefined || s.scheduledAtMs < today;
  const upcoming = (services ?? []).filter((s) => !isPast(s)).reverse();
  const past = (services ?? []).filter(isPast);

  return (
    <div className="space-y-6">
      {me.can.arrange && <NewService />}
      {!me.can.arrange && <Link to="/templates" className="text-sm text-muted hover:text-fg">Templates</Link>}

      {services === undefined ? (
        <p className="text-muted">Loading…</p>
      ) : (
        <>
          <ServiceList title="Upcoming" services={upcoming} empty="Nothing planned yet." />
          {past.length > 0 && <ServiceList title="Past" services={past} />}
        </>
      )}
    </div>
  );
}

function NewService() {
  const safe = useSafe();
  const navigate = useNavigate();
  const templates = useQuery(api.templates.list);
  const create = useMutation(api.templates.createService);
  const [templateId, setTemplateId] = useState<string | null>(null);
  const template = templates?.find((t) => t.id === templateId) ?? (templateId === "" ? null : templates?.[0]) ?? null;
  const [parts, setParts] = useState<string[] | null>(null);
  const chosen = parts ?? (template ? defaultParts(template) : []);
  const [when, setWhen] = useState<string | null>(null);
  const [title, setTitle] = useState<string | null>(null);
  const shownWhen = when ?? toLocalInput(template ? nextOccurrence(template) : nextSundayMorning());
  const shownTitle = title ?? template?.title ?? template?.name ?? "Service";
  const pick = (id: string) => { setTemplateId(id); setParts(null); setTitle(null); setWhen(null); };
  const toggle = (id: string) => setParts(chosen.includes(id) ? chosen.filter((p) => p !== id) : [...chosen, id]);

  return (
    <form
      className="card space-y-3"
      onSubmit={async (e) => {
        e.preventDefault();
        const id = await safe(create)({ templateId: template?.id, parts: chosen, scheduledAtMs: fromLocalInput(shownWhen), title: shownTitle });
        if (id) navigate(`/services/${id}`);
      }}
    >
      <div className="flex flex-wrap items-center gap-2">
        <select aria-label="Template" value={template?.id ?? ""} onChange={(e) => pick(e.target.value)}>
          {(templates ?? []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
          <option value="">Blank service</option>
        </select>
        <input type="datetime-local" value={shownWhen} onChange={(e) => setWhen(e.target.value)} required />
        <input className="min-w-0 flex-1" value={shownTitle} onChange={(e) => setTitle(e.target.value)} />
        <button className="btn-primary">Plan service</button>
      </div>
      {template && (
        <fieldset className="flex flex-wrap gap-x-4 gap-y-1 text-sm">
          <legend className="mb-1 text-xs text-muted">Parts this week · <Link className="hover:text-fg" to="/templates">edit templates</Link></legend>
          {template.parts.map((p) => (
            <label key={p.id} className={`flex items-center gap-1.5 ${chosen.includes(p.id) ? "" : "text-muted"}`}>
              <input type="checkbox" checked={chosen.includes(p.id)} onChange={() => toggle(p.id)} />
              {p.title}
            </label>
          ))}
        </fieldset>
      )}
    </form>
  );
}

function ServiceList({
  title,
  services,
  empty,
}: {
  title: string;
  services: { id: string; scheduledAtMs: number; title: string; live?: boolean }[];
  empty?: string;
}) {
  return (
    <section className="space-y-2">
      <h2 className="eyebrow">{title}</h2>
      {services.length === 0 ? (
        <p className="text-sm text-muted">{empty}</p>
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-card">
          {services.map((s) => (
            <li key={s.id}>
              <Link to={`/services/${s.id}`} className="flex flex-col gap-0.5 px-4 py-3 hover:bg-hover sm:flex-row sm:items-center sm:gap-4">
                <span className="text-sm text-faint sm:w-48 sm:shrink-0">{formatWhen(s.scheduledAtMs)}</span>
                <span className="flex items-center gap-2 font-medium">
                  {s.title}
                  {s.live && <span className="rounded bg-accent px-1.5 py-0.5 font-label text-[10px] font-semibold tracking-[0.12em] text-[#1b0a05]">LIVE</span>}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
