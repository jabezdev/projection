import { expect, test } from "bun:test";
import { edits } from "./index";

const service = (): edits.ServiceRecord => ({
  id: "s",
  scheduledAtMs: 1,
  title: "Sunday",
  items: [
    { id: "a", title: "Welcome", content: { kind: "header" } },
    { id: "b", title: "Offering", content: { kind: "header" } },
  ],
});

test("edits made offline land on the server copy and only real disagreements become conflicts", () => {
  const base = service();
  const mine = service();
  mine.title = "Sunday AM";
  mine.items[0].title = "Welcome!";
  mine.items.push({ id: "c", title: "Closing", content: { kind: "header" } });
  const server = service();
  server.items[0].title = "Greeting";
  server.notes = "Bring candles";
  const change: edits.Change = { kind: "service", id: "s", ops: edits.diffService(base, mine) };
  const result = edits.rebase(change, server);
  expect(server.title).toBe("Sunday AM");
  expect(server.notes).toBe("Bring candles");
  expect(server.items.map((i) => i.id)).toEqual(["a", "b", "c"]);
  expect(result.conflicts).toHaveLength(1);
  expect(result.conflicts[0].theirs).toBe("Greeting");
  expect(server.items[0].title).toBe("Greeting");
});

test("a change someone already made online is not a conflict", () => {
  const base = service();
  const mine = service();
  mine.title = "Easter";
  const server = service();
  server.title = "Easter";
  const result = edits.rebase({ kind: "service", id: "s", ops: edits.diffService(base, mine) }, server);
  expect(result.conflicts).toHaveLength(0);
});
