import { ConvexError, v } from "convex/values";
import { mutation, query } from "./_generated/server";
import { can, currentUser, requirePermission } from "./lib/auth";
import { role } from "./schema";

/**
 * Creates the user row on first sign-in, refreshes name/email after that.
 * The first person to sign in becomes admin; everyone else starts as a singer
 * until an admin gives them a role.
 */
export const ensure = mutation({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) throw new ConvexError("Not signed in");

    const name = identity.name ?? identity.email ?? "Unnamed";
    const email = identity.email;
    const existing = await currentUser(ctx);
    if (existing) {
      if (existing.name !== name || existing.email !== email) {
        await ctx.db.patch(existing._id, { name, email });
      }
      return existing._id;
    }

    const anyone = await ctx.db.query("users").first();
    return ctx.db.insert("users", {
      tokenIdentifier: identity.tokenIdentifier,
      name,
      email,
      role: anyone ? "singer" : "admin",
    });
  },
});

/** The signed-in user plus what their role allows, for the UI to show/hide things. */
export const me = query({
  args: {},
  handler: async (ctx) => {
    const user = await currentUser(ctx);
    if (!user) return null;
    return {
      ...user,
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
    return users
      .map(({ _id, name, email, role }) => ({ _id, name, email, role }))
      .sort((a, b) => a.name.localeCompare(b.name));
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
