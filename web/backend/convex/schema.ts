import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export const role = v.union(
  v.literal("admin"), // manages the team
  v.literal("librarian"), // lyric bank team (Mon–Tue)
  v.literal("leader"), // song leader, arranges services (Wed–Fri)
  v.literal("operator"), // runs the projection app (Sat–Sun)
  v.literal("singer"), // reads lyrics and arrangements
);

/**
 * One occurrence of a section in a service's arrangement. A chorus sung three
 * times is three entries with the same `sectionId` and different `key`s.
 *
 * Label and lyrics come from the library section unless overridden here. An
 * override is a variation for this service only; the library is untouched.
 * An entry with no `sectionId` is a service-only part (e.g. an extra tag),
 * or a library section that has since been deleted.
 */
export const sequenceEntry = v.object({
  key: v.string(),
  sectionId: v.optional(v.id("sections")),
  label: v.optional(v.string()),
  lyrics: v.optional(v.string()),
});

export default defineSchema({
  users: defineTable({
    tokenIdentifier: v.string(),
    name: v.string(),
    email: v.optional(v.string()),
    role,
  }).index("by_token", ["tokenIdentifier"]),

  // ── Song library (lyric bank) ──────────────────────────────────────────
  songs: defineTable({
    title: v.string(),
    artist: v.optional(v.string()),
    ccli: v.optional(v.string()),
    defaultKey: v.optional(v.string()),
    tags: v.array(v.string()),
    /** The order the song is normally sung in; new service items start from it. */
    defaultSequence: v.array(v.id("sections")),
    /** Title + artist + lyrics, kept in sync for full-text search. */
    searchText: v.string(),
    updatedAt: v.number(),
    updatedBy: v.id("users"),
  }).searchIndex("search", { searchField: "searchText" }),

  sections: defineTable({
    songId: v.id("songs"),
    label: v.string(), // "Verse 1", "Chorus", …
    /** Lines of the section. A blank line marks a slide break. */
    lyrics: v.string(),
    order: v.number(),
  }).index("by_song", ["songId", "order"]),

  // ── Services (what the song leader arranges) ───────────────────────────
  services: defineTable({
    date: v.string(), // ISO date of the service, e.g. "2026-10-11"
    title: v.string(),
    leaderId: v.optional(v.id("users")),
    notes: v.optional(v.string()),
  }).index("by_date", ["date"]),

  serviceItems: defineTable({
    serviceId: v.id("services"),
    position: v.number(),
    songId: v.optional(v.id("songs")),
    /** Copied from the song so the item still reads right if the song is deleted. */
    title: v.string(),
    key: v.optional(v.string()),
    notes: v.optional(v.string()),
    sequence: v.array(sequenceEntry),
  })
    .index("by_service", ["serviceId", "position"])
    .index("by_song", ["songId"]),

  // ── Projection computers ───────────────────────────────────────────────
  /** Short-lived code an operator types into the desktop app to pair it. */
  pairingCodes: defineTable({
    code: v.string(),
    userId: v.id("users"),
    expiresAt: v.number(),
  })
    .index("by_code", ["code"])
    .index("by_user", ["userId"]),

  /** A paired desktop app. It acts as the operator who paired it. */
  devices: defineTable({
    name: v.string(),
    userId: v.id("users"),
    tokenHash: v.string(),
    pairedAt: v.number(),
  })
    .index("by_token_hash", ["tokenHash"])
    .index("by_user", ["userId"]),
});
