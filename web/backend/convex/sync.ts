import { v } from "convex/values";
import type { Doc } from "./_generated/dataModel";
import { mutation, query, type MutationCtx, type QueryCtx } from "./_generated/server";
import { requireDevice } from "./devices";
import { nextSeq, tombstone, type RecordKind } from "./lib/clock";
import { keepVersions, mediaWire, putMedia, putService, putSong, serviceWire, songWire, versionWire, type WireMedia, type WireSong, type WireVersion } from "./lib/records";
import { itemsOf, logged, serviceByUid, songByUid, versionByUid } from "./lib/sequence";
import { templateByUid, templateOf, writeTemplate } from "./templates";
import { edits, type ServiceTemplate } from "@magi/shared";

const PAGE = 100;
const LIVE_MS = 90_000;
const MAX_MEDIA_BYTES = 100 * 1024 * 1024;

type Push =
  | { type: "create"; kind: RecordKind; record: unknown }
  | { type: "change"; change: edits.Change; versions?: WireVersion[]; current?: string }
  | { type: "replace"; kind: RecordKind; base: unknown; record: unknown }
  | { type: "delete"; kind: RecordKind; id: string };

type Conflict = { label: string; mine: unknown; theirs: unknown; keepMine: Push };
type Result = { kind: RecordKind; id: string; conflicts: Conflict[]; record?: unknown; error?: string };

export const hello = query({
  args: { token: v.string() },
  handler: async (ctx, { token }) => {
    const { device, user } = await requireDevice(ctx, token);
    return { deviceName: device.name, userName: user.name, role: user.role };
  },
});

export const pull = query({
  args: { token: v.string(), since: v.number() },
  handler: async (ctx, { token, since }) => {
    await requireDevice(ctx, token);
    const tables = {
      songs: await ctx.db.query("songs").withIndex("by_seq", (q) => q.gt("seq", since)).take(PAGE),
      services: await ctx.db.query("services").withIndex("by_seq", (q) => q.gt("seq", since)).take(PAGE),
      templates: await ctx.db.query("templates").withIndex("by_seq", (q) => q.gt("seq", since)).take(PAGE),
      media: await ctx.db.query("media").withIndex("by_seq", (q) => q.gt("seq", since)).take(PAGE),
      tombstones: await ctx.db.query("tombstones").withIndex("by_seq", (q) => q.gt("seq", since)).take(PAGE),
    };
    const full = Object.values(tables).filter((rows) => rows.length === PAGE);
    const cutoff = full.length ? Math.min(...full.map((rows) => rows[rows.length - 1].seq)) : Infinity;
    const keep = <T extends { seq: number }>(rows: T[]) => rows.filter((row) => row.seq <= cutoff);
    const all = Object.values(tables).flat().map((row) => row.seq);
    const services = await Promise.all(keep(tables.services).map(async (doc) => ({ service: await serviceWire(ctx, doc), frozen: doc.frozen })));
    return {
      cursor: Number.isFinite(cutoff) ? cutoff : Math.max(since, ...all),
      more: Number.isFinite(cutoff),
      songs: (await Promise.all(keep(tables.songs).map((doc) => songWire(ctx, doc)))).filter((song) => song !== null).map((song) => ({ song, versions: [] })),
      services,
      templates: keep(tables.templates).map(templateOf),
      media: await Promise.all(keep(tables.media).map((doc) => mediaWire(ctx, doc))),
      deleted: keep(tables.tombstones).map(({ kind, uid }) => ({ kind, id: uid })),
      pinned: await pinnedVersions(ctx, services.map((s) => s.service)),
    };
  },
});

async function pinnedVersions(ctx: QueryCtx, services: edits.ServiceRecord[]) {
  const ids = new Set(services.flatMap((s) => s.items.flatMap((i) => (i.content.kind === "song" && i.content.versionId ? [i.content.versionId] : []))));
  const found = await Promise.all([...ids].map((id) => versionByUid(ctx, id)));
  return found.filter((doc) => doc !== null).map((doc) => ({ songId: doc.songUid, version: versionWire(doc) }));
}

