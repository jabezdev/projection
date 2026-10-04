import { ConvexError } from "convex/values";
import type { Doc } from "../_generated/dataModel";
import type { QueryCtx } from "../_generated/server";

export type Role = Doc<"users">["role"];

/** Who may do what. Change a list here to change it everywhere. */
const PERMISSIONS = {
  view: ["admin", "librarian", "leader", "operator", "singer"],
  editLibrary: ["admin", "librarian", "leader"],
  arrange: ["admin", "leader", "operator"],
  pairDevice: ["admin", "operator"],
  manageTeam: ["admin"],
} as const satisfies Record<string, readonly Role[]>;

export type Permission = keyof typeof PERMISSIONS;

export function can(role: Role, permission: Permission): boolean {
  return (PERMISSIONS[permission] as readonly Role[]).includes(role);
}

/** The signed-in user's row, or null before `users.ensure` has run. */
export async function currentUser(ctx: QueryCtx): Promise<Doc<"users"> | null> {
  const identity = await ctx.auth.getUserIdentity();
  if (!identity) return null;
  return ctx.db
    .query("users")
    .withIndex("by_token", (q) => q.eq("tokenIdentifier", identity.tokenIdentifier))
    .unique();
}

export async function requirePermission(
  ctx: QueryCtx,
  permission: Permission,
): Promise<Doc<"users">> {
  const user = await currentUser(ctx);
  if (!user) throw new ConvexError("Not signed in");
  if (!can(user.role, permission)) {
    throw new ConvexError(`Your role (${user.role}) can't do this`);
  }
  return user;
}
