/**
 * Services and their arrangements.
 *
 * Several people may arrange the same service at once (song leader on their
 * phone, operator at the desk), so every edit is a small operation addressed
 * by entry key, never "replace the whole list". Convex runs mutations as
 * serialized transactions, so concurrent operations all apply instead of the
 * last save overwriting the others.
 */
import { ConvexError, v } from "convex/values";
import type { Doc, Id } from "./_generated/dataModel";
import { mutation, query, type MutationCtx } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { resolveEntry, songSections, type Entry } from "./lib/sequence";
import { defined, newEntryKey } from "./lib/util";

export const list = query({
  args: {},
  handler: async (ctx) => {
    await requirePermission(ctx, "view");
    return ctx.db.query("services").withIndex("by_date").order("desc").take(100);
  },
});

/** A service with every item's arrangement resolved against the library. */
export const get = query({
  args: { serviceId: v.id("services") },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "view");
    const service = await ctx.db.get(serviceId);
    if (!service) return null;

    const leader = service.leaderId ? await ctx.db.get(service.leaderId) : null;
    const items = await ctx.db
      .query("serviceItems")
      .withIndex("by_service", (q) => q.eq("serviceId", serviceId))
      .collect();

    return {
      ...service,
      leaderName: leader?.name ?? null,
      items: await Promise.all(
        items.map(async (item) => {
          const librarySections = item.songId ? await songSections(ctx, item.songId) : [];
          const byId = new Map(librarySections.map((s) => [s._id, s]));
          return {
            _id: item._id,
            songId: item.songId ?? null,
            title: item.title,
            key: item.key,
            notes: item.notes,
            entries: item.sequence.map((entry) => resolveEntry(entry, byId)),
            /** For the "add a section" picker. */
            librarySections: librarySections.map(({ _id, label }) => ({ _id, label })),
          };
        }),
      ),
    };
  },
});

export const create = mutation({
  args: { date: v.string(), title: v.string() },
  handler: async (ctx, { date, title }) => {
    const user = await requirePermission(ctx, "arrange");
    return ctx.db.insert("services", {
      date,
      title: title.trim() || "Service",
      leaderId: user.role === "leader" ? user._id : undefined,
    });
  },
});

export const update = mutation({
  args: {
    serviceId: v.id("services"),
    date: v.optional(v.string()),
    title: v.optional(v.string()),
    notes: v.optional(v.string()),
    leaderId: v.optional(v.id("users")),
  },
  handler: async (ctx, { serviceId, ...fields }) => {
    await requirePermission(ctx, "arrange");
    await ctx.db.patch(serviceId, defined(fields));
  },
});

export const remove = mutation({
  args: { serviceId: v.id("services") },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "arrange");
    const items = await ctx.db
      .query("serviceItems")
      .withIndex("by_service", (q) => q.eq("serviceId", serviceId))
      .collect();
    for (const item of items) await ctx.db.delete(item._id);
    await ctx.db.delete(serviceId);
  },
});

// ── Items ────────────────────────────────────────────────────────────────

export const addSong = mutation({
  args: { serviceId: v.id("services"), songId: v.id("songs") },
  handler: async (ctx, { serviceId, songId }) => {
    await requirePermission(ctx, "arrange");
    const song = await ctx.db.get(songId);
    if (!song) throw new ConvexError("Song not found");
    const last = await ctx.db
      .query("serviceItems")
      .withIndex("by_service", (q) => q.eq("serviceId", serviceId))
      .order("desc")
      .first();
    return ctx.db.insert("serviceItems", {
      serviceId,
      position: (last?.position ?? 0) + 1,
      songId,
      title: song.title,
      key: song.defaultKey,
      sequence: song.defaultSequence.map((sectionId) => ({ key: newEntryKey(), sectionId })),
    });
  },
});

export const updateItem = mutation({
  args: {
    itemId: v.id("serviceItems"),
    key: v.optional(v.string()),
    notes: v.optional(v.string()),
  },
  handler: async (ctx, { itemId, ...fields }) => {
    await requirePermission(ctx, "arrange");
    await ctx.db.patch(itemId, defined(fields));
  },
});

export const removeItem = mutation({
  args: { itemId: v.id("serviceItems") },
  handler: async (ctx, { itemId }) => {
    await requirePermission(ctx, "arrange");
    await ctx.db.delete(itemId);
  },
});

/** Swaps the item with its neighbour above (-1) or below (+1). */
export const moveItem = mutation({
  args: { itemId: v.id("serviceItems"), direction: v.union(v.literal(-1), v.literal(1)) },
  handler: async (ctx, { itemId, direction }) => {
    await requirePermission(ctx, "arrange");
    const item = await ctx.db.get(itemId);
    if (!item) return;
    const neighbour = await ctx.db
      .query("serviceItems")
      .withIndex("by_service", (q) =>
        direction < 0
          ? q.eq("serviceId", item.serviceId).lt("position", item.position)
          : q.eq("serviceId", item.serviceId).gt("position", item.position),
      )
      .order(direction < 0 ? "desc" : "asc")
      .first();
    if (!neighbour) return;
    await ctx.db.patch(item._id, { position: neighbour.position });
    await ctx.db.patch(neighbour._id, { position: item.position });
  },
});

