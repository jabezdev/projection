import { ConvexError, v } from "convex/values";
import type { Doc } from "./_generated/dataModel";
import { mutation, query, type MutationCtx, type QueryCtx } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { assertNotLive, freeze, indexFields, itemByUid, itemsOf, logged, requireSong, serviceByUid, setSongContent, songByUid, versionByUid, versionFor, type Content } from "./lib/sequence";
import { defined } from "./lib/util";
import { sectionKind } from "./schema";
import { nextSeq, tombstone } from "./lib/clock";
import { applyPatch, entriesOf, labelToSection, newId, resolveEntries, sectionLabel, type Entry } from "@magi/shared";

async function requireService(ctx: QueryCtx, uid: string): Promise<Doc<"services">> {
  const service = await serviceByUid(ctx, uid);
  if (!service) throw new ConvexError("Service not found");
  return service;
}

async function describe(ctx: QueryCtx, item: Doc<"serviceItems">) {
  const base = { id: item.uid, kind: item.content.kind, title: item.title ?? null, notes: item.notes, songId: null as string | null, key: undefined as string | undefined, versionNumber: null as number | null, edited: false, entries: [] as ReturnType<typeof resolveEntries>, librarySections: [] as { id: string; label: string }[] };
  if (item.content.kind !== "song") return { ...base, title: item.title ?? item.content.kind };
  const song = await songByUid(ctx, item.content.songId);
  const version = await versionFor(ctx, item.content);
  return {
    ...base,
    title: item.title ?? song?.title ?? "Missing song",
    songId: song ? song.uid : null,
    key: item.content.key ?? song?.key,
    versionNumber: item.content.versionId ? (version?.number ?? null) : null,
    edited: !!item.content.arrangement,
    entries: version ? resolveEntries(item.content.arrangement, version) : [],
    librarySections: version?.sections.map((s) => ({ id: s.id, label: sectionLabel(s) })) ?? [],
  };
}

export const list = query({
  args: {},
  handler: async (ctx) => {
    await requirePermission(ctx, "view");
    const services = await ctx.db.query("services").withIndex("by_time").order("desc").take(100);
    const now = Date.now();
    return services.map(({ uid, scheduledAtMs, title, endedAtMs, frozen, liveUntil }) => ({ id: uid, scheduledAtMs, title, endedAtMs, frozen, live: (liveUntil ?? 0) > now }));
  },
});

export const get = query({
  args: { serviceId: v.string() },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "view");
    const service = await serviceByUid(ctx, serviceId);
    if (!service) return null;
    const leader = service.leaderId ? await ctx.db.get(service.leaderId) : null;
    const items = await Promise.all((await itemsOf(ctx, serviceId)).map((item) => describe(ctx, item)));
    return { id: service.uid, scheduledAtMs: service.scheduledAtMs, title: service.title, notes: service.notes, endedAtMs: service.endedAtMs, frozen: service.frozen, live: (service.liveUntil ?? 0) > Date.now(), leaderName: leader?.name ?? service.leader ?? null, items };
  },
});

export const create = mutation({
  args: { scheduledAtMs: v.number(), title: v.string() },
  handler: async (ctx, { scheduledAtMs, title }) => {
    const user = await requirePermission(ctx, "arrange");
    const uid = newId();
    await ctx.db.insert("services", { uid, scheduledAtMs, title: title.trim() || "Service", frozen: false, leaderId: user.role === "leader" ? user._id : undefined, seq: await nextSeq(ctx) });
    return uid;
  },
});

export const update = mutation({
  args: { serviceId: v.string(), scheduledAtMs: v.optional(v.number()), title: v.optional(v.string()), notes: v.optional(v.string()), leaderId: v.optional(v.id("users")) },
  handler: async (ctx, { serviceId, ...fields }) => {
    await requirePermission(ctx, "arrange");
    const service = await requireService(ctx, serviceId);
    await logged(ctx, "service", serviceId, () => ctx.db.patch(service._id, defined(fields)));
  },
});

export const remove = mutation({
  args: { serviceId: v.string() },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "arrange");
    const service = await requireService(ctx, serviceId);
    assertNotLive(service);
    for (const item of await itemsOf(ctx, serviceId)) await ctx.db.delete(item._id);
    await ctx.db.delete(service._id);
    await tombstone(ctx, "service", serviceId);
  },
});

