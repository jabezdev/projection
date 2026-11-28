import { ConvexError, v } from "convex/values";
import { mutation } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { indexFields, itemByUid, itemsOf, logged, serviceByUid, type Content } from "./lib/sequence";
import { templateByUid, templateOf } from "./templates";
import { templateItems } from "@magi/shared";

export const removeSection = mutation({
  args: { itemId: v.string(), withItems: v.boolean() },
  handler: async (ctx, { itemId, withItems }) => {
    await requirePermission(ctx, "arrange");
    const header = await itemByUid(ctx, itemId);
    if (header.content.kind !== "header") throw new ConvexError("That item isn't a section");
    const items = await itemsOf(ctx, header.serviceUid);
    const start = items.findIndex((i) => i._id === header._id);
    const end = items.findIndex((i, n) => n > start && i.content.kind === "header");
    const doomed = withItems ? items.slice(start, end < 0 ? items.length : end) : [header];
    await logged(ctx, "service", header.serviceUid, async () => {
      for (const item of doomed) await ctx.db.delete(item._id);
    });
    return doomed.length;
  },
});

export const removeEmptySections = mutation({
  args: { serviceId: v.string() },
  handler: async (ctx, { serviceId }) => {
    await requirePermission(ctx, "arrange");
    const items = await itemsOf(ctx, serviceId);
    const empty = items.filter((item, i) => item.content.kind === "header" && (i === items.length - 1 || items[i + 1].content.kind === "header"));
    await logged(ctx, "service", serviceId, async () => {
      for (const item of empty) await ctx.db.delete(item._id);
    });
    return empty.length;
  },
});

export const addPart = mutation({
  args: { serviceId: v.string(), templateId: v.string(), partId: v.string() },
  handler: async (ctx, { serviceId, templateId, partId }) => {
    await requirePermission(ctx, "arrange");
    const service = await serviceByUid(ctx, serviceId);
    const template = await templateByUid(ctx, templateId);
    if (!service || !template) throw new ConvexError("Service or template not found");
    const shape = templateOf(template);
    const want = shape.parts.findIndex((p) => p.id === partId);
    if (want < 0) throw new ConvexError("That part isn't in the template");
    const rank = new Map(shape.parts.map((p, i) => [p.title, i]));
    const items = await itemsOf(ctx, serviceId);
    let at = 0;
    items.forEach((item, i) => {
      const r = item.content.kind === "header" ? rank.get(item.title ?? "") : undefined;
      if (r !== undefined && r < want) at = nextHeader(items, i);
    });
    const added = templateItems(shape, [partId]);
    const before = items[at - 1]?.position ?? 0;
    const step = ((items[at]?.position ?? before + added.length + 1) - before) / (added.length + 1);
    await logged(ctx, "service", serviceId, async () => {
      for (const [n, item] of added.entries()) {
        const content = item.content as Content;
        await ctx.db.insert("serviceItems", { uid: item.id, serviceUid: serviceId, position: before + step * (n + 1), title: item.title, notes: item.notes, content, ...indexFields(content) });
      }
    });
    return added[0]?.id;
  },
});

function nextHeader(items: { content: { kind: string } }[], index: number) {
  const next = items.findIndex((item, i) => i > index && item.content.kind === "header");
  return next < 0 ? items.length : next;
}
