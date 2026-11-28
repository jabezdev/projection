import { expect, test } from "vitest";
import { backend, member } from "../test/members";
import { api } from "./_generated/api";
import { sundayTemplate } from "@magi/shared";

const NOV_1 = Date.UTC(2026, 10, 1);

async function paired() {
  const t = backend();
  const admin = await member(t, "Admin", "admin");
  const { code } = await admin.mutation(api.devices.createPairingCode, {});
  const { token } = await t.mutation(api.devices.claimPairingCode, { code, deviceName: "Church PC" });
  const pull = (since = 0) => t.query(api.sync.pull, { token, since });
  const push = (pushes: unknown[]) => t.mutation(api.sync.push, { token, pushes });
  return { t, admin, token, pull, push };
}

const record = (id: string, title: string, items: unknown[] = []) => ({ id, scheduledAtMs: NOV_1, title, items });
const header = (id: string, title: string) => ({ id, title, content: { kind: "header" } });

test("a computer pushes a new service and pulls only what changed since its cursor", async () => {
  const { pull, push } = await paired();
  const [result] = await push([{ type: "create", kind: "service", record: record("svc", "Sunday", [header("h", "Welcoming")]) }]);
  expect(result.conflicts).toEqual([]);
  const first = await pull();
  expect(first.services.map((s) => s.service.title)).toEqual(["Sunday"]);
  expect(first.services[0].service.items[0].title).toBe("Welcoming");
  const again = await pull(first.cursor);
  expect(again.services).toEqual([]);
  expect(again.more).toBe(false);
});

test("offline edits merge with web edits and only the same field edited twice is a conflict", async () => {
  const { admin, pull, push } = await paired();
  const base = record("svc", "Sunday", [header("a", "Welcome"), header("b", "Offering")]);
  await push([{ type: "create", kind: "service", record: base }]);
  await admin.mutation(api.services.update, { serviceId: "svc", notes: "Bring candles" });
  await admin.mutation(api.services.updateItem, { itemId: "a", title: "Greeting" });
  const ops = [
    { op: "set", field: "title", before: "Sunday", after: "Sunday AM" },
    { op: "set_item", id: "a", field: "title", before: "Welcome", after: "Welcome!" },
  ];
  const [result] = await push([{ type: "change", change: { kind: "service", id: "svc", ops } }]);
  expect(result.conflicts).toHaveLength(1);
  expect(result.conflicts[0]).toMatchObject({ mine: "Welcome!", theirs: "Greeting" });
  const merged = (await pull()).services[0].service;
  expect([merged.title, merged.notes, merged.items[0].title]).toEqual(["Sunday AM", "Bring candles", "Greeting"]);
  await push([result.conflicts[0].keepMine]);
  expect((await pull()).services[0].service.items[0].title).toBe("Welcome!");
});

test("while PALTO runs a service live the web can't edit it, and can again once it lets go", async () => {
  const { admin, token, t, push } = await paired();
  await push([{ type: "create", kind: "service", record: record("svc", "Sunday") }]);
  expect(await t.mutation(api.sync.claimLive, { token, serviceId: "svc" })).toEqual({ live: true });
  await expect(admin.mutation(api.services.update, { serviceId: "svc", title: "Changed" })).rejects.toThrow(/live/);
  expect((await admin.query(api.services.get, { serviceId: "svc" }))!.live).toBe(true);
  await t.mutation(api.sync.claimLive, { token });
  await admin.mutation(api.services.update, { serviceId: "svc", title: "Changed" });
});

test("pages never skip a record, even when one write touched hundreds", async () => {
  const { pull, push } = await paired();
  const songs = Array.from({ length: 230 }, (_, i) => ({
    type: "create",
    kind: "song",
    record: { song: { id: `song${i}`, title: `Song ${i}`, archived: false, credits: {}, tags: [], current: { id: `ver${i}`, number: 1, sections: [{ id: "v", kind: "verse", lyrics: "la" }], arrangement: ["v"] } }, versions: [] },
  }));
  await push(songs);
  const seen = new Set<string>();
  let cursor = 0;
  for (let round = 0; round < 10; round++) {
    const page = await pull(cursor);
    page.songs.forEach((s) => seen.add(s.song.id));
    cursor = page.cursor;
    if (!page.more) break;
  }
  expect(seen.size).toBe(230);
});

