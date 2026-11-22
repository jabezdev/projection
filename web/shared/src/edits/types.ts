import type { Entry, Section } from "../sections";

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export type ItemContentRecord = { kind: string; songId?: string; versionId?: string; key?: string; arrangement?: Entry[]; [field: string]: unknown };
export type ItemRecord = { id: string; title?: string; notes?: string; background?: string; content: ItemContentRecord };
export type ServiceRecord = { id: string; scheduledAtMs: number; title: string; leader?: string; notes?: string; endedAtMs?: number; items: ItemRecord[] };
export type SongRecord = {
  id: string;
  title: string;
  archived?: boolean;
  credits?: Record<string, unknown>;
  key?: string;
  tags?: string[];
  current: { sections: Section[]; arrangement: string[]; background?: string };
};

export type ServiceField = "scheduledAtMs" | "title" | "leader" | "notes" | "endedAtMs";
export type ItemField = "title" | "notes" | "background" | "content" | "key" | "arrangement" | "versionId";
export type SongField = "title" | "archived" | "credits" | "key" | "tags" | "background" | "arrangement";

export type ServiceOp =
  | { op: "set"; field: ServiceField; before: Json; after: Json }
  | { op: "insert_item"; item: ItemRecord; prev: string | null }
  | { op: "remove_item"; item: ItemRecord; prev: string | null }
  | { op: "move_item"; id: string; from: string | null; to: string | null }
  | { op: "set_item"; id: string; field: ItemField; before: Json; after: Json };

export type SongOp =
  | { op: "set"; field: SongField; before: Json; after: Json }
  | { op: "insert_section"; section: Section; prev: string | null }
  | { op: "remove_section"; section: Section; prev: string | null }
  | { op: "move_section"; id: string; from: string | null; to: string | null }
  | { op: "set_section"; id: string; before: Section; after: Section };

export type Change = { kind: "service"; id: string; ops: ServiceOp[] } | { kind: "song"; id: string; ops: SongOp[] };

export type EditStep = { id: string; atMs: number; label?: string; reverts?: string; change: Change };
