import { api } from "@magi/backend/api";
import { type Section } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { Archive, ArchiveRestore, ChevronLeft } from "lucide-react";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { EditableText } from "../../components/EditableText";
import { Segmented } from "../../components/Segmented";
import { useSafe } from "../../lib/errors";
import { useMe } from "../../lib/me";
import { Credits } from "./Credits";
import { Parts } from "./Parts";
import { SongOrder } from "./SongOrder";
import { SongText } from "./SongText";

export type Content = { sections: Section[]; arrangement: string[] };
export type Save = (next: Partial<Content>) => Promise<unknown>;

export function SongPage() {
  const songId = useParams().songId ?? "";
  const song = useQuery(api.songs.get, { songId });
  const me = useMe();
  const safe = useSafe();
  const update = useMutation(api.songs.update);
  const setContent = useMutation(api.songs.setContent);
  const archive = useMutation(api.songs.archive);
  const [mode, setMode] = useState<"parts" | "text">("parts");

  if (song === undefined) return <p className="text-muted">Loading…</p>;
  if (song === null) return <p className="text-muted">This song was not found.</p>;

  const editable = me.can.editLibrary;
  const { sections, arrangement, background } = song.current;
  const save: Save = async (next) =>
    safe(setContent)({ songId, sections: next.sections ?? sections, arrangement: next.arrangement ?? arrangement, background });

  return (
    <div className="space-y-6">
      <div className="space-y-3">
        <Link to="/library" className="inline-flex items-center gap-1 text-sm text-faint hover:text-fg">
          <ChevronLeft size={16} />
          Songs
        </Link>
        <EditableText
          className="w-full border-transparent bg-transparent px-0 text-2xl font-bold tracking-tight focus:border-line focus:px-3"
          value={song.title}
          readOnly={!editable}
          onSave={(title) => safe(update)({ songId, title })}
        />
        {song.archived && (
          <p className="rounded-lg border border-variation/40 bg-variation/10 px-3 py-2 text-sm text-variation">
            Archived: hidden from the library, still shown in services that use it.
          </p>
        )}
        <Credits song={song} editable={editable} />
      </div>

      <SongOrder sections={sections} arrangement={arrangement} editable={editable} save={save} />

      <section className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <h2 className="eyebrow">Parts · {sections.length}</h2>
          {editable && (
            <Segmented
              value={mode}
              onChange={setMode}
              options={[
                { value: "parts", label: "Parts" },
                { value: "text", label: "Whole song" },
              ]}
            />
          )}
        </div>
        {mode === "text" && editable ? (
          <SongText key={song.current.id} content={{ sections, arrangement }} save={save} onDone={() => setMode("parts")} />
        ) : (
          <Parts sections={sections} arrangement={arrangement} editable={editable} save={save} />
        )}
      </section>

      {editable && (
        <div className="border-t border-line pt-5">
          <button
            className="btn text-muted"
            onClick={() => {
              if (!song.archived && !confirm(`Archive “${song.title}”? It leaves the library but stays in services that use it.`)) return;
              void safe(archive)({ songId, archived: !song.archived });
            }}
          >
            {song.archived ? <ArchiveRestore size={15} /> : <Archive size={15} />}
            {song.archived ? "Restore to library" : "Archive song"}
          </button>
        </div>
      )}
    </div>
  );
}
