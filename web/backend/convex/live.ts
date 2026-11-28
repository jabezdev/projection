import { query } from "./_generated/server";
import { requirePermission } from "./lib/auth";

const STALE_MS = 15 * 60 * 1000;

export const nowShowing = query({
  args: {},
  handler: async (ctx) => {
    await requirePermission(ctx, "view");
    const latest = await ctx.db.query("nowShowing").withIndex("by_updated").order("desc").first();
    if (!latest || latest.updatedAt < Date.now() - STALE_MS) return null;
    const { serviceId, serviceTitle, itemTitle, section, text, next, blank, updatedAt } = latest;
    return { serviceId, serviceTitle, itemTitle, section, text, next, blank, updatedAt };
  },
});
