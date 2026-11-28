import { sectionCode, sectionColor, songToText, textToSong } from "@magi/shared";
import { useState } from "react";
import { slideCount } from "../../components/SlidePreview";
import type { Content, Save } from "./SongPage";

export function SongText({ content, save, onDone }: { content: Content; save: Save; onDone: () => void }) {
  const [original] = useState(() => songToText(content.sections));
  const [text, setText] = useState(original);
  const [busy, setBusy] = useState(false);
  const parsed = textToSong(text, content);
  const changed = text !== original;
  const removed = content.sections.filter((s) => !parsed.sections.some((p) => p.id === s.id));

  return (
    <div className="grid gap-4 md:grid-cols-[1fr_14rem]">
      <div className="space-y-2">
        <p className="text-xs text-faint">
          The whole song as text. A line in brackets, like <code className="text-muted">[Chorus]</code> or <code className="text-muted">[Verse 2]</code>, starts a part; a blank line inside a part starts a new slide.
        </p>
        <textarea className="min-h-[60dvh] bg-sunk font-mono text-sm leading-relaxed" spellCheck value={text} onChange={(e) => setText(e.target.value)} />
        <div className="flex flex-wrap items-center gap-2">
          <button
            className="btn-primary"
            disabled={!changed || busy}
            onClick={async () => {
              if (removed.length && !confirm(`This removes ${removed.map(sectionCode).join(", ")}. Past services keep the words they had. Continue?`)) return;
              setBusy(true);
              await save(parsed);
              setBusy(false);
              onDone();
            }}
          >
            Save song
          </button>
          <button className="btn" onClick={onDone}>
            {changed ? "Discard" : "Back to parts"}
          </button>
          {removed.length > 0 && <span className="text-xs text-variation">Removes {removed.map(sectionCode).join(", ")}</span>}
        </div>
      </div>
      <aside className="space-y-1.5">
        <h3 className="eyebrow text-faint">Will save as</h3>
        {parsed.sections.map((s) => (
          <div key={s.id} className="flex items-center gap-2 rounded-md bg-card px-2 py-1.5 text-sm" style={{ boxShadow: `inset 3px 0 0 ${sectionColor(s)}` }}>
            <span className="font-label font-semibold">{sectionCode(s)}</span>
            <span className="text-xs text-faint">
              {slideCount(s.lyrics)} slide{slideCount(s.lyrics) === 1 ? "" : "s"}
            </span>
            {!content.sections.some((c) => c.id === s.id) && <span className="ml-auto text-xs text-accent">new</span>}
          </div>
        ))}
      </aside>
    </div>
  );
}
