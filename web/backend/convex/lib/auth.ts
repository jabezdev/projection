import { getAuthUserId } from "@convex-dev/auth/server";
import { ConvexError } from "convex/values";
import type { Doc } from "../_generated/dataModel";
import type { QueryCtx } from "../_generated/server";

export type Role = Doc<"users">["role"];

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

export async function currentUser(ctx: QueryCtx): Promise<Doc<"users"> | null> {
  const userId = await getAuthUserId(ctx);
  return userId ? ctx.db.get(userId) : null;
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
