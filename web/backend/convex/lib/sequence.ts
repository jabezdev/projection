import { ConvexError, type Infer } from "convex/values";
import type { Doc, Id } from "../_generated/dataModel";
import type { MutationCtx, QueryCtx } from "../_generated/server";
import type { itemContent, section } from "../schema";
import { newId } from "@magi/shared";

export type Content = Infer<typeof itemContent>;
export type SongContent = { sections: Infer<typeof section>[]; arrangement: string[]; background?: string };

export async function songByUid(ctx: QueryCtx, uid: string): Promise<Doc<"songs"> | null> {
  return ctx.db.query("songs").withIndex("by_uid", (q) => q.eq("uid", uid)).unique();
}

export async function versionByUid(ctx: QueryCtx, uid: string): Promise<Doc<"songVersions"> | null> {
  return ctx.db.query("songVersions").withIndex("by_uid", (q) => q.eq("uid", uid)).unique();
}

export async function serviceByUid(ctx: QueryCtx, uid: string): Promise<Doc<"services"> | null> {
  return ctx.db.query("services").withIndex("by_uid", (q) => q.eq("uid", uid)).unique();
}

export async function itemByUid(ctx: QueryCtx, uid: string): Promise<Doc<"serviceItems">> {
  const item = await ctx.db.query("serviceItems").withIndex("by_uid", (q) => q.eq("uid", uid)).unique();
  if (!item) throw new ConvexError("This item was removed from the service");
  return item;
}

export async function requireSong(ctx: QueryCtx, uid: string): Promise<Doc<"songs">> {
  const song = await songByUid(ctx, uid);
  if (!song) throw new ConvexError("Song not found");
  return song;
}

export async function itemsOf(ctx: QueryCtx, serviceUid: string): Promise<Doc<"serviceItems">[]> {
  return ctx.db.query("serviceItems").withIndex("by_service", (q) => q.eq("serviceUid", serviceUid)).collect();
}

export function indexFields(content: Content) {
  return content.kind === "song" ? { songUid: content.songId, versionUid: content.versionId } : { songUid: undefined, versionUid: undefined };
}

export async function versionFor(ctx: QueryCtx, content: Extract<Content, { kind: "song" }>): Promise<Doc<"songVersions"> | null> {
  if (content.versionId) return versionByUid(ctx, content.versionId);
  const song = await songByUid(ctx, content.songId);
  return song ? versionByUid(ctx, song.currentVersion) : null;
}

export async function setSongContent(ctx: MutationCtx, song: Doc<"songs">, content: SongContent, userId: Id<"users">): Promise<string> {
  const current = await versionByUid(ctx, song.currentVersion);
  if (!current) throw new ConvexError("Song has no current version");
  const pinned = await ctx.db.query("serviceItems").withIndex("by_version", (q) => q.eq("versionUid", current.uid)).first();
  let versionUid = current.uid;
  if (pinned) {
    versionUid = newId();
    await ctx.db.insert("songVersions", { uid: versionUid, songUid: song.uid, number: current.number + 1, ...content });
    await ctx.db.patch(song._id, { currentVersion: versionUid });
  } else {
    await ctx.db.patch(current._id, { sections: content.sections, arrangement: content.arrangement, background: content.background });
  }
  await touch(ctx, song._id, userId);
  return versionUid;
}

export async function touch(ctx: MutationCtx, songId: Id<"songs">, userId: Id<"users">) {
  const song = await ctx.db.get(songId);
  if (!song) return;
  const version = await versionByUid(ctx, song.currentVersion);
  const authors = song.credits.authors?.map((a) => a.name) ?? [];
  const searchText = [song.title, ...authors, ...(version?.sections.map((s) => s.lyrics) ?? [])].join("\n");
  await ctx.db.patch(songId, { searchText, updatedAt: Date.now(), updatedBy: userId });
}

export async function pinSongs(ctx: MutationCtx, serviceUid: string) {
  for (const item of await itemsOf(ctx, serviceUid)) {
    if (item.content.kind !== "song" || item.content.versionId) continue;
    const song = await songByUid(ctx, item.content.songId);
    if (!song) continue;
    const content = { ...item.content, versionId: song.currentVersion };
    await ctx.db.patch(item._id, { content, ...indexFields(content) });
  }
}

export async function freeze(ctx: MutationCtx, service: Doc<"services">) {
  await pinSongs(ctx, service.uid);
  await ctx.db.patch(service._id, { frozen: true });
}
