import { sectionLabel } from "../sections";
import type { Change, ServiceOp, SongOp } from "./types";

const SERVICE_FIELDS = { title: "Rename service", scheduledAtMs: "Change date", leader: "Change leader", notes: "Edit service notes", endedAtMs: "End service" };
const SONG_FIELDS = { title: "Rename song", credits: "Edit credits", key: "Change key", tags: "Edit tags", background: "Change background", arrangement: "Change arrangement" };

export function describe(change: Change, itemName: (id: string) => string): string {
  return change.kind === "service" ? service(change.ops, itemName) : song(change.ops);
}

function itemOf(op: ServiceOp): string | null {
  if (op.op === "set") return null;
  return op.op === "insert_item" || op.op === "remove_item" ? op.item.id : op.id;
}

function service(ops: ServiceOp[], name: (id: string) => string): string {
  if (!ops.length) return "No change";
  const [op] = ops;
  if (ops.length === 1) {
    switch (op.op) {
      case "set": return SERVICE_FIELDS[op.field];
      case "insert_item": return `Add ${name(op.item.id)}`;
      case "remove_item": return `Remove ${name(op.item.id)}`;
      case "move_item": return `Move ${name(op.id)}`;
      case "set_item": {
        const n = name(op.id);
        const labels: Record<string, string> = { title: `Rename ${n}`, key: `Change key of ${n}`, arrangement: `Change arrangement of ${n}`, background: `Change background of ${n}`, notes: `Edit notes of ${n}` };
        return labels[op.field] ?? `Edit ${n}`;
      }
    }
  }
  const every = (kind: ServiceOp["op"]) => ops.every((o) => o.op === kind);
  if (every("insert_item")) return `Add ${ops.length} items`;
  if (every("remove_item")) return `Remove ${ops.length} items`;
  if (every("move_item")) return "Reorder items";
  if (every("set")) return "Edit service details";
  const first = itemOf(op);
  return first !== null && ops.every((o) => itemOf(o) === first) ? `Edit ${name(first)}` : "Edit service";
}

function song(ops: SongOp[]): string {
  if (!ops.length) return "No change";
  const [op, ...rest] = ops;
  if (ops.length === 1 && op.op === "set") {
    if (op.field === "archived") return op.after === true ? "Archive song" : "Restore song";
    return SONG_FIELDS[op.field];
  }
  const onlyArrangement = rest.every((o) => o.op === "set" && o.field === "arrangement");
  if (op.op === "insert_section" && onlyArrangement) return `Add ${sectionLabel(op.section)}`;
  if (op.op === "remove_section" && onlyArrangement) return `Remove ${sectionLabel(op.section)}`;
  if (ops.length === 1 && op.op === "set_section") return `Edit ${sectionLabel(op.after)}`;
  if (ops.every((o) => o.op === "move_section")) return "Reorder sections";
  return "Edit song";
}
