import { expect, test } from "vitest";
import { backend, member } from "../test/members";
import { api, internal } from "./_generated/api";
const DAY = 86_400_000;
const NOV_1 = Date.UTC(2026, 10, 1);

async function setup() {
  const t = backend();
  const admin = await member(t, "Admin", "admin");
  const songId = await admin.mutation(api.songs.create, {
    title: "Amazing Grace",
    credits: { ccli: "22025", authors: [{ name: "John Newton", role: "words" }] },
    sections: [
      { id: "v1", kind: "verse", number: 1, lyrics: "Amazing grace" },
      { id: "c", kind: "chorus", lyrics: "My chains are gone" },
    ],
  });
  const serviceId = await admin.mutation(api.services.create, { scheduledAtMs: NOV_1, title: "Sunday" });
  const itemId = await admin.mutation(api.services.addSong, { serviceId, songId });
  const get = async () => (await admin.query(api.services.get, { serviceId }))!.items[0];
  const song = async () => (await admin.query(api.songs.get, { songId }))!;
  return { t, admin, songId, serviceId, itemId, get, song };
}

const labels = (entries: { kind: string; number?: number }[]) => entries.map((e) => `${e.kind}${e.number ?? ""}`);

test("the first account is an admin with the Sunday template, later ones are singers who can only read", async () => {
  const fresh = backend();
  const first = await fresh.mutation(internal.users.createUser, { method: { name: "usernamePassword", profile: {} } });
  const second = await fresh.mutation(internal.users.createUser, { method: { name: "usernamePassword", profile: {} } });
  const roles = await fresh.run(async (ctx) => [(await ctx.db.get(first))!.role, (await ctx.db.get(second))!.role, (await ctx.db.query("templates").collect()).length]);
  expect(roles).toEqual(["admin", "singer", 1]);
  const { t, serviceId } = await setup();
  const singer = await member(t, "Singer", "singer");
  expect((await singer.query(api.users.me, {}))!.role).toBe("singer");
  expect(await singer.query(api.services.get, { serviceId })).not.toBeNull();
  await expect(singer.mutation(api.services.create, { scheduledAtMs: NOV_1, title: "x" })).rejects.toThrow();
});

test("a service follows the song's arrangement and credits stay on the song", async () => {
  const { get, song } = await setup();
  const item = await get();
  expect(labels(item.entries)).toEqual(["verse1", "chorus"]);
  expect(item.edited).toBe(false);
  expect((await song()).credits.ccli).toBe("22025");
});

test("repeat, move and remove address entries by id", async () => {
  const { admin, itemId, get } = await setup();
  const [verse, chorus] = (await get()).entries;
  await admin.mutation(api.services.insertEntry, { itemId, index: 99, sectionId: "c" });
  await admin.mutation(api.services.moveEntry, { itemId, entryId: verse.entryId, toIndex: 2 });
  expect(labels((await get()).entries)).toEqual(["chorus", "chorus", "verse1"]);
  await admin.mutation(api.services.removeEntry, { itemId, entryId: chorus.entryId });
  expect(labels((await get()).entries)).toEqual(["chorus", "verse1"]);
  await admin.mutation(api.services.resetSequence, { itemId });
  expect(labels((await get()).entries)).toEqual(["verse1", "chorus"]);
});

test("a variation stays in the service until saved to the library", async () => {
  const { admin, songId, itemId, get, song } = await setup();
  const [verse] = (await get()).entries;
  await admin.mutation(api.services.setEntryText, { itemId, entryId: verse.entryId, lyrics: "Amazing GRACE" });
  expect((await get()).entries[0]).toMatchObject({ lyrics: "Amazing GRACE", isVariation: true });
  expect((await song()).current.sections[0].lyrics).toBe("Amazing grace");

  const current = (await song()).current;
  await admin.mutation(api.songs.setContent, { songId, sections: [{ ...current.sections[0], lyrics: "Amazing grace!" }, current.sections[1]], arrangement: current.arrangement });
  expect((await get()).entries[0].lyrics).toBe("Amazing GRACE");

  const entryId = (await get()).entries[0].entryId;
  await admin.mutation(api.services.promoteEntry, { itemId, entryId });
  expect((await get()).entries[0]).toMatchObject({ lyrics: "Amazing GRACE", isVariation: false });
  expect((await song()).current.sections[0].lyrics).toBe("Amazing GRACE");
});

