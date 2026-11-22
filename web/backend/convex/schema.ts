import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export const role = v.union(
  v.literal("admin"),
  v.literal("librarian"),
  v.literal("leader"),
  v.literal("operator"),
  v.literal("singer"),
);

export const sectionKind = v.union(
  v.literal("verse"),
  v.literal("pre_chorus"),
  v.literal("chorus"),
  v.literal("bridge"),
  v.literal("intro"),
  v.literal("outro"),
  v.literal("tag"),
  v.literal("other"),
);

const sectionFields = {
  number: v.optional(v.number()),
  name: v.optional(v.string()),
  color: v.optional(v.string()),
};

export const section = v.object({ id: v.string(), kind: sectionKind, ...sectionFields, lyrics: v.string() });
export const sectionPatch = v.object({ kind: v.optional(sectionKind), ...sectionFields, lyrics: v.optional(v.string()) });
export const entry = v.object({ id: v.string(), sectionId: v.optional(v.string()), variation: v.optional(sectionPatch) });

export const credits = v.object({
  authors: v.optional(
    v.array(v.object({ name: v.string(), role: v.optional(v.union(v.literal("words"), v.literal("music"), v.literal("translation"))) })),
  ),
  copyright: v.optional(v.string()),
  ccli: v.optional(v.string()),
  publisher: v.optional(v.string()),
  year: v.optional(v.number()),
});

export const videoOptions = v.object({
  confidence: v.boolean(),
  looping: v.boolean(),
  start_ms: v.number(),
  end_ms: v.union(v.number(), v.null()),
  volume: v.number(),
  rate_percent: v.number(),
});

const fit = v.union(v.literal("fit"), v.literal("fill"));
export const captureOptions = v.object({ projection: v.boolean(), confidence: v.boolean(), projection_fit: fit, confidence_fit: fit });

export const songContent = v.object({
  kind: v.literal("song"),
  songId: v.string(),
  versionId: v.optional(v.string()),
  key: v.optional(v.string()),
  arrangement: v.optional(v.array(entry)),
});

export const itemContent = v.union(
  songContent,
  v.object({ kind: v.literal("text"), steps: v.array(v.string()) }),
  v.object({ kind: v.literal("media"), mediaId: v.string(), options: videoOptions }),
  v.object({ kind: v.literal("presentation"), mediaId: v.optional(v.string()) }),
  v.object({ kind: v.literal("capture"), options: captureOptions }),
  v.object({ kind: v.literal("header") }),
);

export const mediaKind = v.union(v.literal("image"), v.literal("video"), v.literal("audio"), v.literal("presentation"));

export default defineSchema({
  users: defineTable({
    tokenIdentifier: v.string(),
    name: v.string(),
    email: v.optional(v.string()),
    role,
  }).index("by_token", ["tokenIdentifier"]),

  songs: defineTable({
    uid: v.string(),
    title: v.string(),
    archived: v.boolean(),
    credits,
    key: v.optional(v.string()),
    tags: v.array(v.string()),
    currentVersion: v.string(),
    searchText: v.string(),
    updatedAt: v.number(),
    updatedBy: v.id("users"),
  })
    .index("by_uid", ["uid"])
    .searchIndex("search", { searchField: "searchText", filterFields: ["archived"] }),

  songVersions: defineTable({
    uid: v.string(),
    songUid: v.string(),
    number: v.number(),
    sections: v.array(section),
    arrangement: v.array(v.string()),
    background: v.optional(v.string()),
  })
    .index("by_uid", ["uid"])
    .index("by_song", ["songUid", "number"]),

  services: defineTable({
    uid: v.string(),
    scheduledAtMs: v.number(),
    title: v.string(),
    leaderId: v.optional(v.id("users")),
    notes: v.optional(v.string()),
    endedAtMs: v.optional(v.number()),
    frozen: v.boolean(),
  })
    .index("by_uid", ["uid"])
    .index("by_time", ["scheduledAtMs"]),

  serviceItems: defineTable({
    uid: v.string(),
    serviceUid: v.string(),
    position: v.number(),
    title: v.optional(v.string()),
    notes: v.optional(v.string()),
    background: v.optional(v.string()),
    content: itemContent,
    songUid: v.optional(v.string()),
    versionUid: v.optional(v.string()),
  })
    .index("by_uid", ["uid"])
    .index("by_service", ["serviceUid", "position"])
    .index("by_song", ["songUid"])
    .index("by_version", ["versionUid"]),

  media: defineTable({
    uid: v.string(),
    title: v.string(),
    kind: mediaKind,
    hash: v.optional(v.string()),
  }).index("by_uid", ["uid"]),

  edits: defineTable({
    uid: v.string(),
    recordUid: v.string(),
    atMs: v.number(),
    by: v.id("users"),
    label: v.optional(v.string()),
    change: v.any(),
  }).index("by_record", ["recordUid", "atMs"]),

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