test("deletes reach other computers and a song edit made on a pinned version adopts the computer's new version", async () => {
  const { admin, pull, push } = await paired();
  const song = { id: "grace", title: "Grace", archived: false, credits: {}, tags: [], current: { id: "v1", number: 1, sections: [{ id: "v", kind: "verse", lyrics: "old" }], arrangement: ["v"] } };
  await push([{ type: "create", kind: "song", record: { song, versions: [] } }]);
  const newVersion = { id: "v2", number: 2, sections: [{ id: "v", kind: "verse", lyrics: "new" }], arrangement: ["v"] };
  const ops = [{ op: "set_section", id: "v", before: song.current.sections[0], after: newVersion.sections[0] }];
  await push([{ type: "change", change: { kind: "song", id: "grace", ops }, versions: [song.current], current: "v2" }]);
  const pulled = (await admin.query(api.songs.get, { songId: "grace" }))!;
  expect([pulled.current.id, pulled.current.sections[0].lyrics]).toEqual(["v2", "new"]);
  expect((await admin.query(api.songs.version, { versionId: "v1" }))!.sections[0].lyrics).toBe("old");
  await push([{ type: "create", kind: "service", record: record("svc", "Sunday") }, { type: "delete", kind: "service", id: "svc" }]);
  expect((await pull()).deleted).toContainEqual({ kind: "service", id: "svc" });
});

test("a service from a template has the chosen parts, and sections can be removed with or without their items", async () => {
  const { t, admin } = await paired();
  const template = sundayTemplate();
  template.parts[3].items.push({ id: "fixed", title: "Offering slide", content: { kind: "text", steps: ["Give"] } });
  await admin.mutation(api.templates.save, { template: template as never });
  const parts = ["sunday-welcoming", "sunday-worship", "sunday-offering"];
  const serviceId = await admin.mutation(api.templates.createService, { templateId: "sunday", parts, scheduledAtMs: NOV_1, title: "" });
  const titles = async () => (await admin.query(api.services.get, { serviceId }))!.items.map((i) => i.title);
  expect(await titles()).toEqual(["Welcoming", "Praise & Worship", "Tithes and Offering", "Offering slide"]);
  expect((await admin.query(api.services.get, { serviceId }))!.title).toBe("Sunday Service");
  await admin.mutation(api.serviceParts.addPart, { serviceId, templateId: "sunday", partId: "sunday-special-number" });
  expect(await titles()).toEqual(["Welcoming", "Praise & Worship", "Tithes and Offering", "Offering slide", "Special Number"]);
  expect(await admin.mutation(api.serviceParts.removeEmptySections, { serviceId })).toBe(3);
  const items = (await admin.query(api.services.get, { serviceId }))!.items;
  await admin.mutation(api.serviceParts.removeSection, { itemId: items[0].id, withItems: true });
  expect(await titles()).toEqual([]);
  const saved = await admin.mutation(api.templates.saveFromService, { serviceId, name: "Empty", keepSongs: false });
  expect((await admin.query(api.templates.list, {})).map((x) => x.id)).toContain(saved);
  expect(await t.run(async (ctx) => (await ctx.db.query("templates").collect()).length)).toBe(2);
});

test("a template pushed with this computer's slide list is stored without it", async () => {
  const { pull, push } = await paired();
  const template = sundayTemplate();
  template.parts[3].items.push({ id: "slides", title: "Offering", content: { kind: "presentation", slides: [] } as never });
  const [result] = await push([{ type: "create", kind: "template", record: template }]);
  expect(result.error).toBeUndefined();
  const pulled = (await pull()).templates.find((t) => t.id === "sunday")!;
  expect(pulled.parts[3].items[0].content).toEqual({ kind: "presentation" });
});

test("Bible passages from a computer keep their reference and translations", async () => {
  const { pull, push } = await paired();
  const verse = { id: "v", content: { kind: "scripture", reference: "John 3:16", translation: "niv", parallel: "adb1905" } };
  await push([{ type: "create", kind: "service", record: record("svc", "Sunday", [verse]) }]);
  const pulled = await pull();
  expect(pulled.services[0].service.items[0].content).toEqual(verse.content);
});
