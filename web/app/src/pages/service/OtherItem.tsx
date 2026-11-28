import { api } from "@magi/backend/api";
import { parseReference, splitSlidesText } from "@magi/shared";
import { useMutation } from "convex/react";
import { ArrowDown, ArrowUp, BookOpen, FileText, Film, Monitor, Presentation, Type, X } from "lucide-react";
import { useState } from "react";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";
import { TranslationFields } from "./AddItem";
import type { Item } from "./types";

const KINDS = {
  header: ["Section", FileText],
  text: ["Text", Type],
  media: ["Media", Film],
  presentation: ["Slides · added in PALTO", Presentation],
  capture: ["Screen capture · set up in PALTO", Monitor],
  scripture: ["Bible passage", BookOpen],
  song: ["Song", FileText],
} as const;

export function OtherItem({ item, isFirst, isLast }: { item: Item; isFirst: boolean; isLast: boolean }) {
  const safe = useSafe();
  const moveItem = useMutation(api.services.moveItem);
  const removeItem = useMutation(api.services.removeItem);
  const updateItem = useMutation(api.services.updateItem);
  const [open, setOpen] = useState(false);
  const itemId = item.id;
  const [label, Icon] = KINDS[item.kind];
  const editable = item.kind === "text" || item.kind === "scripture";
  const detail = item.kind === "text" ? `${label} · ${item.steps.length} slide${item.steps.length === 1 ? "" : "s"}` : item.kind === "media" && item.media ? `${label} · ${item.media.kind}` : label;
  return (
    <div className="rounded-xl border border-line bg-card/60 text-sm">
      <div className="flex items-center gap-3 px-3 py-2">
        <span className={`grid size-8 shrink-0 place-items-center rounded-lg ${item.kind === "scripture" ? "bg-[#c9a24a22] text-[#c9a24a]" : "bg-hover text-muted"}`}><Icon size={15} /></span>
        <button className="min-w-0 flex-1 text-left disabled:cursor-default" disabled={!editable} onClick={() => setOpen(!open)} aria-expanded={editable ? open : undefined}>
          <span className="block truncate font-medium">{item.title}</span>
          <span className="text-xs text-faint">{detail}{editable && (open ? " · close" : " · edit")}</span>
        </button>
        <button className="btn-icon" disabled={isFirst} aria-label="Move up" onClick={() => safe(moveItem)({ itemId, direction: -1 })}><ArrowUp size={15} /></button>
        <button className="btn-icon" disabled={isLast} aria-label="Move down" onClick={() => safe(moveItem)({ itemId, direction: 1 })}><ArrowDown size={15} /></button>
        <button className="btn-icon" aria-label="Remove from service" onClick={() => confirm(`Remove “${item.title}” from this service?`) && safe(removeItem)({ itemId })}><X size={15} /></button>
      </div>
      {open && item.kind === "text" && (
        <div className="space-y-2 border-t border-line px-3 py-3">
          <EditableText className="w-full" placeholder="Title" value={item.title ?? ""} onSave={(title) => safe(updateItem)({ itemId, title })} />
          <EditableText multiline className="min-h-32 w-full" placeholder="A blank line starts a new slide" value={item.steps.join("\n\n")} onSave={(text) => safe(updateItem)({ itemId, steps: splitSlidesText(text) })} />
        </div>
      )}
      {open && item.kind === "scripture" && item.scripture && <ScriptureFields itemId={itemId} scripture={item.scripture} />}
    </div>
  );
}

function ScriptureFields({ itemId, scripture }: { itemId: string; scripture: { reference: string; translation: string; parallel?: string } }) {
  const safe = useSafe();
  const updateItem = useMutation(api.services.updateItem);
  const [reference, setReference] = useState(scripture.reference);
  const parsed = parseReference(reference);
  const save = (next: { reference: string; translation: string; parallel?: string }) => safe(updateItem)({ itemId, scripture: next });
  return (
    <div className="space-y-2 border-t border-line px-3 py-3">
      <input className="w-full" aria-label="Bible reference" value={reference} onChange={(e) => setReference(e.target.value)} onBlur={() => parsed.ok && reference !== scripture.reference && save({ ...scripture, reference })} />
      {!parsed.ok && <p className="text-variation">{parsed.error}</p>}
      <TranslationFields translation={scripture.translation} parallel={scripture.parallel ?? ""} onChange={(translation, parallel) => translation && save({ reference: scripture.reference, translation, parallel: parallel || undefined })} />
      <p className="text-xs text-faint">PALTO looks the verses up in its own Bibles and shows one per slide. A Bible PALTO doesn't have falls back to the first one it has.</p>
    </div>
  );
}
