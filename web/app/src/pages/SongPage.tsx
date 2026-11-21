import { api } from "@magi/backend/api";
import { labelToSection, newId, sectionCode, sectionLabel, type Section } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useParams } from "react-router";
import { EditableText } from "../components/EditableText";
import { useSafe } from "../lib/errors";
import { useMe } from "../lib/me";

export function SongPage() {
  const songId = useParams().songId ?? "";
  const song = useQuery(api.songs.get, { songId });
  const me = useMe();
  const safe = useSafe();
  const update = useMutation(api.songs.update);
  const setContent = useMutation(api.songs.setContent);
  const archive = useMutation(api.songs.archive);

  if (song === undefined) return <p className="text-muted">Loading…</p>;
  if (song === null) return <p className="text-muted">This song was not found.</p>;

  const editable = me.can.editLibrary;
  const { sections, arrangement, background } = song.current;
  const byId = new Map(sections.map((s) => [s.id, s]));
  const save = (next: { sections?: Section[]; arrangement?: string[] }) =>
    safe(setContent)({ songId, sections: next.sections ?? sections, arrangement: next.arrangement ?? arrangement, background });
  const setSection = (id: string, patch: Partial<Section>) => save({ sections: sections.map((s) => (s.id === id ? { ...s, ...patch } : s)) });
  const credits = song.credits;
  const setCredits = (patch: Partial<typeof credits>) => safe(update)({ songId, credits: { ...credits, ...patch } });
  const text = (value: string) => value.trim() || undefined;

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <EditableText
          className="w-full border-transparent bg-transparent px-0 text-xl font-semibold"
          value={song.title}
          readOnly={!editable}
          onSave={(title) => safe(update)({ songId, title })}
        />
        {song.archived && <p className="text-sm text-muted">Archived: hidden from the library, still shown in services that use it.</p>}
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <EditableText
            placeholder="Authors, comma separated"
            value={credits.authors?.map((a) => a.name).join(", ") ?? ""}
            readOnly={!editable}
            onSave={(value) => {
              const names = value.split(",").map((n) => n.trim()).filter(Boolean);
              return setCredits({ authors: names.length ? names.map((name) => credits.authors?.find((a) => a.name === name) ?? { name }) : undefined });
            }}
          />
          <EditableText placeholder="Key" value={song.key ?? ""} readOnly={!editable} onSave={(key) => safe(update)({ songId, key })} />
          <EditableText placeholder="CCLI #" value={credits.ccli ?? ""} readOnly={!editable} onSave={(ccli) => setCredits({ ccli: text(ccli) })} />
          <EditableText placeholder="Copyright" value={credits.copyright ?? ""} readOnly={!editable} onSave={(copyright) => setCredits({ copyright: text(copyright) })} />
          <EditableText placeholder="Publisher" value={credits.publisher ?? ""} readOnly={!editable} onSave={(publisher) => setCredits({ publisher: text(publisher) })} />
          <EditableText
            placeholder="Year"
            value={credits.year ? String(credits.year) : ""}
            readOnly={!editable}
            onSave={(year) => setCredits({ year: Number.isInteger(Number(year)) && year.trim() ? Number(year) : undefined })}
          />
        </div>
      </div>

      <section className="space-y-2">
        <h2 className="text-sm font-semibold text-muted">Usual order</h2>
        <div className="flex flex-wrap gap-1.5">
          {arrangement.map((id, i) => {
            const section = byId.get(id);
            return (
              <span key={i} className="inline-flex items-center gap-1 rounded-md bg-accent-soft px-2 py-1 text-sm">
                {editable && i > 0 && (
                  <button title="Move earlier" className="text-muted hover:text-white" onClick={() => save({ arrangement: swap(arrangement, i, i - 1) })}>
                    ‹
                  </button>
                )}
                <span title={section ? sectionLabel(section) : undefined}>{section ? sectionCode(section) : "?"}</span>
                {editable && (
                  <button title="Remove from order" className="text-muted hover:text-white" onClick={() => save({ arrangement: arrangement.filter((_, j) => j !== i) })}>
                    ×
                  </button>
                )}
              </span>
            );
          })}
          {arrangement.length === 0 && <span className="text-sm text-muted">No sections in the order yet.</span>}
        </div>
        <p className="text-xs text-muted">
          Services follow this order unless they change it. Use “+ order” on a section to repeat it (e.g. a second chorus). Past services keep the
          version they used.
        </p>
      </section>

      <section className="space-y-3">
        <h2 className="text-sm font-semibold text-muted">Sections</h2>
        {sections.map((section) => (
          <div key={section.id} className="card space-y-2">
            <div className="flex items-center gap-2">
              <EditableText
                className="w-40 font-semibold"
                value={sectionLabel(section)}
                readOnly={!editable}
                onSave={(label) => setSection(section.id, { ...labelToSection(label), color: section.color })}
              />
              {editable && (
                <>
                  <button className="btn ml-auto" onClick={() => save({ arrangement: [...arrangement, section.id] })}>
                    + order
                  </button>
                  <button
                    className="btn"
                    onClick={() => {
                      if (confirm(`Delete “${sectionLabel(section)}”? Past services keep the words they had.`)) {
                        void save({ sections: sections.filter((s) => s.id !== section.id), arrangement: arrangement.filter((id) => id !== section.id) });
                      }
                    }}
                  >
                    Delete
                  </button>
                </>
              )}
            </div>
            <EditableText multiline value={section.lyrics} readOnly={!editable} onSave={(lyrics) => setSection(section.id, { lyrics })} />
          </div>
        ))}
        {editable && (
          <button
            className="btn"
            onClick={() => {
              const id = newId();
              const number = sections.filter((s) => s.kind === "verse").length + 1;
              return save({ sections: [...sections, { id, kind: "verse", number, lyrics: "" }], arrangement: [...arrangement, id] });
            }}
          >
            Add section
          </button>
        )}
      </section>

      {editable && (
        <button
          className="btn border-red-500/50 text-red-400"
          onClick={() => {
            if (!song.archived && !confirm(`Archive “${song.title}”? It leaves the library but stays in services that use it.`)) return;
            void safe(archive)({ songId, archived: !song.archived });
          }}
        >
          {song.archived ? "Restore to library" : "Archive song"}
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
