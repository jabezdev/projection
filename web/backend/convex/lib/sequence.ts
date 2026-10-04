import type { Doc, Id } from "../_generated/dataModel";
import type { MutationCtx, QueryCtx } from "../_generated/server";

export type Entry = Doc<"serviceItems">["sequence"][number];

/** An entry with its label and lyrics filled in from the library. */
export type ResolvedEntry = {
  key: string;
  sectionId: Id<"sections"> | null;
  label: string;
  lyrics: string;
  /** Lyrics differ from the library for this service. */
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

/**
 * Before a library section (or a whole song) is deleted, copies its label and
 * lyrics into every service entry that uses it, so arrangements keep reading
 * the same instead of losing that part.
 */
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
