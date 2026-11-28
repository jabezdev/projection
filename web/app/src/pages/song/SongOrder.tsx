import { sectionCode, sectionColor, sectionLabel, type Section } from "@magi/shared";
import { ChevronLeft, ChevronRight, Plus, X } from "lucide-react";
import { useState } from "react";
import { OrderStrip } from "../../components/OrderStrip";
import type { Save } from "./SongPage";

export function SongOrder({ sections, arrangement, editable, save }: { sections: Section[]; arrangement: string[]; editable: boolean; save: Save }) {
  const [selected, setSelected] = useState<number | null>(null);
  const byId = new Map(sections.map((s) => [s.id, s]));
  const chips = arrangement.map((id, i) => {
    const section = byId.get(id);
    return { key: String(i), code: section ? sectionCode(section) : "?", label: section ? sectionLabel(section) : "Missing part", color: section ? sectionColor(section) : "#74736e" };
  });
  const move = (from: number, to: number) => {
    const next = [...arrangement];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    setSelected(to);
    return save({ arrangement: next });
  };
  const current = selected !== null && selected < arrangement.length ? selected : null;

  return (
    <section className="card space-y-3">
      <div>
        <h2 className="eyebrow">Usual order</h2>
        <p className="mt-1 text-xs text-faint">
          Services follow this order unless they change it.{editable && " Drag to reorder, or tap a part to move or remove it."}
        </p>
      </div>
      <OrderStrip
        chips={chips}
        selected={current === null ? null : String(current)}
        onSelect={editable ? (key) => setSelected(key === null ? null : Number(key)) : undefined}
        onMove={editable ? (from, to) => void move(from, to) : undefined}
        empty="No parts in the order yet."
      />
      {editable && current !== null && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg bg-sunk p-2 text-sm">
          <span className="px-1 font-medium">{chips[current].label}</span>
          <span className="grow" />
          <button className="btn-icon" aria-label="Earlier" disabled={current === 0} onClick={() => void move(current, current - 1)}>
            <ChevronLeft size={16} />
          </button>
          <button className="btn-icon" aria-label="Later" disabled={current === arrangement.length - 1} onClick={() => void move(current, current + 1)}>
            <ChevronRight size={16} />
          </button>
          <button
            className="btn"
            onClick={() => {
              setSelected(null);
              void save({ arrangement: arrangement.filter((_, j) => j !== current) });
            }}
          >
            <X size={14} />
            Remove from order
          </button>
        </div>
      )}
      {editable && sections.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-line pt-3">
          <span className="mr-1 text-xs text-faint">Add</span>
          {sections.map((section) => (
            <button
              key={section.id}
              title={`Add ${sectionLabel(section)} to the end`}
              className="inline-flex items-center gap-1 rounded-md border border-dashed border-line px-2 py-1 font-label text-sm font-semibold text-muted hover:border-muted hover:text-fg"
              onClick={() => void save({ arrangement: [...arrangement, section.id] })}
            >
              <Plus size={12} style={{ color: sectionColor(section) }} />
              {sectionCode(section)}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
