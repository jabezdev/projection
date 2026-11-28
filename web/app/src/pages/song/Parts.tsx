import { newId, SECTION_KINDS, sectionColor, sectionLabel, type Section, type SectionKind } from "@magi/shared";
import { Plus, Trash2 } from "lucide-react";
import { EditableText } from "../../components/EditableText";
import { SlidePreview, slideCount } from "../../components/SlidePreview";
import type { Save } from "./SongPage";

type Props = { sections: Section[]; arrangement: string[]; editable: boolean; save: Save };

export function Parts({ sections, arrangement, editable, save }: Props) {
  const setSection = (id: string, patch: Partial<Section>) => save({ sections: sections.map((s) => (s.id === id ? { ...s, ...patch } : s)) });
  const add = (kind: SectionKind) => {
    const id = newId();
    const number = kind === "verse" ? sections.filter((s) => s.kind === "verse").length + 1 : undefined;
    return save({ sections: [...sections, { id, kind, ...(number ? { number } : {}), lyrics: "" }], arrangement: [...arrangement, id] });
  };
  const remove = (section: Section) => {
    if (confirm(`Delete “${sectionLabel(section)}”? Past services keep the words they had.`)) {
      void save({ sections: sections.filter((s) => s.id !== section.id), arrangement: arrangement.filter((id) => id !== section.id) });
    }
  };

  return (
    <div className="space-y-3">
      {sections.map((section) => (
        <PartCard key={section.id} section={section} uses={arrangement.filter((id) => id === section.id).length} editable={editable} onChange={(patch) => setSection(section.id, patch)} onRemove={() => remove(section)} />
      ))}
      {sections.length === 0 && <p className="text-sm text-faint">No parts yet.</p>}
      {editable && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="mr-1 inline-flex items-center gap-1 text-sm text-faint">
            <Plus size={14} />
            New part
          </span>
          {SECTION_KINDS.map((k) => (
            <button key={k.kind} className="btn" onClick={() => void add(k.kind)}>
              <span className="size-2 rounded-sm" style={{ background: k.color }} />
              {k.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function PartCard({ section, uses, editable, onChange, onRemove }: { section: Section; uses: number; editable: boolean; onChange: (patch: Partial<Section>) => void; onRemove: () => void }) {
  const color = sectionColor(section);
  const slides = slideCount(section.lyrics);
  return (
    <article className="overflow-hidden rounded-xl border border-line bg-card" style={{ boxShadow: `inset 3px 0 0 ${color}` }}>
      <header className="flex flex-wrap items-center gap-2 border-b border-line px-4 py-2.5">
        {editable ? (
          <>
            <select
              aria-label="Kind of part"
              className="py-1 font-semibold"
              value={section.kind}
              onChange={(e) => onChange({ kind: e.target.value as SectionKind, ...(e.target.value === "other" && !section.name ? { name: "Part" } : {}) })}
            >
              {SECTION_KINDS.map((k) => (
                <option key={k.kind} value={k.kind}>
                  {k.label}
                </option>
              ))}
            </select>
            {section.kind !== "other" && (
              <EditableText
                className="w-14 py-1 text-center"
                placeholder="#"
                value={section.number != null ? String(section.number) : ""}
                onSave={(value) => onChange({ number: /^\d+$/.test(value.trim()) ? Number(value) : undefined })}
              />
            )}
            <EditableText
              className="order-last min-w-0 basis-full py-1 sm:order-none sm:w-40 sm:basis-auto"
              placeholder={section.kind === "other" ? "Name, e.g. Interlude" : "Name (optional)"}
              value={section.name ?? ""}
              onSave={(name) => onChange({ name: name.trim() || undefined })}
            />
          </>
        ) : (
          <h3 className="font-semibold" style={{ color }}>
            {sectionLabel(section)}
          </h3>
        )}
        <span className="ml-auto text-xs text-faint">
          {slides} slide{slides === 1 ? "" : "s"}
          {uses === 0 ? " · not in the order" : uses > 1 ? ` · sung ${uses}×` : ""}
        </span>
        {editable && (
          <button className="btn-icon" aria-label={`Delete ${sectionLabel(section)}`} title="Delete part" onClick={onRemove}>
            <Trash2 size={15} />
          </button>
        )}
      </header>
      <div className="grid gap-4 p-4 md:grid-cols-[1fr_15rem]">
        {editable ? (
          <EditableText multiline className="min-h-[6lh] border-transparent bg-sunk text-base" placeholder="Words. A blank line starts a new slide." value={section.lyrics} onSave={(lyrics) => onChange({ lyrics })} />
        ) : (
          <p className="text-base leading-relaxed whitespace-pre-line">{section.lyrics}</p>
        )}
        <div className="hidden md:block">
          <SlidePreview lyrics={section.lyrics} color={color} />
        </div>
      </div>
    </article>
  );
}
