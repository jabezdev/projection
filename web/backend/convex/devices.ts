import { ConvexError, v } from "convex/values";
import type { Doc } from "./_generated/dataModel";
import { mutation, query, type QueryCtx } from "./_generated/server";
import { can, requirePermission } from "./lib/auth";
import { randomToken, sha256Hex } from "./lib/util";

const CODE_TTL_MS = 10 * 60 * 1000;
const CODE_ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";

export const createPairingCode = mutation({
  args: {},
  handler: async (ctx) => {
    const user = await requirePermission(ctx, "pairDevice");

    const old = await ctx.db
      .query("pairingCodes")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    for (const code of old) await ctx.db.delete(code._id);

    const bytes = new Uint8Array(8);
    crypto.getRandomValues(bytes);
    const raw = Array.from(bytes, (b) => CODE_ALPHABET[b % CODE_ALPHABET.length]).join("");
    const code = `${raw.slice(0, 4)}-${raw.slice(4)}`;
    const expiresAt = Date.now() + CODE_TTL_MS;
    await ctx.db.insert("pairingCodes", { code, userId: user._id, expiresAt });
    return { code, expiresAt };
  },
});

export const claimPairingCode = mutation({
  args: { code: v.string(), deviceName: v.string() },
  handler: async (ctx, { code, deviceName }) => {
    const normalized = code.trim().toUpperCase().replace(/[^A-Z0-9]/g, "");
    const formatted = `${normalized.slice(0, 4)}-${normalized.slice(4)}`;
    const pairing = await ctx.db
      .query("pairingCodes")
      .withIndex("by_code", (q) => q.eq("code", formatted))
      .unique();
    if (!pairing || pairing.expiresAt < Date.now()) {
      throw new ConvexError("That code is wrong or has expired");
    }
    await ctx.db.delete(pairing._id);

    const token = randomToken(32);
    await ctx.db.insert("devices", {
      name: deviceName.trim() || "Projection computer",
      userId: pairing.userId,
      tokenHash: await sha256Hex(token),
      pairedAt: Date.now(),
    });
    return { token };
  },
});

export const listMine = query({
  args: {},
  handler: async (ctx) => {
    const user = await requirePermission(ctx, "pairDevice");
    const devices = await ctx.db
      .query("devices")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    return devices.map(({ _id, name, pairedAt, lastSeenAt }) => ({ _id, name, pairedAt, lastSeenAt }));
  },
});

export const revoke = mutation({
  args: { deviceId: v.id("devices") },
  handler: async (ctx, { deviceId }) => {
    const user = await requirePermission(ctx, "pairDevice");
    const device = await ctx.db.get(deviceId);
    if (device?.userId !== user._id && user.role !== "admin") {
      throw new ConvexError("Not your device");
    }
    if (device) await ctx.db.delete(deviceId);
  },
});

export async function requireDevice(ctx: QueryCtx, token: string): Promise<{ device: Doc<"devices">; user: Doc<"users"> }> {
  const tokenHash = await sha256Hex(token);
  const device = await ctx.db
    .query("devices")
    .withIndex("by_token_hash", (q) => q.eq("tokenHash", tokenHash))
    .unique();
  const user = device ? await ctx.db.get(device.userId) : null;
  if (!device || !user || !can(user.role, "arrange")) {
    throw new ConvexError("This computer isn't paired, or its operator lost access");
  }
  return { device, user };
}
