import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { defaultParts, nextOccurrence, sundayTemplate, templateFromService, templateItems } from "./templates";

const fixture = JSON.parse(readFileSync(new URL("../../../desktop/fixtures/sunday-template.json", import.meta.url), "utf8"));

test("the Sunday template matches the desktop's", () => {
  expect(sundayTemplate()).toEqual(fixture);
  expect(defaultParts(sundayTemplate())).toHaveLength(6);
});

test("a service from a template has a section per chosen part followed by its fixed items", () => {
  const t = sundayTemplate();
  t.parts[1].items.push({ id: "fixed", title: "Countdown video", content: { kind: "media", mediaId: "m1" } });
  const items = templateItems(t, ["sunday-countdown", "sunday-closing"]);
  expect(items.map((i) => i.title)).toEqual(["Countdown", "Countdown video", "Closing"]);
  expect(items.every((i) => i.id !== "fixed")).toBe(true);
});

test("a template from a service drops songs unless asked and unpins kept ones", () => {
  const items = [
    { id: "h", title: "Worship", content: { kind: "header" } },
    { id: "s", content: { kind: "song", songId: "a", versionId: "v" } },
    { id: "t", title: "Offering", content: { kind: "text", steps: ["Give"] } },
  ];
  expect(templateFromService("x", "Youth", "Youth", items, false).parts[0].items.map((i) => i.title)).toEqual(["Offering"]);
  expect(templateFromService("x", "Youth", "Youth", items, true).parts[0].items[0].content.versionId).toBeUndefined();
});

test("the next service falls on the template's weekday and time", () => {
  const wednesday = new Date(2026, 10, 25, 12, 0);
  const next = new Date(nextOccurrence({ weekday: 0, startMinutes: 600 }, wednesday));
  expect([next.getDay(), next.getHours(), next.getDate()]).toEqual([0, 10, 29]);
});
