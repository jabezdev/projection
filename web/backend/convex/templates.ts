import { ConvexError, v } from "convex/values";
import type { Doc } from "./_generated/dataModel";
import { mutation, query, type MutationCtx, type QueryCtx } from "./_generated/server";
import { requirePermission } from "./lib/auth";
import { nextSeq, revive, tombstone } from "./lib/clock";
import { cleanContent, strip } from "./lib/records";
import { indexFields, itemsOf, serviceByUid, type Content } from "./lib/sequence";
import { templateFields } from "./schema";
import { newId, sundayTemplate, templateFromService, templateItems, type ServiceTemplate } from "@magi/shared";

export function templateOf(doc: Doc<"templates">): ServiceTemplate {
  const { uid, name, title, weekday, startMinutes, parts } = doc;
  return { id: uid, name, title, weekday, startMinutes, parts } as ServiceTemplate;
}

export async function templateByUid(ctx: QueryCtx, uid: string) {
  return ctx.db.query("templates").withIndex("by_uid", (q) => q.eq("uid", uid)).unique();
}

export async function writeTemplate(ctx: MutationCtx, template: ServiceTemplate) {
  const { id, ...fields } = template;
  const seq = await nextSeq(ctx);
  const existing = await templateByUid(ctx, id);
  const parts = fields.parts.map((part) => strip({ id: part.id, title: part.title, optional: part.optional || undefined, notes: part.notes, items: part.items.map((item) => strip({ id: item.id, title: item.title, notes: item.notes, background: item.background, content: cleanContent(item.content) })) }));
  const doc = strip({ name: fields.name, title: fields.title, weekday: fields.weekday, startMinutes: fields.startMinutes, parts: parts as Doc<"templates">["parts"], seq });
  if (existing) await ctx.db.replace(existing._id, { uid: id, ...doc });
  else await ctx.db.insert("templates", { uid: id, ...doc });
  await revive(ctx, "template", id);
}

export async function seedDefaultTemplate(ctx: MutationCtx) {
  if (!(await ctx.db.query("templates").first())) await writeTemplate(ctx, sundayTemplate());
}

export const list = query({
  args: {},
  handler: async (ctx) => {
    await requirePermission(ctx, "view");
    const all = await ctx.db.query("templates").collect();
    return all.map(templateOf).sort((a, b) => a.name.localeCompare(b.name));
  },
});

export const save = mutation({
  args: { template: v.object({ id: v.string(), ...templateFields }) },
  handler: async (ctx, { template }) => {
    await requirePermission(ctx, "arrange");
    await writeTemplate(ctx, { ...template, name: template.name.trim() || "Template" } as ServiceTemplate);
  },
});

export const remove = mutation({
  args: { templateId: v.string() },
  handler: async (ctx, { templateId }) => {
    await requirePermission(ctx, "arrange");
    const existing = await templateByUid(ctx, templateId);
    if (!existing) return;
    await ctx.db.delete(existing._id);
    await tombstone(ctx, "template", templateId);
  },
});

export const createService = mutation({
  args: { templateId: v.optional(v.string()), parts: v.array(v.string()), scheduledAtMs: v.number(), title: v.string() },
  handler: async (ctx, { templateId, parts, scheduledAtMs, title }) => {
    const user = await requirePermission(ctx, "arrange");
    const template = templateId ? await templateByUid(ctx, templateId) : null;
    if (templateId && !template) throw new ConvexError("That template was deleted");
    const uid = newId();
    const fallback = template?.title ?? template?.name ?? "Service";
    await ctx.db.insert("services", { uid, scheduledAtMs, title: title.trim() || fallback, frozen: false, leaderId: user.role === "leader" ? user._id : undefined, seq: await nextSeq(ctx) });
    const items = template ? templateItems(templateOf(template), parts) : [];
    for (const [i, item] of items.entries()) {
      const content = item.content as Content;
      await ctx.db.insert("serviceItems", { uid: item.id, serviceUid: uid, position: i + 1, title: item.title, notes: item.notes, background: item.background, content, ...indexFields(content) });
    }
    return uid;
  },
});

export const saveFromService = mutation({
  args: { serviceId: v.string(), templateId: v.optional(v.string()), name: v.string(), keepSongs: v.boolean() },
  handler: async (ctx, { serviceId, templateId, name, keepSongs }) => {
    await requirePermission(ctx, "arrange");
    const service = await serviceByUid(ctx, serviceId);
    if (!service) throw new ConvexError("Service not found");
    const items = (await itemsOf(ctx, serviceId)).map(({ uid, title, notes, background, content }) => ({ id: uid, title, notes, background, content }));
    const existing = templateId ? await templateByUid(ctx, templateId) : null;
    const made = templateFromService(templateId ?? newId(), name.trim() || service.title, service.title, items, keepSongs);
    const template = existing ? { ...templateOf(existing), name: made.name, parts: made.parts } : made;
    await writeTemplate(ctx, template);
    return template.id;
  },
});
