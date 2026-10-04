import { api } from "@magi/backend/api";
import { parseLyrics } from "@magi/shared";
import { useMutation } from "convex/react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useSafe } from "../lib/errors";

export function NewSong() {
  const navigate = useNavigate();
  const safe = useSafe();
  const create = useMutation(api.songs.create);
  const [title, setTitle] = useState("");
  const [artist, setArtist] = useState("");
  const [text, setText] = useState("");
  const sections = parseLyrics(text);

  const submit = safe(async () => {
    const songId = await create({ title, artist: artist || undefined, sections });
    navigate(`/library/${songId}`, { replace: true });
  });

  return (
    <div className="space-y-4">
      <h1 className="text-lg font-semibold">New song</h1>
      <div className="grid gap-2 sm:grid-cols-2">
        <input placeholder="Title" value={title} onChange={(e) => setTitle(e.target.value)} />
        <input placeholder="Artist (optional)" value={artist} onChange={(e) => setArtist(e.target.value)} />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <label className="space-y-1">
          <span className="text-sm text-muted">
            Paste the lyrics. Lines like “Verse 1”, “[Chorus]” or “Bridge:” start a new section; a blank
            line inside a section is a slide break.
          </span>
          <textarea
            className="min-h-[50dvh]"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={"Verse 1\nAmazing grace, how sweet the sound\n…\n\nChorus\n…"}
          />
        </label>

        <div className="space-y-2">
          <span className="text-sm text-muted">{sections.length} sections</span>
          {sections.map((section, i) => (
            <div key={i} className="card py-3">
              <div className="mb-1 text-xs font-semibold text-accent uppercase">{section.label}</div>
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
