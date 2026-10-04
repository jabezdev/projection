import { convexTest } from "convex-test";
import { expect, test } from "vitest";
import { api } from "./_generated/api";
import schema from "./schema";

const modules = import.meta.glob("./**/*.ts");

async function setup() {
  const t = convexTest(schema, modules);
  const admin = t.withIdentity({ tokenIdentifier: "admin", name: "Admin" });
  await admin.mutation(api.users.ensure, {});
  const songId = await admin.mutation(api.songs.create, {
    title: "Amazing Grace",
    sections: [
      { label: "Verse 1", lyrics: "Amazing grace" },
      { label: "Chorus", lyrics: "My chains are gone" },
    ],
  });
  const serviceId = await admin.mutation(api.services.create, { date: "2026-10-11", title: "Sunday" });
  const itemId = await admin.mutation(api.services.addSong, { serviceId, songId });
  const get = async () => (await admin.query(api.services.get, { serviceId }))!.items[0];
  return { t, admin, songId, serviceId, itemId, get };
}

test("first user is admin, later users start as singers who can only read", async () => {
  const { t, serviceId } = await setup();
  const singer = t.withIdentity({ tokenIdentifier: "singer", name: "Singer" });
  await singer.mutation(api.users.ensure, {});
  expect((await singer.query(api.users.me, {}))!.role).toBe("singer");
  expect(await singer.query(api.services.get, { serviceId })).not.toBeNull();
  await expect(singer.mutation(api.services.create, { date: "2026-10-18", title: "x" })).rejects.toThrow();
});

test("a service starts from the song's usual order", async () => {
  const { get } = await setup();
  expect((await get()).entries.map((e) => e.label)).toEqual(["Verse 1", "Chorus"]);
});

test("repeat, move and remove address entries by key", async () => {
  const { admin, itemId, get } = await setup();
  const [verse, chorus] = (await get()).entries;
  await admin.mutation(api.services.insertEntry, { itemId, index: 99, sectionId: chorus.sectionId! });
  await admin.mutation(api.services.moveEntry, { itemId, entryKey: verse.key, toIndex: 2 });
  expect((await get()).entries.map((e) => e.label)).toEqual(["Chorus", "Chorus", "Verse 1"]);

  await admin.mutation(api.services.removeEntry, { itemId, entryKey: chorus.key });
  await admin.mutation(api.services.removeEntry, { itemId, entryKey: chorus.key }); // already gone: no-op
  expect((await get()).entries.map((e) => e.label)).toEqual(["Chorus", "Verse 1"]);
});

test("a variation stays in the service until saved to the library", async () => {
  const { admin, songId, itemId, get } = await setup();
  const [verse] = (await get()).entries;
  await admin.mutation(api.services.setEntryText, { itemId, entryKey: verse.key, lyrics: "Amazing GRACE" });

  let entry = (await get()).entries[0];
  expect(entry).toMatchObject({ lyrics: "Amazing GRACE", isVariation: true });
  let song = await admin.query(api.songs.get, { songId });
  expect(song!.sections[0].lyrics).toBe("Amazing grace");

  // Library fixes don't reach a service that has its own variation...
  await admin.mutation(api.songs.updateSection, { sectionId: song!.sections[0]._id, lyrics: "Amazing grace!" });
  expect((await get()).entries[0].lyrics).toBe("Amazing GRACE");

  // ...until it's promoted (or reverted).
  await admin.mutation(api.services.promoteEntry, { itemId, entryKey: verse.key });
  entry = (await get()).entries[0];
  expect(entry).toMatchObject({ lyrics: "Amazing GRACE", isVariation: false });
  song = await admin.query(api.songs.get, { songId });
  expect(song!.sections[0].lyrics).toBe("Amazing GRACE");
});

test("deleting library content leaves services readable", async () => {
  const { admin, songId, get } = await setup();
  const song = await admin.query(api.songs.get, { songId });
  await admin.mutation(api.songs.removeSection, { sectionId: song!.sections[1]._id });
  expect((await get()).entries[1]).toMatchObject({ label: "Chorus", lyrics: "My chains are gone", sectionId: null });

  await admin.mutation(api.songs.remove, { songId });
  const item = await get();
  expect(item.songId).toBeNull();
  expect(item.title).toBe("Amazing Grace");
  expect(item.entries.map((e) => e.lyrics)).toEqual(["Amazing grace", "My chains are gone"]);
});

test("pairing codes are single use", async () => {
  const { admin } = await setup();
  const { code } = await admin.mutation(api.devices.createPairingCode, {});
  const { token } = await admin.mutation(api.devices.claimPairingCode, { code: code.toLowerCase(), deviceName: "Booth" });
  expect(token).toHaveLength(64);
  await expect(admin.mutation(api.devices.claimPairingCode, { code, deviceName: "Again" })).rejects.toThrow();
  expect((await admin.query(api.devices.listMine, {})).map((d) => d.name)).toEqual(["Booth"]);
});
