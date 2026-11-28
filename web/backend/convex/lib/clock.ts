import type { Infer } from "convex/values";
import type { MutationCtx } from "../_generated/server";
import type { recordKind } from "../schema";

export type RecordKind = Infer<typeof recordKind>;

export async function nextSeq(ctx: MutationCtx): Promise<number> {
  const clock = await ctx.db.query("syncClock").first();
  const value = (clock?.value ?? 0) + 1;
  if (clock) await ctx.db.patch(clock._id, { value });
  else await ctx.db.insert("syncClock", { value });
  return value;
}

export async function tombstone(ctx: MutationCtx, kind: RecordKind, uid: string) {
  const seq = await nextSeq(ctx);
  const old = await ctx.db.query("tombstones").withIndex("by_record", (q) => q.eq("kind", kind).eq("uid", uid)).unique();
  if (old) await ctx.db.patch(old._id, { seq });
  else await ctx.db.insert("tombstones", { kind, uid, seq });
}

export async function revive(ctx: MutationCtx, kind: RecordKind, uid: string) {
  const old = await ctx.db.query("tombstones").withIndex("by_record", (q) => q.eq("kind", kind).eq("uid", uid)).unique();
  if (old) await ctx.db.delete(old._id);
}
