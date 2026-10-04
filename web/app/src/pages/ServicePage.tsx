import { api } from "@magi/backend/api";
import type { Id } from "@magi/backend/dataModel";
import { shortLabel } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import type { FunctionReturnType } from "convex/server";
import { useDeferredValue, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { EditableText } from "../components/EditableText";
import { useSafe } from "../lib/errors";
import { formatDate, useMe } from "../lib/me";

type Service = NonNullable<FunctionReturnType<typeof api.services.get>>;
type Item = Service["items"][number];
type Entry = Item["entries"][number];

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

// ── Lyrics (what singers read) ──────────────────────────────────────────

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

// ── Arrange (song leader / operator) ────────────────────────────────────

function Arrange({ service }: { service: Service }) {
  return (
    <div className="space-y-3">
      {service.items.map((item, i) => (
        <ItemCard key={item._id} item={item} isFirst={i === 0} isLast={i === service.items.length - 1} />
      ))}
      <AddSong serviceId={service._id} />
    </div>
  );
}

function ItemCard({ item, isFirst, isLast }: { item: Item; isFirst: boolean; isLast: boolean }) {
  const safe = useSafe();
  const updateItem = useMutation(api.services.updateItem);
  const moveItem = useMutation(api.services.moveItem);
  const removeItem = useMutation(api.services.removeItem);
  const insertEntry = useMutation(api.services.insertEntry);
  const resetSequence = useMutation(api.services.resetSequence);
  const [selected, setSelected] = useState<string | null>(null);
  const selectedEntry = item.entries.find((e) => e.key === selected);
  const itemId = item._id;

  return (
    <div className="card space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="mr-auto font-semibold">{item.title}</h2>
        <EditableText
          className="w-20"
          placeholder="Key"
          value={item.key ?? ""}
          onSave={(key) => safe(updateItem)({ itemId, key })}
        />
        <button className="btn" disabled={isFirst} title="Move up" onClick={() => safe(moveItem)({ itemId, direction: -1 })}>
          ↑
        </button>
        <button className="btn" disabled={isLast} title="Move down" onClick={() => safe(moveItem)({ itemId, direction: 1 })}>
          ↓
        </button>
        <button
          className="btn"
          title="Remove from service"
          onClick={() => confirm(`Remove “${item.title}” from this service?`) && safe(removeItem)({ itemId })}
        >
          ✕
        </button>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {item.entries.map((entry) => (
          <button
            key={entry.key}
            title={entry.label}
            onClick={() => setSelected(selected === entry.key ? null : entry.key)}
            className={`rounded-md px-2.5 py-1 text-sm ${
              selected === entry.key ? "bg-accent text-white" : "bg-accent-soft hover:bg-accent/40"
            } ${entry.isVariation || !entry.sectionId ? "ring-1 ring-variation" : ""}`}
          >
            {shortLabel(entry.label)}
          </button>
        ))}
        <select
          className="py-1"
          value=""
          onChange={async (e) => {
            const value = e.target.value;
            const index = item.entries.length;
            if (value === "__new") {
              await safe(insertEntry)({ itemId, index, label: "Tag", lyrics: "" });
            } else if (value) {
              await safe(insertEntry)({ itemId, index, sectionId: value as Id<"sections"> });
            }
          }}
        >
          <option value="">+ section</option>
          {item.librarySections.map((s) => (
            <option key={s._id} value={s._id}>
              {s.label}
            </option>
          ))}
          <option value="__new">New part for this service only…</option>
        </select>
      </div>

      {selectedEntry && <EntryEditor itemId={itemId} entries={item.entries} entry={selectedEntry} onClose={() => setSelected(null)} />}

      <div className="flex flex-wrap items-center gap-2">
        <EditableText
          className="min-w-0 flex-1"
          placeholder="Notes for the team (e.g. “acapella last chorus”)"
          value={item.notes ?? ""}
          onSave={(notes) => safe(updateItem)({ itemId, notes })}
        />
        {item.songId && (
          <button
            className="btn text-muted"
            onClick={() =>
              confirm("Reset this song to the library's usual order? Changes for this service will be lost.") &&
              safe(resetSequence)({ itemId })
            }
          >
            Reset order
          </button>
        )}
      </div>
    </div>
  );
}

function EntryEditor({
  itemId,
  entries,
  entry,
  onClose,
}: {
  itemId: Id<"serviceItems">;
  entries: Entry[];
  entry: Entry;
  onClose: () => void;
}) {
  const me = useMe();
  const safe = useSafe();
  const moveEntry = useMutation(api.services.moveEntry);
  const removeEntry = useMutation(api.services.removeEntry);
  const insertEntry = useMutation(api.services.insertEntry);
  const setEntryText = useMutation(api.services.setEntryText);
  const promoteEntry = useMutation(api.services.promoteEntry);
  const index = entries.findIndex((e) => e.key === entry.key);
  const entryKey = entry.key;

  return (
    <div className="space-y-2 rounded-lg border border-line p-3">
      <div className="flex flex-wrap items-center gap-2">
        <EditableText
          className="w-36 font-semibold"
          value={entry.label}
          onSave={(label) => safe(setEntryText)({ itemId, entryKey, label })}
        />
        <button className="btn" disabled={index === 0} onClick={() => safe(moveEntry)({ itemId, entryKey, toIndex: index - 1 })}>
          ← Earlier
        </button>
        <button
          className="btn"
          disabled={index === entries.length - 1}
          onClick={() => safe(moveEntry)({ itemId, entryKey, toIndex: index + 1 })}
        >
          Later →
        </button>
        {entry.sectionId && (
          <button
            className="btn"
            title="Sing this part again right after"
            onClick={() => safe(insertEntry)({ itemId, index: index + 1, sectionId: entry.sectionId! })}
          >
            Repeat
          </button>
        )}
        <button
          className="btn"
          onClick={() => {
            onClose();
            void safe(removeEntry)({ itemId, entryKey });
          }}
        >
          Remove
        </button>
      </div>

      <EditableText multiline value={entry.lyrics} onSave={(lyrics) => safe(setEntryText)({ itemId, entryKey, lyrics })} />

      <p className="text-xs text-muted">
        {!entry.sectionId
          ? "This part exists only in this service."
          : entry.isVariation
            ? "Changed for this service only. The library version is untouched."
            : "Editing here changes it for this service only."}
      </p>
      {entry.isVariation && (
        <div className="flex gap-2">
          <button className="btn" onClick={() => safe(setEntryText)({ itemId, entryKey, lyrics: null, label: null })}>
            Use library version
          </button>
          {me.can.editLibrary && (
            <button
              className="btn"
              onClick={() =>
                confirm("Replace the library's lyrics with this version? Every future service will get it.") &&
                safe(promoteEntry)({ itemId, entryKey })
              }
            >
              Save to library
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function AddSong({ serviceId }: { serviceId: Id<"services"> }) {
  const safe = useSafe();
  const addSong = useMutation(api.services.addSong);
  const [search, setSearch] = useState("");
  const deferred = useDeferredValue(search.trim());
  const results = useQuery(api.songs.list, deferred ? { search: deferred } : "skip");

  return (
    <div className="card space-y-2">
      <input
        className="w-full"
        type="search"
        placeholder="Add a song from the library…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      {results?.map((song) => (
        <button
          key={song._id}
          className="flex w-full items-baseline gap-3 rounded-lg px-3 py-2 text-left hover:bg-hover"
          onClick={async () => {
            setSearch("");
            await safe(addSong)({ serviceId, songId: song._id });
          }}
        >
          <span>{song.title}</span>
          <span className="text-sm text-muted">{song.artist}</span>
        </button>
      ))}
      {results?.length === 0 && <p className="px-3 text-sm text-muted">No songs match.</p>}
    </div>
  );
}
