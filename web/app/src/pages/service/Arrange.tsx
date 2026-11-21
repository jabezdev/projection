import { api } from "@magi/backend/api";
import { SECTION_KINDS, sectionCode, sectionLabel, type SectionKind } from "@magi/shared";
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
      {service.items.map((item, i) => item.kind === "song" ? (
        <ItemCard key={item.id} item={item} isFirst={i === 0} isLast={i === service.items.length - 1} />
      ) : (
        <OtherItem key={item.id} item={item} isFirst={i === 0} isLast={i === service.items.length - 1} />
      ))}
      <AddSong serviceId={service.id} />
    </div>
  );
}

function OtherItem({ item, isFirst, isLast }: { item: Item; isFirst: boolean; isLast: boolean }) {
  const safe = useSafe();
  const moveItem = useMutation(api.services.moveItem);
  const removeItem = useMutation(api.services.removeItem);
  const itemId = item.id;
  const kind = { header: "Section", text: "Text", media: "Media", presentation: "Slides", capture: "Screen capture", song: "Song" }[item.kind];
  return (
    <div className={`flex items-center gap-2 px-1 ${item.kind === "header" ? "pt-3 font-semibold" : "text-sm"}`}>
      <span className="mr-auto">{item.title}<span className="ml-2 text-xs font-normal text-muted">{kind}</span></span>
      <button className="btn" disabled={isFirst} title="Move up" onClick={() => safe(moveItem)({ itemId, direction: -1 })}>↑</button>
      <button className="btn" disabled={isLast} title="Move down" onClick={() => safe(moveItem)({ itemId, direction: 1 })}>↓</button>
      <button className="btn" title="Remove from service" onClick={() => confirm(`Remove “${item.title}” from this service?`) && safe(removeItem)({ itemId })}>✕</button>
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
  const selectedEntry = item.entries.find((e) => e.entryId === selected);
  const itemId = item.id;

  return (
    <div className="card space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="mr-auto font-semibold">
          {item.title}
          {item.versionNumber !== null && <span className="ml-2 text-xs font-normal text-muted">past · version {item.versionNumber}</span>}
        </h2>
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
            key={entry.entryId}
            title={sectionLabel(entry)}
            onClick={() => setSelected(selected === entry.entryId ? null : entry.entryId)}
            className={`rounded-md px-2.5 py-1 text-sm ${
              selected === entry.entryId ? "bg-accent text-white" : "bg-accent-soft hover:bg-accent/40"
            } ${entry.isVariation || !entry.sectionId ? "ring-1 ring-variation" : ""}`}
          >
            {sectionCode(entry)}
          </button>
        ))}
        <select
          className="py-1"
          value=""
          onChange={async (e) => {
            const value = e.target.value;
            const index = item.entries.length;
            if (value.startsWith("__new:")) {
              await safe(insertEntry)({ itemId, index, kind: value.slice(6) as SectionKind, lyrics: "" });
            } else if (value) {
              await safe(insertEntry)({ itemId, index, sectionId: value });
            }
          }}
        >
          <option value="">+ section</option>
          {item.librarySections.map((s) => (
            <option key={s.id} value={s.id}>
              {s.label}
            </option>
          ))}
          <optgroup label="New part for this service only">
            {SECTION_KINDS.map((k) => (
              <option key={k.kind} value={`__new:${k.kind}`}>{k.label}</option>
            ))}
          </optgroup>
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