// ── Arrangement (sequence of sections within an item) ───────────────────

/**
 * Inserts a library section, or a service-only part when `sectionId` is
 * omitted. `index` past the end appends.
 */
export const insertEntry = mutation({
  args: {
    itemId: v.id("serviceItems"),
    index: v.number(),
    sectionId: v.optional(v.id("sections")),
    label: v.optional(v.string()),
    lyrics: v.optional(v.string()),
  },
  handler: async (ctx, { itemId, index, sectionId, label, lyrics }) => {
    await editSequence(ctx, itemId, async (item, sequence) => {
      if (sectionId) {
        const section = await ctx.db.get(sectionId);
        if (!section || section.songId !== item.songId) {
          throw new ConvexError("That section isn't part of this song");
        }
      }
      const entry: Entry = sectionId
        ? { key: newEntryKey(), sectionId }
        : { key: newEntryKey(), label: label ?? "Part", lyrics: lyrics ?? "" };
      sequence.splice(clamp(index, 0, sequence.length), 0, entry);
    });
  },
});

export const moveEntry = mutation({
  args: { itemId: v.id("serviceItems"), entryKey: v.string(), toIndex: v.number() },
  handler: async (ctx, { itemId, entryKey, toIndex }) => {
    await editSequence(ctx, itemId, (_item, sequence) => {
      const from = sequence.findIndex((e) => e.key === entryKey);
      if (from < 0) return; // someone else removed it meanwhile
      const [entry] = sequence.splice(from, 1);
      sequence.splice(clamp(toIndex, 0, sequence.length), 0, entry);
    });
  },
});

export const removeEntry = mutation({
  args: { itemId: v.id("serviceItems"), entryKey: v.string() },
  handler: async (ctx, { itemId, entryKey }) => {
    await editSequence(ctx, itemId, (_item, sequence) => {
      const index = sequence.findIndex((e) => e.key === entryKey);
      if (index >= 0) sequence.splice(index, 1);
    });
  },
});

/**
 * Sets this service's variation of an entry's lyrics/label.
 * `null` reverts to the library version (for library sections).
 */
export const setEntryText = mutation({
  args: {
    itemId: v.id("serviceItems"),
    entryKey: v.string(),
    label: v.optional(v.union(v.string(), v.null())),
    lyrics: v.optional(v.union(v.string(), v.null())),
  },
  handler: async (ctx, { itemId, entryKey, label, lyrics }) => {
    await editSequence(ctx, itemId, (_item, sequence) => {
      const entry = sequence.find((e) => e.key === entryKey);
      if (!entry) return;
      // Service-only parts have no library text to fall back to.
      const canRevert = entry.sectionId !== undefined;
      if (label !== undefined) entry.label = label ?? (canRevert ? undefined : "Part");
      if (lyrics !== undefined) entry.lyrics = lyrics ?? (canRevert ? undefined : "");
    });
  },
});

/** Makes this service's variation the library version for everyone. */
export const promoteEntry = mutation({
  args: { itemId: v.id("serviceItems"), entryKey: v.string() },
  handler: async (ctx, { itemId, entryKey }) => {
    await requirePermission(ctx, "editLibrary");
    await editSequence(ctx, itemId, async (_item, sequence) => {
      const entry = sequence.find((e) => e.key === entryKey);
      if (!entry?.sectionId) return;
      await ctx.db.patch(entry.sectionId, defined({ label: entry.label, lyrics: entry.lyrics }));
      delete entry.label;
      delete entry.lyrics;
    });
  },
});

/** Throws away this service's arrangement and starts again from the song's default. */
export const resetSequence = mutation({
  args: { itemId: v.id("serviceItems") },
  handler: async (ctx, { itemId }) => {
    await editSequence(ctx, itemId, async (item, sequence) => {
      const song = item.songId ? await ctx.db.get(item.songId) : null;
      if (!song) return;
      sequence.splice(
        0,
        sequence.length,
        ...song.defaultSequence.map((sectionId) => ({ key: newEntryKey(), sectionId })),
      );
    });
  },
});

async function editSequence(
  ctx: MutationCtx,
  itemId: Id<"serviceItems">,
  edit: (item: Doc<"serviceItems">, sequence: Entry[]) => void | Promise<void>,
) {
  await requirePermission(ctx, "arrange");
  const item = await ctx.db.get(itemId);
  if (!item) throw new ConvexError("This song was removed from the service");
  const sequence = item.sequence.map((e) => ({ ...e }));
  await edit(item, sequence);
  await ctx.db.patch(itemId, { sequence });
}

function clamp(n: number, min: number, max: number) {
  return Math.max(min, Math.min(max, Math.trunc(n)));
}