export const push = mutation({
  args: { token: v.string(), pushes: v.array(v.any()) },
  handler: async (ctx, { token, pushes }) => {
    const { device, user } = await requireDevice(ctx, token);
    await ctx.db.patch(device._id, { lastSeenAt: Date.now() });
    const results: Result[] = [];
    for (const push of pushes as Push[]) results.push(await applyPush(ctx, push, user));
    return results;
  },
});

async function applyPush(ctx: MutationCtx, push: Push, actor: Doc<"users">): Promise<Result> {
  const kind = push.type === "change" ? push.change.kind : push.kind;
  const id = push.type === "change" ? push.change.id : push.type === "delete" ? push.id : recordId(kind, push.record);
  const result: Result = { kind, id, conflicts: [] };
  if (push.type === "delete") {
    await remove(ctx, kind, id, actor);
    return result;
  }
  const existing = await current(ctx, kind, id);
  if (push.type === "change") {
    if (!existing) return { ...result, error: "missing" };
    const record = existing as edits.ServiceRecord | edits.SongRecord;
    const { conflicts } = edits.rebase(push.change, record);
    if (push.change.kind === "service") await putService(ctx, record as edits.ServiceRecord, actor);
    else {
      await keepVersions(ctx, id, push.versions ?? []);
      await putSong(ctx, record as WireSong, actor, push.current);
    }
    const name = (itemId: string) => itemName(record, itemId);
    result.conflicts = conflicts.map(({ op, theirs }) => ({
      label: edits.describe({ kind: push.change.kind, id, ops: [op] } as edits.Change, name),
      mine: (op as { after: unknown }).after,
      theirs,
      keepMine: { type: "change", change: { kind: push.change.kind, id, ops: [{ ...op, before: theirs }] } as edits.Change },
    }));
  } else if (!existing || edits.same(existing, push.record) || (push.type === "replace" && edits.same(existing, push.base))) {
    await put(ctx, kind, push.record, actor);
  } else {
    result.conflicts = [{ label: `${titleOf(push.record)} was changed here and online`, mine: push.record, theirs: existing, keepMine: { type: "replace", kind, base: existing, record: push.record } }];
    return { ...result, record: existing };
  }
  return { ...result, record: await current(ctx, kind, id) };
}

function recordId(kind: RecordKind, record: unknown): string {
  const value = record as { id?: string; song?: { id: string } };
  return kind === "song" && value.song ? value.song.id : String(value.id);
}

async function current(ctx: QueryCtx, kind: RecordKind, id: string): Promise<unknown> {
  if (kind === "service") {
    const doc = await serviceByUid(ctx, id);
    return doc ? serviceWire(ctx, doc) : null;
  }
  if (kind === "song") {
    const doc = await songByUid(ctx, id);
    return doc ? songWire(ctx, doc) : null;
  }
  if (kind === "template") {
    const doc = await templateByUid(ctx, id);
    return doc ? templateOf(doc) : null;
  }
  const doc = await ctx.db.query("media").withIndex("by_uid", (q) => q.eq("uid", id)).unique();
  return doc ? (await mediaWire(ctx, doc)).media : null;
}

async function put(ctx: MutationCtx, kind: RecordKind, record: unknown, actor: Doc<"users">) {
  if (kind === "service") return putService(ctx, record as edits.ServiceRecord, actor);
  if (kind === "template") return writeTemplate(ctx, record as ServiceTemplate);
  if (kind === "media") return putMedia(ctx, record as WireMedia);
  const song = (record as { song?: WireSong }).song ?? (record as WireSong);
  await keepVersions(ctx, song.id, (record as { versions?: WireVersion[] }).versions ?? []);
  return putSong(ctx, song, actor, song.current.id);
}

async function remove(ctx: MutationCtx, kind: RecordKind, id: string, actor: Doc<"users">) {
  if (kind === "song") {
    const song = await songByUid(ctx, id);
    if (song) await logged(ctx, "song", id, () => ctx.db.patch(song._id, { archived: true }), actor);
    return;
  }
  if (kind === "service") {
    const service = await serviceByUid(ctx, id);
    if (!service) return;
    for (const item of await itemsOf(ctx, id)) await ctx.db.delete(item._id);
    await ctx.db.delete(service._id);
  } else if (kind === "template") {
    const template = await templateByUid(ctx, id);
    if (template) await ctx.db.delete(template._id);
  } else {
    const media = await ctx.db.query("media").withIndex("by_uid", (q) => q.eq("uid", id)).unique();
    if (!media) return;
    await ctx.db.delete(media._id);
    const shared = media.hash ? await ctx.db.query("media").withIndex("by_hash", (q) => q.eq("hash", media.hash)).first() : null;
    if (media.storageId && !shared?.storageId) await ctx.storage.delete(media.storageId);
  }
  await tombstone(ctx, kind, id);
}