export const end = mutation({
  args: { serviceId: v.string() },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "arrange");
    const service = await requireService(ctx, serviceId);
    await logged(ctx, "service", serviceId, () => ctx.db.patch(service._id, { endedAtMs: service.endedAtMs ?? Date.now() }));
    await freeze(ctx, service);
  },
});

export const freezePast = mutation({
  args: { todayStartsAtMs: v.number() },
  handler: async (ctx, { todayStartsAtMs }) => {
    await requirePermission(ctx, "view");
    const past = await ctx.db.query("services").withIndex("by_time", (q) => q.lt("scheduledAtMs", todayStartsAtMs)).collect();
    const ended = (await ctx.db.query("services").collect()).filter((s) => s.endedAtMs !== undefined);
    const due = [...past, ...ended].filter((s, i, all) => !s.frozen && all.findIndex((o) => o._id === s._id) === i);
    for (const service of due) await freeze(ctx, service);
    return due.length;
  },
});

async function append(ctx: MutationCtx, service: Doc<"services">, content: Content, title?: string) {
  const items = await itemsOf(ctx, service.uid);
  const uid = newId();
  const position = (items[items.length - 1]?.position ?? 0) + 1;
  await logged(ctx, "service", service.uid, () => ctx.db.insert("serviceItems", { uid, serviceUid: service.uid, position, title, content, ...indexFields(content) }));
  if (service.frozen) await freeze(ctx, service);
  return uid;
}

export const addSong = mutation({
  args: { serviceId: v.string(), songId: v.string() },
  handler: async (ctx, { serviceId, songId }) => {
    await requirePermission(ctx, "arrange");
    const service = await requireService(ctx, serviceId);
    await requireSong(ctx, songId);
    return append(ctx, service, { kind: "song", songId });
  },
});

export const addHeader = mutation({
  args: { serviceId: v.string(), title: v.string() },
  handler: async (ctx, { serviceId, title }) => {
    await requirePermission(ctx, "arrange");
    return append(ctx, await requireService(ctx, serviceId), { kind: "header" }, title);
  },
});

export const updateItem = mutation({
  args: { itemId: v.string(), key: v.optional(v.string()), notes: v.optional(v.string()), title: v.optional(v.string()) },
  handler: async (ctx, { itemId, key, notes, title }) => {
    await requirePermission(ctx, "arrange");
    const item = await itemByUid(ctx, itemId);
    const content = key !== undefined && item.content.kind === "song" ? { ...item.content, key: key.trim() || undefined } : item.content;
    await logged(ctx, "service", item.serviceUid, () => ctx.db.patch(item._id, { ...defined({ notes, title }), content }));
  },
});

export const removeItem = mutation({
  args: { itemId: v.string() },
  handler: async (ctx, { itemId }) => {
    await requirePermission(ctx, "arrange");
    const item = await itemByUid(ctx, itemId);
    await logged(ctx, "service", item.serviceUid, () => ctx.db.delete(item._id));
  },
});

export const moveItem = mutation({
  args: { itemId: v.string(), direction: v.union(v.literal(-1), v.literal(1)) },
  handler: async (ctx, { itemId, direction }) => {
    await requirePermission(ctx, "arrange");
    const item = await itemByUid(ctx, itemId);
    const neighbour = await ctx.db
      .query("serviceItems")
      .withIndex("by_service", (q) =>
        direction < 0 ? q.eq("serviceUid", item.serviceUid).lt("position", item.position) : q.eq("serviceUid", item.serviceUid).gt("position", item.position),
      )
      .order(direction < 0 ? "desc" : "asc")
      .first();
    if (!neighbour) return;
    await logged(ctx, "service", item.serviceUid, async () => {
      await ctx.db.patch(item._id, { position: neighbour.position });
      await ctx.db.patch(neighbour._id, { position: item.position });
    });
  },
});

export const insertEntry = mutation({
  args: { itemId: v.string(), index: v.number(), sectionId: v.optional(v.string()), kind: v.optional(sectionKind), lyrics: v.optional(v.string()) },
  handler: async (ctx, { itemId, index, sectionId, kind, lyrics }) => {
    await editEntries(ctx, itemId, (entries, version) => {
      if (sectionId && !version.sections.some((s) => s.id === sectionId)) throw new ConvexError("That part isn't in this song");
      const entry: Entry = sectionId ? { id: newId(), sectionId } : { id: newId(), variation: { kind: kind ?? "tag", lyrics: lyrics ?? "" } };
      entries.splice(clamp(index, 0, entries.length), 0, entry);
    });
  },
});

