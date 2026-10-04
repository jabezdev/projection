import { api } from "@magi/backend/api";
import type { Id } from "@magi/backend/dataModel";
import { shortLabel } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useNavigate, useParams } from "react-router";
import { EditableText } from "../components/EditableText";
import { useSafe } from "../lib/errors";
import { useMe } from "../lib/me";

export function SongPage() {
  const songId = useParams().songId as Id<"songs">;
  const song = useQuery(api.songs.get, { songId });
  const me = useMe();
  const safe = useSafe();
  const navigate = useNavigate();
  const update = useMutation(api.songs.update);
  const updateSection = useMutation(api.songs.updateSection);
  const addSection = useMutation(api.songs.addSection);
  const removeSection = useMutation(api.songs.removeSection);
  const setDefaultSequence = useMutation(api.songs.setDefaultSequence);
  const removeSong = useMutation(api.songs.remove);

  if (song === undefined) return <p className="text-muted">Loading…</p>;
  if (song === null) return <p className="text-muted">This song was deleted.</p>;

  const editable = me.can.editLibrary;
  const byId = new Map(song.sections.map((s) => [s._id, s]));
  const sequence = song.defaultSequence;
  const setSequence = (next: Id<"sections">[]) => safe(setDefaultSequence)({ songId, sequence: next });

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <EditableText
          className="w-full border-transparent bg-transparent px-0 text-xl font-semibold"
          value={song.title}
          readOnly={!editable}
          onSave={(title) => safe(update)({ songId, title })}
        />
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <EditableText
            placeholder="Artist"
            value={song.artist ?? ""}
            readOnly={!editable}
            onSave={(artist) => safe(update)({ songId, artist })}
          />
          <EditableText
            placeholder="Key"
            value={song.defaultKey ?? ""}
            readOnly={!editable}
            onSave={(defaultKey) => safe(update)({ songId, defaultKey })}
          />
          <EditableText
            placeholder="CCLI #"
            value={song.ccli ?? ""}
            readOnly={!editable}
            onSave={(ccli) => safe(update)({ songId, ccli })}
          />
        </div>
      </div>

      <section className="space-y-2">
        <h2 className="text-sm font-semibold text-muted">Usual order</h2>
        <div className="flex flex-wrap gap-1.5">
          {sequence.map((id, i) => (
            <span key={i} className="inline-flex items-center gap-1 rounded-md bg-accent-soft px-2 py-1 text-sm">
              {editable && i > 0 && (
                <button
                  title="Move earlier"
                  className="text-muted hover:text-white"
                  onClick={() => setSequence(swap(sequence, i, i - 1))}
                >
                  ‹
                </button>
              )}
              <span title={byId.get(id)?.label}>{shortLabel(byId.get(id)?.label ?? "?")}</span>
              {editable && (
                <button
                  title="Remove from order"
                  className="text-muted hover:text-white"
                  onClick={() => setSequence(sequence.filter((_, j) => j !== i))}
                >
                  ×
                </button>
              )}
            </span>
          ))}
          {sequence.length === 0 && <span className="text-sm text-muted">No sections in the order yet.</span>}
        </div>
        <p className="text-xs text-muted">
          New services start from this order. Use “+ order” on a section to repeat it (e.g. a second chorus).
        </p>
      </section>

      <section className="space-y-3">
        <h2 className="text-sm font-semibold text-muted">Sections</h2>
        {song.sections.map((section) => (
          <div key={section._id} className="card space-y-2">
            <div className="flex items-center gap-2">
              <EditableText
                className="w-40 font-semibold"
                value={section.label}
                readOnly={!editable}
                onSave={(label) => safe(updateSection)({ sectionId: section._id, label })}
              />
              {editable && (
                <>
                  <button className="btn ml-auto" onClick={() => setSequence([...sequence, section._id])}>
                    + order
                  </button>
                  <button
                    className="btn"
                    onClick={() => {
                      if (confirm(`Delete “${section.label}”? Services that use it keep their copy.`)) {
                        void safe(removeSection)({ sectionId: section._id });
                      }
                    }}
                  >
                    Delete
                  </button>
                </>
              )}
            </div>
            <EditableText
              multiline
              value={section.lyrics}
              readOnly={!editable}
              onSave={(lyrics) => safe(updateSection)({ sectionId: section._id, lyrics })}
            />
          </div>
        ))}
        {editable && (
          <button
            className="btn"
            onClick={() => safe(addSection)({ songId, label: nextLabel(song.sections.map((s) => s.label)), lyrics: "" })}
          >
            Add section
          </button>
        )}
      </section>

      {editable && (
        <button
          className="btn border-red-500/50 text-red-400"
          onClick={async () => {
            if (!confirm(`Delete “${song.title}” from the library? Services that use it keep their copy.`)) return;
            navigate("/library", { replace: true });
            await safe(removeSong)({ songId });
          }}
        >
          Delete song
        </button>
      )}
    </div>
  );
}

function swap<T>(list: T[], a: number, b: number): T[] {
  const next = [...list];
  [next[a], next[b]] = [next[b], next[a]];
  return next;
}

function nextLabel(existing: string[]): string {
  const verses = existing.filter((l) => /^verse/i.test(l)).length;
  return `Verse ${verses + 1}`;
}
