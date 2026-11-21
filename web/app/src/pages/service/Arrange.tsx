import { api } from "@magi/backend/api";
import type { Id } from "@magi/backend/dataModel";
import { shortLabel } from "@magi/shared";
import { useMutation } from "convex/react";
import { useState } from "react";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import type { Service, Item } from "./types";
import { EntryEditor } from "./EntryEditor";
import { AddSong } from "./AddSong";

export function Arrange({ service }: { service: Service }) {
  return (
    <div className="space-y-3">
      {service.items.map((item, i) => (
        <ItemCard key={item._id} item={item} isFirst={i === 0} isLast={i === service.items.length - 1} />
      ))}
      <AddSong serviceId={service._id} />
    </div>
  );
}

export function ItemCard({ item, isFirst, isLast }: { item: Item; isFirst: boolean; isLast: boolean }) {
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
