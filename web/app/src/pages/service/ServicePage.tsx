import { api } from "@magi/backend/api";
import type { Id } from "@magi/backend/dataModel";
import { shortLabel } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import { formatDate, useMe } from "../../lib/me";
import type { Service } from "./types";
import { Arrange } from "./Arrange";

export function ServicePage() {
  const serviceId = useParams().serviceId as Id<"services">;
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

  if (!me.can.arrange) {
    return (
      <div>
        <h1 className="text-xl font-semibold">{service.title}</h1>
        <p className="text-sm text-muted">
          {formatDate(service.date)}
          {service.leaderName && ` · led by ${service.leaderName}`}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <EditableText
        className="min-w-0 flex-1 border-transparent bg-transparent px-0 text-xl font-semibold"
        value={service.title}
        onSave={(title) => safe(update)({ serviceId: service._id, title })}
      />
      <input
        type="date"
        value={service.date}
        onChange={(e) => e.target.value && safe(update)({ serviceId: service._id, date: e.target.value })}
      />
      <button
        className="btn"
        onClick={async () => {
          if (!confirm(`Delete “${service.title}”?`)) return;
          navigate("/services", { replace: true });
          await safe(remove)({ serviceId: service._id });
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
      {service.items.map((item, n) => (
        <article key={item._id} className="space-y-4">
          <header className="flex items-baseline gap-3 border-b border-line/60 pb-2">
            <span className="text-muted">{n + 1}.</span>
            <h2 className="text-lg font-semibold">{item.title}</h2>
            {item.key && <span className="text-sm text-muted">Key of {item.key}</span>}
            <span className="ml-auto text-xs text-muted">
              {item.entries.map((e) => shortLabel(e.label)).join(" · ")}
            </span>
          </header>
          {item.notes && <p className="text-sm text-muted italic">{item.notes}</p>}
          {item.entries.map((entry) => (
            <section key={entry.key}>
              <h3 className="mb-1 text-xs font-semibold tracking-wide text-accent uppercase">
                {entry.label}
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

