import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { defaultParts, insertPart, nextOccurrence, partItems, templateFromService, withoutEmptySections } from './templates.ts';

const sunday = () => JSON.parse(readFileSync(new URL('../../../../../fixtures/sunday-template.json', import.meta.url), 'utf8'));
const titles = items => items.map(i => i.title);

test('planning picks the non-optional parts and adds a section for each', () => {
  const t = sunday();
  assert.equal(defaultParts(t).length, 6);
  assert.deepEqual(titles(partItems(t, ['sunday-welcoming', 'sunday-closing'])), ['Welcoming', 'Closing']);
});

test('a part added later lands after the parts that come before it in the template', () => {
  const t = sunday();
  const items = [...partItems(t, ['sunday-welcoming', 'sunday-message']), { id: 'x', title: 'Sermon notes', content: { kind: 'text', steps: [] } }];
  assert.deepEqual(titles(insertPart(items, t, 'sunday-offering').items), ['Welcoming', 'Tithes and Offering', 'Message', 'Sermon notes']);
  assert.deepEqual(titles(insertPart(items, t, 'sunday-closing').items), ['Welcoming', 'Message', 'Sermon notes', 'Closing']);
});

test('empty sections go and sections with items stay', () => {
  const items = [...partItems(sunday(), ['sunday-welcoming', 'sunday-message']), { id: 'x', title: 'Sermon notes', content: { kind: 'text', steps: [] } }];
  assert.deepEqual(titles(withoutEmptySections(items)), ['Message', 'Sermon notes']);
});

test('saving a service as a template keeps sections and fixed items but not songs', () => {
  const service = { id: 's', title: 'Youth', scheduledAtMs: 0, items: [{ id: 'h', title: 'Worship', content: { kind: 'header' } }, { id: 'song', content: { kind: 'song', songId: 'a' } }, { id: 'v', title: 'Countdown', content: { kind: 'media', mediaId: 'm' } }] };
  const t = templateFromService('t', 'Youth', service, false);
  assert.deepEqual(t.parts.map(p => [p.title, titles(p.items)]), [['Worship', ['Countdown']]]);
});

test('the next service is on the template day and time', () => {
  const next = new Date(nextOccurrence({ weekday: 0, startMinutes: 600 }, new Date(2026, 10, 25, 12)));
  assert.deepEqual([next.getDay(), next.getHours(), next.getDate()], [0, 10, 29]);
});
