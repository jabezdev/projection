import { ConvexError, v } from "convex/values";
import { internalMutation, mutation, query } from "./_generated/server";
import { can, currentUser, requirePermission } from "./lib/auth";
import { role } from "./schema";
import { seedDefaultTemplate } from "./templates";

export const createUser = internalMutation({
  args: { method: v.object({ name: v.string(), profile: v.any() }) },
  handler: async (ctx) => {
    const anyone = await ctx.db.query("users").first();
    if (!anyone) await seedDefaultTemplate(ctx);
    return ctx.db.insert("users", { name: "", role: anyone ? "singer" : "admin" });
  },
});

export const setName = mutation({
  args: { name: v.string() },
  handler: async (ctx, { name }) => {
    const user = await currentUser(ctx);
    if (!user) throw new ConvexError("Not signed in");
    await ctx.db.patch(user._id, { name: name.trim() });
  },
});

export const me = query({
  args: {},
  handler: async (ctx) => {
    const user = await currentUser(ctx);
    if (!user) return null;
    return {
      ...user,
      name: user.name || "Unnamed",
      can: {
        editLibrary: can(user.role, "editLibrary"),
        arrange: can(user.role, "arrange"),
        pairDevice: can(user.role, "pairDevice"),
        manageTeam: can(user.role, "manageTeam"),
      },
    };
  },
});

export const list = query({
  args: {},
  handler: async (ctx) => {
    await requirePermission(ctx, "view");
    const users = await ctx.db.query("users").collect();
    return users.map(({ _id, name, role }) => ({ _id, name: name || "Unnamed", role })).sort((a, b) => a.name.localeCompare(b.name));
  },
});

export const setRole = mutation({
  args: { userId: v.id("users"), role },
  handler: async (ctx, { userId, role }) => {
    await requirePermission(ctx, "manageTeam");
    const target = await ctx.db.get(userId);
    if (!target) throw new ConvexError("User not found");

    if (target.role === "admin" && role !== "admin") {
      const admins = (await ctx.db.query("users").collect()).filter((u) => u.role === "admin");
      if (admins.length <= 1) throw new ConvexError("The team needs at least one admin");
    }
    await ctx.db.patch(userId, { role });
  },
});