export const moveEntry = mutation({
  args: { itemId: v.string(), entryId: v.string(), toIndex: v.number() },
  handler: async (ctx, { itemId, entryId, toIndex }) => {
    await editEntries(ctx, itemId, (entries) => {
      const from = entries.findIndex((e) => e.id === entryId);
      if (from < 0) return;
      const [entry] = entries.splice(from, 1);
      entries.splice(clamp(toIndex, 0, entries.length), 0, entry);
    });
  },
});

export const removeEntry = mutation({
  args: { itemId: v.string(), entryId: v.string() },
  handler: async (ctx, { itemId, entryId }) => {
    await editEntries(ctx, itemId, (entries) => {
      const index = entries.findIndex((e) => e.id === entryId);
      if (index >= 0) entries.splice(index, 1);
    });
  },
});

export const setEntryText = mutation({
  args: { itemId: v.string(), entryId: v.string(), label: v.optional(v.union(v.string(), v.null())), lyrics: v.optional(v.union(v.string(), v.null())) },
  handler: async (ctx, { itemId, entryId, label, lyrics }) => {
    await editEntries(ctx, itemId, (entries) => {
      const entry = entries.find((e) => e.id === entryId);
      if (!entry) return;
      const own = !entry.sectionId;
      const variation = { ...entry.variation };
      if (label !== undefined) {
        const parsed = label === null ? null : labelToSection(label);
        variation.kind = parsed?.kind ?? (own ? "other" : undefined);
        variation.number = parsed?.number;
        variation.name = parsed?.name;
      }
      if (lyrics !== undefined) variation.lyrics = lyrics ?? (own ? "" : undefined);
      const kept = Object.fromEntries(Object.entries(variation).filter(([, value]) => value !== undefined));
      entry.variation = Object.keys(kept).length ? kept : undefined;
    });
  },
});

export const promoteEntry = mutation({
  args: { itemId: v.string(), entryId: v.string() },
  handler: async (ctx, { itemId, entryId }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const item = await itemByUid(ctx, itemId);
    if (item.content.kind !== "song") return;
    const song = await requireSong(ctx, item.content.songId);
    const version = await versionByUid(ctx, song.currentVersion);
    if (!version) return;
    const entries = entriesOf(item.content.arrangement, version).map((e) => ({ ...e }));
    const entry = entries.find((e) => e.id === entryId);
    if (!entry?.sectionId || !entry.variation) return;
    const sections = version.sections.map((s) => (s.id === entry.sectionId ? applyPatch(s.id, entry.variation, s) : s));
    delete entry.variation;
    await logged(ctx, "song", song.uid, () => setSongContent(ctx, song, { sections, arrangement: version.arrangement, background: version.background }, user._id));
    const content = { ...item.content, arrangement: entries };
    await logged(ctx, "service", item.serviceUid, () => ctx.db.patch(item._id, { content, ...indexFields(content) }));
  },
});

export const resetSequence = mutation({
  args: { itemId: v.string() },
  handler: async (ctx, { itemId }) => {
    await requirePermission(ctx, "arrange");
    const item = await itemByUid(ctx, itemId);
    if (item.content.kind !== "song") return;
    const content = { ...item.content, arrangement: undefined };
    await logged(ctx, "service", item.serviceUid, () => ctx.db.patch(item._id, { content, ...indexFields(content) }));
  },
});

async function editEntries(
  ctx: MutationCtx,
  itemId: string,
  edit: (entries: Entry[], version: Doc<"songVersions">) => void,
) {
  await requirePermission(ctx, "arrange");
  const item = await itemByUid(ctx, itemId);
  if (item.content.kind !== "song") throw new ConvexError("Only songs have parts");
  const version = await versionFor(ctx, item.content);
  if (!version) throw new ConvexError("This song is no longer in the library");
  const entries = entriesOf(item.content.arrangement, version).map((e) => ({ ...e }));
  edit(entries, version);
  const content = { ...item.content, arrangement: entries };
  await logged(ctx, "service", item.serviceUid, () => ctx.db.patch(item._id, { content, ...indexFields(content) }));
}

function clamp(n: number, min: number, max: number) {
  return Math.max(min, Math.min(max, Math.trunc(n)));
}