test("library fixes reach upcoming services but not past ones", async () => {
  const { admin, songId, serviceId, get, song } = await setup();
  const next = await admin.mutation(api.services.create, { scheduledAtMs: NOV_1 + 14 * DAY, title: "Later" });
  await admin.mutation(api.services.addSong, { serviceId: next, songId });
  expect(await admin.mutation(api.services.freezePast, { todayStartsAtMs: NOV_1 + 7 * DAY })).toBe(1);
  expect((await admin.query(api.services.get, { serviceId }))!.frozen).toBe(true);

  const current = (await song()).current;
  await admin.mutation(api.songs.setContent, { songId, sections: [current.sections[0], { ...current.sections[1], lyrics: "Fixed" }], arrangement: current.arrangement });
  expect((await song()).current.number).toBe(2);
  expect((await get()).entries[1].lyrics).toBe("My chains are gone");
  expect((await get()).versionNumber).toBe(1);
  expect((await admin.query(api.services.get, { serviceId: next }))!.items[0].entries[1].lyrics).toBe("Fixed");

  const fixed = (await song()).current;
  await admin.mutation(api.songs.setContent, { songId, sections: [fixed.sections[0], { ...fixed.sections[1], lyrics: "Fixed twice" }], arrangement: fixed.arrangement });
  expect((await song()).current).toMatchObject({ id: fixed.id, number: 2 });
});

test("ending a service pins it; archived songs still show in it", async () => {
  const { admin, songId, serviceId, get } = await setup();
  await admin.mutation(api.services.end, { serviceId });
  expect((await get()).versionNumber).toBe(1);
  await admin.mutation(api.songs.archive, { songId, archived: true });
  expect(await admin.query(api.songs.list, {})).toHaveLength(0);
  expect(await admin.query(api.songs.list, { includeArchived: true })).toHaveLength(1);
  expect((await get()).entries.map((e) => e.lyrics)).toEqual(["Amazing grace", "My chains are gone"]);
});

test("pairing codes are single use", async () => {
  const { admin } = await setup();
  const { code } = await admin.mutation(api.devices.createPairingCode, {});
  const { token } = await admin.mutation(api.devices.claimPairingCode, { code: code.toLowerCase(), deviceName: "Booth" });
  expect(token).toHaveLength(64);
  await expect(admin.mutation(api.devices.claimPairingCode, { code, deviceName: "Again" })).rejects.toThrow();
  expect((await admin.query(api.devices.listMine, {})).map((d) => d.name)).toEqual(["Booth"]);
});

test("edits are logged as small operations in the shared format", async () => {
  const { t, admin, songId, serviceId, itemId } = await setup();
  const log = (uid: string) => t.run((ctx) => ctx.db.query("edits").withIndex("by_record", (q) => q.eq("recordUid", uid)).collect());
  await admin.mutation(api.services.update, { serviceId, title: "Sunday AM" });
  await admin.mutation(api.services.updateItem, { itemId, key: "G" });
  await admin.mutation(api.services.removeItem, { itemId });
  expect((await log(serviceId)).map((e) => e.label)).toEqual(["Add Amazing Grace", "Rename service", "Change key of Amazing Grace", "Remove Amazing Grace"]);
  expect((await log(serviceId))[2].change.ops).toEqual([{ op: "set_item", id: itemId, field: "key", before: null, after: "G" }]);

  await admin.mutation(api.songs.archive, { songId, archived: true });
  expect((await log(songId)).map((e) => e.label)).toEqual(["Archive song"]);

  await admin.mutation(api.services.end, { serviceId });
  const before = (await log(serviceId)).length;
  await admin.mutation(api.services.update, { serviceId, title: "Renamed later" });
  expect(await log(serviceId)).toHaveLength(before);
});

test("the web adds Bible passages, text and media, and checks references", async () => {
  const { t, admin, serviceId } = await setup();
  await expect(admin.mutation(api.services.addScripture, { serviceId, scripture: { reference: "Hezekiah 1:1", translation: "kjv" } })).rejects.toThrow("No book called");
  const verseId = await admin.mutation(api.services.addScripture, { serviceId, scripture: { reference: "jn 3:16-17", translation: "KJV", parallel: "adb1905" } });
  const textId = await admin.mutation(api.services.addText, { serviceId, title: "", steps: ["Welcome", "Coffee after"] });
  await t.run((ctx) => ctx.db.insert("media", { uid: "loop", title: "Worship loop", kind: "video", seq: 1 }));
  await admin.mutation(api.services.addMedia, { serviceId, mediaId: "loop" });
  await admin.mutation(api.services.updateItem, { itemId: textId, steps: ["Welcome home"] });
  await admin.mutation(api.services.updateItem, { itemId: verseId, scripture: { reference: "Juan 3:16", translation: "adb1905" } });
  const items = (await admin.query(api.services.get, { serviceId }))!.items;
  expect(items.map((i) => [i.kind, i.title])).toEqual([["song", "Amazing Grace"], ["scripture", "Juan 3:16 · ADB1905"], ["text", "Welcome home"], ["media", "Worship loop"]]);
  expect(items[1].scripture).toEqual({ reference: "Juan 3:16", translation: "adb1905" });
  expect(items[2].steps).toEqual(["Welcome home"]);
});
