import { ConvexError, v } from "convex/values";
import type { Id } from "./_generated/dataModel";
import { mutation, query, type MutationCtx } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { detachSections, songSections } from "./lib/sequence";
import { defined } from "./lib/util";

export const list = query({
  args: { search: v.optional(v.string()) },
  handler: async (ctx, { search }) => {
    await requirePermission(ctx, "view");
    const term = search?.trim();
    const songs = term
      ? await ctx.db
          .query("songs")
          .withSearchIndex("search", (q) => q.search("searchText", term))
          .take(50)
      : (await ctx.db.query("songs").collect()).sort((a, b) => a.title.localeCompare(b.title));
    return songs.map(({ _id, title, artist, defaultKey, tags }) => ({
      _id,
      title,
      artist,
      defaultKey,
      tags,
    }));
  },
});

export const get = query({
  args: { songId: v.id("songs") },
  handler: async (ctx, { songId }) => {
    await requirePermission(ctx, "view");
    const song = await ctx.db.get(songId);
    if (!song) return null;
    return { ...song, sections: await songSections(ctx, songId) };
  },
});

export const create = mutation({
  args: {
    title: v.string(),
    artist: v.optional(v.string()),
    sections: v.array(v.object({ label: v.string(), lyrics: v.string() })),
  },
  handler: async (ctx, { title, artist, sections }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const songId = await ctx.db.insert("songs", {
      title: title.trim() || "Untitled",
      artist,
      tags: [],
      defaultSequence: [],
      searchText: "",
      updatedAt: Date.now(),
      updatedBy: user._id,
    });
    const sectionIds = [];
    for (const [order, section] of sections.entries()) {
      sectionIds.push(await ctx.db.insert("sections", { songId, order, ...section }));
    }
    await ctx.db.patch(songId, { defaultSequence: sectionIds });
    await touch(ctx, songId, user._id);
    return songId;
  },
});

export const update = mutation({
  args: {
    songId: v.id("songs"),
    title: v.optional(v.string()),
    artist: v.optional(v.string()),
    ccli: v.optional(v.string()),
    defaultKey: v.optional(v.string()),
    tags: v.optional(v.array(v.string())),
  },
  handler: async (ctx, { songId, ...fields }) => {
    const user = await requirePermission(ctx, "editLibrary");
    await ctx.db.patch(songId, defined(fields));
    await touch(ctx, songId, user._id);
  },
});

export const remove = mutation({
  args: { songId: v.id("songs") },
  handler: async (ctx, { songId }) => {
    await requirePermission(ctx, "editLibrary");
    const sections = await songSections(ctx, songId);
    await detachSections(ctx, songId, sections, { unlinkSong: true });
    for (const section of sections) await ctx.db.delete(section._id);
    await ctx.db.delete(songId);
  },
});

/** The order the song is normally sung in. Sections may repeat. */
export const setDefaultSequence = mutation({
  args: { songId: v.id("songs"), sequence: v.array(v.id("sections")) },
  handler: async (ctx, { songId, sequence }) => {
    const user = await requirePermission(ctx, "editLibrary");
    for (const id of sequence) {
      const section = await ctx.db.get(id);
      if (section?.songId !== songId) throw new ConvexError("Section belongs to another song");
    }
    await ctx.db.patch(songId, { defaultSequence: sequence });
    await touch(ctx, songId, user._id);
  },
});

export const addSection = mutation({
  args: { songId: v.id("songs"), label: v.string(), lyrics: v.string() },
  handler: async (ctx, { songId, label, lyrics }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const song = await ctx.db.get(songId);
    if (!song) throw new ConvexError("Song not found");
    const sections = await songSections(ctx, songId);
    const order = sections.length ? sections[sections.length - 1].order + 1 : 0;
    const sectionId = await ctx.db.insert("sections", { songId, label, lyrics, order });
    await ctx.db.patch(songId, { defaultSequence: [...song.defaultSequence, sectionId] });
    await touch(ctx, songId, user._id);
    return sectionId;
  },
});

export const updateSection = mutation({
  args: {
    sectionId: v.id("sections"),
    label: v.optional(v.string()),
    lyrics: v.optional(v.string()),
  },
  handler: async (ctx, { sectionId, ...fields }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const section = await ctx.db.get(sectionId);
    if (!section) throw new ConvexError("Section not found");
    await ctx.db.patch(sectionId, defined(fields));
    await touch(ctx, section.songId, user._id);
  },
});

export const removeSection = mutation({
  args: { sectionId: v.id("sections") },
  handler: async (ctx, { sectionId }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const section = await ctx.db.get(sectionId);
    if (!section) return;
    const song = await ctx.db.get(section.songId);
    await detachSections(ctx, section.songId, [section], { unlinkSong: false });
    if (song) {
      await ctx.db.patch(song._id, {
        defaultSequence: song.defaultSequence.filter((id) => id !== sectionId),
      });
    }
    await ctx.db.delete(sectionId);
    await touch(ctx, section.songId, user._id);
  },
});

/** Refreshes the search text and last-edited stamp after any change. */
async function touch(ctx: MutationCtx, songId: Id<"songs">, userId: Id<"users">) {
  const song = await ctx.db.get(songId);
  if (!song) return;
  const sections = await songSections(ctx, songId);
  const searchText = [song.title, song.artist ?? "", ...sections.map((s) => s.lyrics)].join("\n");
  await ctx.db.patch(songId, { searchText, updatedAt: Date.now(), updatedBy: userId });
}
