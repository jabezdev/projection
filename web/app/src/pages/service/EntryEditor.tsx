import { api } from "@magi/backend/api";
import { sectionLabel } from "@magi/shared";
import { useMutation } from "convex/react";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import { useMe } from "../../lib/me";
import type { Entry } from "./types";

export function EntryEditor({
  itemId,
  entries,
  entry,
  onClose,
}: {
  itemId: string;
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
  const index = entries.findIndex((e) => e.entryId === entry.entryId);
  const entryId = entry.entryId;

  return (
    <div className="space-y-2 rounded-lg border border-line p-3">
      <div className="flex flex-wrap items-center gap-2">
        <EditableText
          className="w-36 font-semibold"
          value={sectionLabel(entry)}
          onSave={(label) => safe(setEntryText)({ itemId, entryId, label })}
        />
        <button className="btn" disabled={index === 0} onClick={() => safe(moveEntry)({ itemId, entryId, toIndex: index - 1 })}>
          ← Earlier
        </button>
        <button
          className="btn"
          disabled={index === entries.length - 1}
          onClick={() => safe(moveEntry)({ itemId, entryId, toIndex: index + 1 })}
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
            void safe(removeEntry)({ itemId, entryId });
          }}
        >
          Remove
        </button>
      </div>

      <EditableText multiline value={entry.lyrics} onSave={(lyrics) => safe(setEntryText)({ itemId, entryId, lyrics })} />

      <p className="text-xs text-muted">
        {!entry.sectionId
          ? "This part exists only in this service."
          : entry.isVariation
            ? "Changed for this service only. The library version is untouched."
            : "Editing here changes it for this service only."}
      </p>
      {entry.isVariation && (
        <div className="flex gap-2">
          <button className="btn" onClick={() => safe(setEntryText)({ itemId, entryId, lyrics: null, label: null })}>
            Use library version
          </button>
          {me.can.editLibrary && (
            <button
              className="btn"
              onClick={() =>
                confirm("Replace the library's lyrics with this version? Every future service will get it.") &&
                safe(promoteEntry)({ itemId, entryId })
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
