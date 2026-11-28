import type { Infer } from "convex/values";
import type { Doc } from "../_generated/dataModel";
import type { MutationCtx, QueryCtx } from "../_generated/server";
import type { section } from "../schema";
import { edits } from "@magi/shared";
import { nextSeq, revive } from "./clock";
import { indexFields, itemsOf, logged, serviceByUid, setSongContent, songByUid, touch, versionByUid, type Content } from "./sequence";

type Section = Infer<typeof section>;
export type WireVersion = { id: string; number: number; sections: Section[]; arrangement: string[]; background?: string };
export type WireSong = Omit<edits.SongRecord, "current"> & { current: WireVersion };
export type WireMedia = { id: string; title: string; kind: Doc<"media">["kind"]; hash?: string };

const VIDEO_DEFAULTS = { confidence: false, looping: false, start_ms: 0, end_ms: null, volume: 100, rate_percent: 100 };
const CAPTURE_DEFAULTS = { projection: true, confidence: false, projection_fit: "fit", confidence_fit: "fit" };

export function versionWire(doc: Doc<"songVersions">): WireVersion {
  return { id: doc.uid, number: doc.number, sections: doc.sections, arrangement: doc.arrangement, ...(doc.background ? { background: doc.background } : {}) };
}

export async function songWire(ctx: QueryCtx, doc: Doc<"songs">): Promise<WireSong | null> {
  const current = await versionByUid(ctx, doc.currentVersion);
  if (!current) return null;
  const { uid, title, archived, credits, key, tags } = doc;
  return { id: uid, title, archived, credits, ...(key ? { key } : {}), tags, current: versionWire(current) };
}

export async function serviceWire(ctx: QueryCtx, doc: Doc<"services">): Promise<edits.ServiceRecord> {
  const items = (await itemsOf(ctx, doc.uid)).map(({ uid, title, notes, background, content }) => strip({ id: uid, title, notes, background, content }));
  return strip({ id: doc.uid, scheduledAtMs: doc.scheduledAtMs, title: doc.title, leader: doc.leader, notes: doc.notes, endedAtMs: doc.endedAtMs, items });
}

export async function mediaWire(ctx: QueryCtx, doc: Doc<"media">) {
  const url = doc.storageId ? await ctx.storage.getUrl(doc.storageId) : null;
  return { media: strip({ id: doc.uid, title: doc.title, kind: doc.kind, hash: doc.hash }), ...(url ? { url } : {}), ...(doc.size ? { size: doc.size } : {}) };
}

export function cleanContent(raw: Record<string, unknown>): Content {
  const kind = raw.kind;
  if (kind === "song") return strip({ kind, songId: String(raw.songId), versionId: raw.versionId, key: raw.key, arrangement: raw.arrangement }) as Content;
  if (kind === "text") return { kind, steps: (raw.steps as string[]) ?? [] };
  if (kind === "media") return { kind, mediaId: String(raw.mediaId), options: { ...VIDEO_DEFAULTS, ...(raw.options as object) } } as Content;
  if (kind === "presentation") return strip({ kind, mediaId: raw.mediaId }) as Content;
  if (kind === "capture") return { kind, options: { ...CAPTURE_DEFAULTS, ...(raw.options as object) } } as Content;
  return { kind: "header" };
}

function cleanItem(item: edits.ItemRecord) {
  return strip({ title: item.title, notes: item.notes, background: item.background, content: cleanContent(item.content) });
}

export async function putService(ctx: MutationCtx, record: edits.ServiceRecord, actor: Doc<"users">) {
  const existing = await serviceByUid(ctx, record.id);
  const fields = { scheduledAtMs: record.scheduledAtMs, title: record.title, leader: record.leader, notes: record.notes, endedAtMs: record.endedAtMs };
  if (!existing) {
    await ctx.db.insert("services", { uid: record.id, ...strip(fields), frozen: false, seq: await nextSeq(ctx) });
    await revive(ctx, "service", record.id);
  }
  const work = async () => {
    const service = (await serviceByUid(ctx, record.id))!;
    await ctx.db.patch(service._id, fields);
    const current = new Map((await itemsOf(ctx, record.id)).map((item) => [item.uid, item]));
    for (const [i, item] of record.items.entries()) {
      const doc = { position: i + 1, ...cleanItem(item) };
      const old = current.get(item.id);
      current.delete(item.id);
      if (old) await ctx.db.replace(old._id, { uid: old.uid, serviceUid: record.id, ...doc, ...indexFields(doc.content) });
      else await ctx.db.insert("serviceItems", { uid: item.id, serviceUid: record.id, ...doc, ...indexFields(doc.content) });
    }
    for (const old of current.values()) await ctx.db.delete(old._id);
  };
  if (existing) await logged(ctx, "service", record.id, work, actor);
  else await work();
}

export async function keepVersions(ctx: MutationCtx, songUid: string, versions: WireVersion[]) {
  for (const version of versions) {
    if (await versionByUid(ctx, version.id)) continue;
    const { id, ...body } = version;
    await ctx.db.insert("songVersions", { uid: id, songUid, ...body });
  }
}

export async function putSong(ctx: MutationCtx, record: WireSong, actor: Doc<"users">, adoptVersion?: string) {
  const existing = await songByUid(ctx, record.id);
  const details = { title: record.title, archived: record.archived ?? false, credits: (record.credits ?? {}) as Doc<"songs">["credits"], key: record.key, tags: record.tags ?? [] };
  const content = { sections: record.current.sections, arrangement: record.current.arrangement, background: record.current.background };
  if (!existing) {
    await keepVersions(ctx, record.id, [record.current]);
    const id = await ctx.db.insert("songs", { uid: record.id, ...details, currentVersion: record.current.id, searchText: "", updatedAt: Date.now(), updatedBy: actor._id, seq: await nextSeq(ctx) });
    await touch(ctx, id, actor._id);
    await revive(ctx, "song", record.id);
    return;
  }
  await logged(ctx, "song", record.id, async () => {
    await ctx.db.patch(existing._id, details);
    const song = (await ctx.db.get(existing._id))!;
    if (adoptVersion && adoptVersion !== song.currentVersion && !(await versionByUid(ctx, adoptVersion))) {
      const last = await ctx.db.query("songVersions").withIndex("by_song", (q) => q.eq("songUid", record.id)).order("desc").first();
      await ctx.db.insert("songVersions", { uid: adoptVersion, songUid: record.id, number: (last?.number ?? 0) + 1, ...content });
      await ctx.db.patch(song._id, { currentVersion: adoptVersion });
      await touch(ctx, song._id, actor._id);
    } else {
      await setSongContent(ctx, song, content, actor._id);
    }
  }, actor);
}

export async function putMedia(ctx: MutationCtx, record: WireMedia) {
  const existing = await ctx.db.query("media").withIndex("by_uid", (q) => q.eq("uid", record.id)).unique();
  const fields = { title: record.title, kind: record.kind, hash: record.hash ?? existing?.hash, seq: await nextSeq(ctx) };
  if (existing) await ctx.db.patch(existing._id, fields);
  else await ctx.db.insert("media", { uid: record.id, ...fields });
  await revive(ctx, "media", record.id);
}

export function strip<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined && v !== null)) as T;
}
