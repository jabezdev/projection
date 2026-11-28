import { api } from "@magi/backend/api";
import { SECTION_KINDS, sectionCode, sectionColor, sectionLabel, type SectionKind } from "@magi/shared";
import { useMutation } from "convex/react";
import { ArrowDown, ArrowUp, Music, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { EditableText } from "../../components/EditableText";
import { OrderStrip } from "../../components/OrderStrip";
import { useSafe } from "../../lib/errors";
import { EntryEditor } from "./EntryEditor";
import type { Item } from "./types";

export function ItemCard({ item, isFirst, isLast }: { item: Item; isFirst: boolean; isLast: boolean }) {
  const safe = useSafe();
  const updateItem = useMutation(api.services.updateItem);
  const moveItem = useMutation(api.services.moveItem);
  const removeItem = useMutation(api.services.removeItem);
  const insertEntry = useMutation(api.services.insertEntry);
  const moveEntry = useMutation(api.services.moveEntry);
  const resetSequence = useMutation(api.services.resetSequence);
  const [selected, setSelected] = useState<string | null>(null);
  const selectedEntry = item.entries.find((e) => e.entryId === selected);
  const itemId = item.id;
  const changed = item.entries.some((e) => e.isVariation || !e.sectionId);

  return (
    <article className="card space-y-3">
      <header className="flex items-start gap-3">
        <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
          <Music size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="truncate font-semibold">
            {item.songId ? (
              <Link to={`/library/${item.songId}`} className="hover:underline">
                {item.title}
              </Link>
            ) : (
              item.title
            )}
          </h3>
          {(item.versionNumber !== null || changed) && (
            <p className="text-xs text-faint">{item.versionNumber !== null ? `Past · version ${item.versionNumber}` : "Some words changed for this service"}</p>
          )}
        </div>
        <EditableText className="w-14 shrink-0 px-1 py-1 text-center" placeholder="Key" value={item.key ?? ""} onSave={(key) => safe(updateItem)({ itemId, key })} />
        <div className="flex">
          <button className="btn-icon" disabled={isFirst} aria-label="Move up" onClick={() => safe(moveItem)({ itemId, direction: -1 })}>
            <ArrowUp size={15} />
          </button>
          <button className="btn-icon" disabled={isLast} aria-label="Move down" onClick={() => safe(moveItem)({ itemId, direction: 1 })}>
            <ArrowDown size={15} />
          </button>
          <button className="btn-icon" aria-label="Remove from service" onClick={() => confirm(`Remove “${item.title}” from this service?`) && safe(removeItem)({ itemId })}>
            <X size={15} />
          </button>
        </div>
      </header>

      <OrderStrip
        chips={item.entries.map((entry) => ({ key: entry.entryId, code: sectionCode(entry), label: sectionLabel(entry), color: sectionColor(entry), flagged: entry.isVariation || !entry.sectionId }))}
        selected={selected}
        onSelect={setSelected}
        onMove={(from, to) => void safe(moveEntry)({ itemId, entryId: item.entries[from].entryId, toIndex: to })}
        empty="No parts. Add one."
      >
        <select
          className="py-1 text-xs"
          aria-label="Add a part"
          value=""
          onChange={async (e) => {
            const value = e.target.value;
            const index = item.entries.length;
            if (value.startsWith("__new:")) await safe(insertEntry)({ itemId, index, kind: value.slice(6) as SectionKind, lyrics: "" });
            else if (value) await safe(insertEntry)({ itemId, index, sectionId: value });
          }}
        >
          <option value="">+ Part</option>
          {item.librarySections.map((s) => (
            <option key={s.id} value={s.id}>
              {s.label}
            </option>
          ))}
          <optgroup label="New part for this service only">
            {SECTION_KINDS.map((k) => (
              <option key={k.kind} value={`__new:${k.kind}`}>
                {k.label}
              </option>
            ))}
          </optgroup>
        </select>
      </OrderStrip>

      {selectedEntry && <EntryEditor itemId={itemId} entries={item.entries} entry={selectedEntry} onClose={() => setSelected(null)} />}

      <div className="flex flex-wrap items-center gap-2">
        <EditableText
          className="min-w-0 flex-1"
          placeholder="Notes for the team, e.g. “acapella last chorus”"
          value={item.notes ?? ""}
          onSave={(notes) => safe(updateItem)({ itemId, notes })}
        />
        {item.songId && (
          <button
            className="btn text-muted"
            onClick={() => confirm("Go back to the library's usual order? Changes for this service will be lost.") && safe(resetSequence)({ itemId })}
          >
            <RotateCcw size={14} />
            Library order
          </button>
        )}
      </div>
    </article>
  );
}
