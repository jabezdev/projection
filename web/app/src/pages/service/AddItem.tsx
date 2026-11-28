import { api } from "@magi/backend/api";
import { BUNDLED_BIBLES, parseReference, scriptureTitle, splitSlidesText } from "@magi/shared";
import { useMutation, useQuery } from "convex/react";
import { useState } from "react";
import { Segmented } from "../../components/Segmented";
import { useSafe } from "../../lib/errors";
import { AddSong } from "./AddSong";

type Mode = "song" | "bible" | "text" | "media";
const MODES: { value: Mode; label: string }[] = [
  { value: "song", label: "Song" },
  { value: "bible", label: "Bible" },
  { value: "text", label: "Text" },
  { value: "media", label: "Media" },
];

export function AddItem({ serviceId }: { serviceId: string }) {
  const [mode, setMode] = useState<Mode>("song");
  return (
    <div className="space-y-2">
      <Segmented value={mode} options={MODES} onChange={setMode} />
      {mode === "song" && <AddSong serviceId={serviceId} />}
      {mode === "bible" && <AddScripture serviceId={serviceId} />}
      {mode === "text" && <AddText serviceId={serviceId} />}
      {mode === "media" && <AddMedia serviceId={serviceId} />}
    </div>
  );
}

export function TranslationFields({ translation, parallel, onChange }: { translation: string; parallel: string; onChange: (translation: string, parallel: string) => void }) {
  const known = BUNDLED_BIBLES.some((b) => b.id === translation);
  return (
    <div className="flex flex-wrap gap-2 text-sm">
      <select aria-label="Translation" value={known ? translation : "__other"} onChange={(e) => onChange(e.target.value === "__other" ? "" : e.target.value, parallel)}>
        {BUNDLED_BIBLES.map((b) => <option key={b.id} value={b.id}>{b.abbreviation} · {b.name}</option>)}
        <option value="__other">Another Bible PALTO has…</option>
      </select>
      {!known && <input className="w-28" aria-label="Bible abbreviation" placeholder="e.g. NIV" value={translation} onChange={(e) => onChange(e.target.value.toLowerCase(), parallel)} />}
      <select aria-label="Second translation" value={parallel} onChange={(e) => onChange(translation, e.target.value)}>
        <option value="">No second translation</option>
        {BUNDLED_BIBLES.filter((b) => b.id !== translation).map((b) => <option key={b.id} value={b.id}>Side by side: {b.abbreviation}</option>)}
      </select>
    </div>
  );
}

function AddScripture({ serviceId }: { serviceId: string }) {
  const safe = useSafe();
  const add = useMutation(api.services.addScripture);
  const [reference, setReference] = useState("");
  const [translation, setTranslation] = useState("kjv");
  const [parallel, setParallel] = useState("");
  const parsed = reference.trim() ? parseReference(reference) : null;
  return (
    <form
      className="card space-y-2"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!parsed?.ok || !translation) return;
        await safe(add)({ serviceId, scripture: { reference, translation, parallel: parallel || undefined } });
        setReference("");
      }}
    >
      <input className="w-full" placeholder="John 3:16-18, Awit 23, Rom 5:8; 8:28" aria-label="Bible reference" value={reference} onChange={(e) => setReference(e.target.value)} />
      <TranslationFields translation={translation} parallel={parallel} onChange={(t, p) => { setTranslation(t); setParallel(p); }} />
      {parsed && !parsed.ok && <p className="text-sm text-variation">{parsed.error}</p>}
      {parsed?.ok && <p className="text-sm text-muted">{scriptureTitle(reference, translation, parallel)} · PALTO shows one verse per slide</p>}
      <button className="btn-primary" disabled={!parsed?.ok || !translation}>Add passage</button>
    </form>
  );
}

function AddText({ serviceId }: { serviceId: string }) {
  const safe = useSafe();
  const add = useMutation(api.services.addText);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const steps = splitSlidesText(body);
  return (
    <form
      className="card space-y-2"
      onSubmit={async (e) => {
        e.preventDefault();
        await safe(add)({ serviceId, title: title || steps[0]?.split("\n")[0] || "Text", steps });
        setTitle("");
        setBody("");
      }}
    >
      <input className="w-full" placeholder="Title, e.g. Announcements" value={title} onChange={(e) => setTitle(e.target.value)} />
      <textarea className="min-h-32 w-full" placeholder={"Slide text. A blank line starts a new slide."} value={body} onChange={(e) => setBody(e.target.value)} />
      <div className="flex items-center gap-3">
        <button className="btn-primary" disabled={!steps.length}>Add text</button>
        <span className="text-sm text-muted">{steps.length} slide{steps.length === 1 ? "" : "s"}</span>
      </div>
    </form>
  );
}

function AddMedia({ serviceId }: { serviceId: string }) {
  const safe = useSafe();
  const add = useMutation(api.services.addMedia);
  const media = useQuery(api.services.mediaList);
  const [search, setSearch] = useState("");
  const found = (media ?? []).filter((m) => m.title.toLowerCase().includes(search.trim().toLowerCase()));
  return (
    <div className="card space-y-2">
      <input className="w-full" type="search" placeholder="Find a video, image or song track…" value={search} onChange={(e) => setSearch(e.target.value)} />
      {found.slice(0, 30).map((m) => (
        <button key={m.id} className="flex w-full items-baseline gap-3 rounded-lg px-3 py-2 text-left hover:bg-hover" onClick={() => safe(add)({ serviceId, mediaId: m.id })}>
          <span>{m.title}</span>
          <span className="text-sm text-muted capitalize">{m.kind}</span>
        </button>
      ))}
      {media && !media.length && <p className="px-3 text-sm text-muted">No media yet. Files imported in PALTO appear here once it syncs.</p>}
      {media && media.length > 0 && !found.length && <p className="px-3 text-sm text-muted">Nothing matches.</p>}
    </div>
  );
}