function titleOf(record: unknown): string {
  const value = record as { title?: string; name?: string; song?: { title: string } };
  return `“${value.song?.title ?? value.title ?? value.name ?? "This record"}”`;
}

function itemName(record: edits.ServiceRecord | edits.SongRecord, itemId: string): string {
  const item = "items" in record ? record.items.find((i) => i.id === itemId) : undefined;
  return item?.title ?? (item?.content.kind === "song" ? "a song" : "an item");
}

export const claimLive = mutation({
  args: { token: v.string(), serviceId: v.optional(v.string()) },
  handler: async (ctx, { token, serviceId }) => {
    const { device } = await requireDevice(ctx, token);
    const now = Date.now();
    await ctx.db.patch(device._id, { lastSeenAt: now });
    for (const held of await ctx.db.query("services").withIndex("by_live", (q) => q.eq("liveDevice", device._id)).collect()) {
      if (held.uid !== serviceId) await ctx.db.patch(held._id, { liveDevice: undefined, liveUntil: undefined });
    }
    const service = serviceId ? await serviceByUid(ctx, serviceId) : null;
    if (!service) return { live: false };
    if (service.liveDevice && service.liveDevice !== device._id && (service.liveUntil ?? 0) > now) {
      const other = await ctx.db.get(service.liveDevice);
      return { live: false, heldBy: other?.name ?? "another computer" };
    }
    await ctx.db.patch(service._id, { liveDevice: device._id, liveUntil: now + LIVE_MS });
    return { live: true };
  },
});

export const mediaUpload = mutation({
  args: { token: v.string(), mediaId: v.string(), hash: v.string(), size: v.number() },
  handler: async (ctx, { token, mediaId, hash, size }) => {
    await requireDevice(ctx, token);
    const twin = await ctx.db.query("media").withIndex("by_hash", (q) => q.eq("hash", hash)).filter((q) => q.neq(q.field("storageId"), undefined)).first();
    const media = await ctx.db.query("media").withIndex("by_uid", (q) => q.eq("uid", mediaId)).unique();
    if (twin?.storageId && media) {
      if (!media.storageId) await ctx.db.patch(media._id, { storageId: twin.storageId, size: twin.size, hash });
      return { status: "exists" as const };
    }
    if (size > MAX_MEDIA_BYTES) return { status: "too_large" as const };
    return { status: "upload" as const, url: await ctx.storage.generateUploadUrl() };
  },
});

export const attachMedia = mutation({
  args: { token: v.string(), mediaId: v.string(), hash: v.string(), storageId: v.id("_storage"), size: v.number() },
  handler: async (ctx, { token, mediaId, hash, storageId, size }) => {
    await requireDevice(ctx, token);
    const media = await ctx.db.query("media").withIndex("by_uid", (q) => q.eq("uid", mediaId)).unique();
    if (!media) {
      await ctx.storage.delete(storageId);
      return false;
    }
    await ctx.db.patch(media._id, { storageId, size, hash, seq: await nextSeq(ctx) });
    return true;
  },
});

export const showing = mutation({
  args: {
    token: v.string(),
    now: v.union(
      v.null(),
      v.object({ serviceId: v.string(), serviceTitle: v.string(), itemTitle: v.optional(v.string()), section: v.optional(v.string()), text: v.string(), next: v.optional(v.string()), blank: v.boolean() }),
    ),
  },
  handler: async (ctx, { token, now }) => {
    const { device } = await requireDevice(ctx, token);
    const old = await ctx.db.query("nowShowing").filter((q) => q.eq(q.field("deviceId"), device._id)).first();
    if (!now) {
      if (old) await ctx.db.delete(old._id);
      return;
    }
    const row = { deviceId: device._id, ...now, updatedAt: Date.now() };
    if (old) await ctx.db.replace(old._id, row);
    else await ctx.db.insert("nowShowing", row);
  },
});
