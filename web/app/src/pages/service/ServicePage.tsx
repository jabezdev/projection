import { api } from "@magi/backend/api";
import { sectionCode, sectionColor, sectionLabel } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import { formatWhen, fromLocalInput, toLocalInput, useMe } from "../../lib/me";
import type { Service } from "./types";
import { Arrange } from "./Arrange";

export function ServicePage() {
  const serviceId = useParams().serviceId ?? "";
  const service = useQuery(api.services.get, { serviceId });
  const me = useMe();
  const [params, setParams] = useSearchParams();
  const view = me.can.arrange ? (params.get("view") ?? "arrange") : "lyrics";

  if (service === undefined) return <p className="text-muted">Loading…</p>;
  if (service === null) return <p className="text-muted">This service was deleted.</p>;

  return (
    <div className="space-y-4">
      <ServiceHeader service={service} />

      {me.can.arrange && (
        <div className="inline-flex rounded-lg border border-line p-0.5 text-sm">
          {(["arrange", "lyrics"] as const).map((v) => (
            <button
              key={v}
              className={`rounded-md px-3 py-1 capitalize ${view === v ? "bg-hover" : "text-muted"}`}
              onClick={() => setParams({ view: v }, { replace: true })}
            >
              {v}
            </button>
          ))}
        </div>
      )}

      {view === "arrange" ? <Arrange service={service} /> : <Lyrics service={service} />}
    </div>
  );
}

function ServiceHeader({ service }: { service: Service }) {
  const me = useMe();
  const safe = useSafe();
  const navigate = useNavigate();
  const update = useMutation(api.services.update);
  const remove = useMutation(api.services.remove);
  const end = useMutation(api.services.end);
  const past = service.frozen || service.endedAtMs !== undefined;

  if (!me.can.arrange) {
    return (
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{service.title}</h1>
        <p className="text-sm text-muted">
          {formatWhen(service.scheduledAtMs)}
          {service.leaderName && ` · led by ${service.leaderName}`}
          {past && " · past"}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <EditableText
        className="min-w-0 flex-1 basis-full border-transparent bg-transparent px-0 text-2xl font-bold tracking-tight focus:border-line focus:px-3 sm:basis-auto"
        value={service.title}
        onSave={(title) => safe(update)({ serviceId: service.id, title })}
      />
      <input
        type="datetime-local"
        value={toLocalInput(service.scheduledAtMs)}
        onChange={(e) => e.target.value && safe(update)({ serviceId: service.id, scheduledAtMs: fromLocalInput(e.target.value) })}
      />
      {past ? (
        <span className="text-sm text-muted" title="Songs keep the words they had; library changes don't reach this service">Past</span>
      ) : (
        <button
          className="btn"
          onClick={() => confirm(`End “${service.title}”? Its songs keep the words they have now.`) && safe(end)({ serviceId: service.id })}
        >
          End
        </button>
      )}
      <button
        className="btn"
        onClick={async () => {
          if (!confirm(`Delete “${service.title}”?`)) return;
          navigate("/services", { replace: true });
          await safe(remove)({ serviceId: service.id });
        }}
      >
        Delete
      </button>
    </div>
  );
}

function Lyrics({ service }: { service: Service }) {
  if (service.items.length === 0) return <p className="text-muted">No songs yet.</p>;
  return (
    <div className="space-y-10">
      {service.items.map((item, n) => item.kind === "header" ? (
        <h2 key={item.id} className="eyebrow border-b border-line pb-1.5 text-faint">{item.title}</h2>
      ) : item.kind !== "song" ? (
        <p key={item.id} className="text-sm text-muted">{n + 1}. {item.title}</p>
      ) : (
        <article key={item.id} className="space-y-4">
          <header className="flex items-baseline gap-3 border-b border-line pb-2">
            <span className="text-muted">{n + 1}.</span>
            <h2 className="text-lg font-semibold">{item.title}</h2>
            {item.key && <span className="text-sm text-muted">Key of {item.key}</span>}
            <span className="ml-auto text-xs text-muted">
              {item.entries.map(sectionCode).join(" · ")}
            </span>
          </header>
          {item.notes && <p className="text-sm text-muted italic">{item.notes}</p>}
          {item.entries.map((entry) => (
            <section key={entry.entryId}>
              <h3 className="mb-1 font-label text-xs font-semibold tracking-[0.12em] uppercase" style={{ color: sectionColor(entry) }}>
                {sectionLabel(entry)}
                {entry.isVariation && <span className="ml-2 text-variation normal-case">changed for this service</span>}
              </h3>
              <p className="text-lg leading-relaxed whitespace-pre-line">{entry.lyrics}</p>
            </section>
          ))}
        </article>
      ))}
    </div>
  );
}

