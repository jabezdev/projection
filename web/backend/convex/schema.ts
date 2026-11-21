import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export const role = v.union(
  v.literal("admin"),
  v.literal("librarian"),
  v.literal("leader"),
  v.literal("operator"),
  v.literal("singer"),
);

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

  songs: defineTable({
    title: v.string(),
    artist: v.optional(v.string()),
    ccli: v.optional(v.string()),
    defaultKey: v.optional(v.string()),
    tags: v.array(v.string()),
    defaultSequence: v.array(v.id("sections")),
    searchText: v.string(),
    updatedAt: v.number(),
    updatedBy: v.id("users"),
  }).searchIndex("search", { searchField: "searchText" }),

  sections: defineTable({
    songId: v.id("songs"),
    label: v.string(),
    lyrics: v.string(),
    order: v.number(),
  }).index("by_song", ["songId", "order"]),

  services: defineTable({
    date: v.string(),
    title: v.string(),
    leaderId: v.optional(v.id("users")),
    notes: v.optional(v.string()),
  }).index("by_date", ["date"]),

  serviceItems: defineTable({
    serviceId: v.id("services"),
    position: v.number(),
    songId: v.optional(v.id("songs")),
    title: v.string(),
    key: v.optional(v.string()),
    notes: v.optional(v.string()),
    sequence: v.array(sequenceEntry),
  })
    .index("by_service", ["serviceId", "position"])
    .index("by_song", ["songId"]),

  pairingCodes: defineTable({
    code: v.string(),
    userId: v.id("users"),
    expiresAt: v.number(),
  })
    .index("by_code", ["code"])
    .index("by_user", ["userId"]),

  devices: defineTable({
    name: v.string(),
    userId: v.id("users"),
    tokenHash: v.string(),
    pairedAt: v.number(),
  })
    .index("by_token_hash", ["tokenHash"])
    .index("by_user", ["userId"]),
});
