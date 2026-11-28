import { api } from "@magi/backend/api";
import { labelToSection, newId, parseLyrics, sectionColor, sectionLabel } from "@magi/shared";
import { useMutation } from "convex/react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useSafe } from "../lib/errors";

export function NewSong() {
  const navigate = useNavigate();
  const safe = useSafe();
  const create = useMutation(api.songs.create);
  const [title, setTitle] = useState("");
  const [authors, setAuthors] = useState("");
  const [text, setText] = useState("");
  const sections = parseLyrics(text).map((s) => ({ ...labelToSection(s.label), lyrics: s.lyrics }));

  const submit = safe(async () => {
    const names = authors.split(",").map((a) => a.trim()).filter(Boolean);
    const songId = await create({
      title,
      credits: names.length ? { authors: names.map((name) => ({ name })) } : {},
      sections: sections.map((s) => ({ ...s, id: newId() })),
    });
    navigate(`/library/${songId}`, { replace: true });
  });

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold tracking-tight">New song</h1>
      <div className="grid gap-2 sm:grid-cols-2">
        <input placeholder="Title" value={title} onChange={(e) => setTitle(e.target.value)} />
        <input placeholder="Authors, comma separated (optional)" value={authors} onChange={(e) => setAuthors(e.target.value)} />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <label className="space-y-1">
          <span className="text-sm text-muted">
            Paste the lyrics. Lines like “Verse 1”, “[Chorus]” or “Bridge:” start a new part; a blank
            line inside a part starts a new slide.
          </span>
          <textarea
            className="min-h-[50dvh]"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={"Verse 1\nAmazing grace, how sweet the sound\n…\n\nChorus\n…"}
          />
        </label>

        <div className="space-y-2">
          <span className="eyebrow">{sections.length} parts</span>
          {sections.map((section, i) => (
            <div key={i} className="card py-3">
              <div className="mb-1 font-label text-xs font-semibold tracking-[0.12em] uppercase" style={{ color: sectionColor(section) }}>{sectionLabel(section)}</div>
              <p className="text-sm whitespace-pre-line">{section.lyrics}</p>
            </div>
          ))}
        </div>
      </div>

      <button className="btn-primary" disabled={!title.trim()} onClick={submit}>
        Add to library
      </button>
    </div>
  );
}
