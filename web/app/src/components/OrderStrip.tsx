import { useState, type ReactNode } from "react";

export type Chip = { key: string; code: string; label: string; color: string; flagged?: boolean };

type Props = {
  chips: Chip[];
  selected?: string | null;
  onSelect?: (key: string | null) => void;
  onMove?: (from: number, to: number) => void;
  empty?: string;
  children?: ReactNode;
};

export function OrderStrip({ chips, selected, onSelect, onMove, empty, children }: Props) {
  const [dragged, setDragged] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {chips.map((chip, i) => (
        <button
          key={chip.key}
          type="button"
          title={chip.label}
          draggable={!!onMove}
          onDragStart={(e) => {
            setDragged(i);
            e.dataTransfer.effectAllowed = "move";
          }}
          onDragEnd={() => {
            setDragged(null);
            setOver(null);
          }}
          onDragOver={(e) => {
            if (dragged === null) return;
            e.preventDefault();
            setOver(i);
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragged !== null && dragged !== i) onMove?.(dragged, i);
            setDragged(null);
            setOver(null);
          }}
          onClick={() => onSelect?.(selected === chip.key ? null : chip.key)}
          style={{ borderLeftColor: chip.color }}
          className={`min-w-10 rounded-md border-l-[3px] px-2.5 py-1.5 font-label text-sm font-semibold tracking-wide transition-colors ${
            selected === chip.key ? "bg-fg text-bg" : "bg-hover text-fg hover:bg-line"
          } ${chip.flagged ? "outline outline-1 outline-variation" : ""} ${dragged === i ? "opacity-40" : ""} ${
            over === i && dragged !== i ? "ring-2 ring-accent" : ""
          }`}
        >
          {chip.code}
        </button>
      ))}
      {chips.length === 0 && empty && <span className="text-sm text-faint">{empty}</span>}
      {children}
    </div>
  );
}
