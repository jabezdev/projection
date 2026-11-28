import { ConvexError, v } from "convex/values";
import type { Doc } from "./_generated/dataModel";
import { mutation, query, type QueryCtx } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { logged, requireSong, setSongContent, songByUid, touch, versionByUid } from "./lib/sequence";
import { defined } from "./lib/util";
import { credits, section } from "./schema";
import { nextSeq } from "./lib/clock";
import { newId } from "@magi/shared";

async function withVersion(ctx: QueryCtx, song: Doc<"songs">) {
  const current = await versionByUid(ctx, song.currentVersion);
  if (!current) throw new ConvexError("Song has no current version");
  const { uid, title, archived, credits, key, tags } = song;
  return { id: uid, title, archived, credits, key, tags, current: { id: current.uid, number: current.number, sections: current.sections, arrangement: current.arrangement, background: current.background } };
}

export const list = query({
  args: { search: v.optional(v.string()), includeArchived: v.optional(v.boolean()) },
  handler: async (ctx, { search, includeArchived }) => {
    await requirePermission(ctx, "view");
    const term = search?.trim();
    const songs = term
      ? await ctx.db.query("songs").withSearchIndex("search", (q) => (includeArchived ? q.search("searchText", term) : q.search("searchText", term).eq("archived", false))).take(50)
      : (await ctx.db.query("songs").collect()).filter((s) => includeArchived || !s.archived).sort((a, b) => a.title.localeCompare(b.title));
    return songs.map(({ uid, title, archived, credits, key, tags }) => ({ id: uid, title, archived, credits, key, tags }));
  },
});

export const get = query({
  args: { songId: v.string() },
  handler: async (ctx, { songId }) => {
    await requirePermission(ctx, "view");
    const song = await songByUid(ctx, songId);
    return song ? withVersion(ctx, song) : null;
  },
});

export const version = query({
  args: { versionId: v.string() },
  handler: async (ctx, { versionId }) => {
    await requirePermission(ctx, "view");
    const found = await versionByUid(ctx, versionId);
    return found ? { id: found.uid, number: found.number, sections: found.sections, arrangement: found.arrangement, background: found.background } : null;
  },
});

export const create = mutation({
  args: { title: v.string(), credits: v.optional(credits), sections: v.array(section) },
  handler: async (ctx, { title, credits, sections }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const uid = newId();
    const versionUid = newId();
    await ctx.db.insert("songVersions", { uid: versionUid, songUid: uid, number: 1, sections, arrangement: sections.map((s) => s.id) });
    const id = await ctx.db.insert("songs", {
      uid,
      title: title.trim() || "Untitled",
      archived: false,
      credits: credits ?? {},
      tags: [],
      currentVersion: versionUid,
      searchText: "",
      updatedAt: Date.now(),
      updatedBy: user._id,
      seq: await nextSeq(ctx),
    });
    await touch(ctx, id, user._id);
    return uid;
  },
});

export const update = mutation({
  args: {
    songId: v.string(),
    title: v.optional(v.string()),
    credits: v.optional(credits),
    key: v.optional(v.string()),
    tags: v.optional(v.array(v.string())),
  },
  handler: async (ctx, { songId, ...fields }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const song = await requireSong(ctx, songId);
    await logged(ctx, "song", songId, async () => {
      await ctx.db.patch(song._id, defined(fields));
      await touch(ctx, song._id, user._id);
    });
  },
});

export const setContent = mutation({
  args: { songId: v.string(), sections: v.array(section), arrangement: v.array(v.string()), background: v.optional(v.string()) },
  handler: async (ctx, { songId, ...content }) => {
    const user = await requirePermission(ctx, "editLibrary");
    const song = await requireSong(ctx, songId);
    const ids = new Set(content.sections.map((s) => s.id));
    if (content.arrangement.some((id) => !ids.has(id))) throw new ConvexError("The arrangement names a part that isn't in the song");
    return logged(ctx, "song", songId, () => setSongContent(ctx, song, content, user._id));
  },
});

export const archive = mutation({
  args: { songId: v.string(), archived: v.boolean() },
  handler: async (ctx, { songId, archived }) => {
    await requirePermission(ctx, "editLibrary");
    const song = await requireSong(ctx, songId);
    await logged(ctx, "song", songId, () => ctx.db.patch(song._id, { archived }));
  },
});
