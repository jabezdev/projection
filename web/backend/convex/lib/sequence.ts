import { ConvexError, type Infer } from "convex/values";
import type { Doc, Id } from "../_generated/dataModel";
import type { MutationCtx, QueryCtx } from "../_generated/server";
import type { itemContent, section } from "../schema";
import { edits, newId } from "@magi/shared";
import { currentUser } from "./auth";

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

export async function serviceRecord(ctx: QueryCtx, uid: string): Promise<edits.ServiceRecord | null> {
  const service = await serviceByUid(ctx, uid);
  if (!service) return null;
  const items = (await itemsOf(ctx, uid)).map(({ uid: id, title, notes, background, content }) => ({ id, title, notes, background, content }));
  return { id: uid, scheduledAtMs: service.scheduledAtMs, title: service.title, notes: service.notes, endedAtMs: service.endedAtMs, items };
}

export async function songRecord(ctx: QueryCtx, uid: string): Promise<edits.SongRecord | null> {
  const song = await songByUid(ctx, uid);
  const version = song && (await versionByUid(ctx, song.currentVersion));
  if (!song || !version) return null;
  const { title, archived, credits, key, tags } = song;
  return { id: uid, title, archived, credits, key, tags, current: { sections: version.sections, arrangement: version.arrangement, background: version.background } };
}

export async function logged<T>(ctx: MutationCtx, kind: "service" | "song", uid: string, work: () => Promise<T>): Promise<T> {
  const read = () => (kind === "service" ? serviceRecord(ctx, uid) : songRecord(ctx, uid));
  const before = await read();
  const frozen = kind === "service" && (await serviceByUid(ctx, uid))?.frozen;
  const result = await work();
  const after = await read();
  const user = await currentUser(ctx);
  if (!before || !after || frozen || !user) return result;
  const change: edits.Change =
    kind === "service"
      ? { kind, id: uid, ops: edits.diffService(before as edits.ServiceRecord, after as edits.ServiceRecord) }
      : { kind, id: uid, ops: edits.diffSong(before as edits.SongRecord, after as edits.SongRecord) };
  if (edits.isEmpty(change)) return result;
  const names = await itemNames(ctx, [...((after as edits.ServiceRecord).items ?? []), ...((before as edits.ServiceRecord).items ?? [])]);
  const atMs = Date.now();
  await ctx.db.insert("edits", { uid: newId(), recordUid: uid, atMs, by: user._id, label: edits.describe(change, (id) => names.get(id) ?? "an item"), change });
  return result;
}

async function itemNames(ctx: QueryCtx, items: edits.ItemRecord[]): Promise<Map<string, string>> {
  const names = new Map<string, string>();
  for (const item of items) {
    if (names.has(item.id)) continue;
    const songId = item.content.kind === "song" ? item.content.songId : undefined;
    const song = !item.title && songId ? await songByUid(ctx, songId) : null;
    names.set(item.id, item.title || song?.title || (item.content.kind === "header" ? "a header" : `a ${item.content.kind} item`));
  }
  return names;
}
