import type { Doc, Id } from "../_generated/dataModel";
import type { MutationCtx, QueryCtx } from "../_generated/server";

export type Entry = Doc<"serviceItems">["sequence"][number];

export type ResolvedEntry = {
  key: string;
  sectionId: Id<"sections"> | null;
  label: string;
  lyrics: string;
  isVariation: boolean;
};

export function resolveEntry(entry: Entry, sections: Map<Id<"sections">, Doc<"sections">>): ResolvedEntry {
  const section = entry.sectionId ? sections.get(entry.sectionId) : undefined;
  return {
    key: entry.key,
    sectionId: section?._id ?? null,
    label: entry.label ?? section?.label ?? "Part",
    lyrics: entry.lyrics ?? section?.lyrics ?? "",
    isVariation: entry.lyrics !== undefined && section !== undefined,
  };
}

export async function songSections(ctx: QueryCtx, songId: Id<"songs">): Promise<Doc<"sections">[]> {
  return ctx.db
    .query("sections")
    .withIndex("by_song", (q) => q.eq("songId", songId))
    .collect();
}

export async function detachSections(
  ctx: MutationCtx,
  songId: Id<"songs">,
  sections: Doc<"sections">[],
  { unlinkSong }: { unlinkSong: boolean },
): Promise<void> {
  const byId = new Map(sections.map((s) => [s._id, s]));
  const items = await ctx.db
    .query("serviceItems")
    .withIndex("by_song", (q) => q.eq("songId", songId))
    .collect();

  for (const item of items) {
    const sequence = item.sequence.map((entry): Entry => {
      const section = entry.sectionId ? byId.get(entry.sectionId) : undefined;
      if (!section) return entry;
      return {
        key: entry.key,
        label: entry.label ?? section.label,
        lyrics: entry.lyrics ?? section.lyrics,
      };
    });
    await ctx.db.patch(item._id, unlinkSong ? { sequence, songId: undefined } : { sequence });
  }
}
