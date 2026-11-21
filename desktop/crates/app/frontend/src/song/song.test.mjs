import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { splitSlides, slides, draftSteps, emptyDraft, moveArrangement, resolveSong, serviceDraft, fromServiceDraft, sectionCode, sectionColor } from './song.ts';

const section = (id, kind, lyrics, number) => ({ id, kind, lyrics, ...(number ? { number } : {}) });
const version = { id: 'v1', number: 1, sections: [section('v', 'verse', 'A\nB\n\nC', 1), section('c', 'chorus', 'D')], arrangement: ['v', 'c', 'v'] };

test('single line breaks stay within a slide; blank lines start the next', () => {
  assert.deepEqual(splitSlides('First line\nSecond line\n\nThird line\nFourth line'), ['First line\nSecond line', 'Third line\nFourth line']);
});
test('arranged repeated parts preserve multiline slides in order', () => {
  assert.deepEqual(draftSteps({ ...emptyDraft(), sections: [section('v', 'verse', 'A\nB\n\nC')], arrangement: ['v', 'v'] }), ['A\nB', 'C', 'A\nB', 'C']);
  assert.deepEqual(slides(version.sections[0]), ['A\nB', 'C']);
});
test('arrangement drops use insertion gaps without losing repeated occurrences', () => {
  assert.deepEqual(moveArrangement(['v', 'c', 'v', 'b'], 0, 4), ['c', 'v', 'b', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v', 'b'], 3, 1), ['v', 'b', 'c', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v'], 1, 2), ['v', 'c', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v'], 1, 0), ['c', 'v', 'v']);
});
test('shared section fixtures match the Rust model', () => {
  const fixtures = JSON.parse(readFileSync(new URL('../../../../../fixtures/sections.json', import.meta.url), 'utf8'));
  for (const c of fixtures.slides) assert.deepEqual(splitSlides(c.lyrics), c.slides, c.name);
  for (const c of fixtures.codes) {
    const s = { id: 'x', lyrics: '', ...c.section };
    assert.deepEqual([sectionCode(s), sectionColor(s)], [c.code, c.color], JSON.stringify(c.section));
  }
});
test('a song item follows the library until the service changes it', () => {
  const item = { songId: 's' };
  assert.deepEqual(resolveSong(item, version).map(s => s.lyrics), ['A\nB\n\nC', 'D', 'A\nB\n\nC']);
  assert.equal(fromServiceDraft(serviceDraft(item, version), version), undefined, 'an untouched draft keeps following the library');
});
test('service edits become variations and service-only parts', () => {
  const draft = serviceDraft({ songId: 's' }, version);
  draft.sections = draft.sections.map(s => s.id === 'c' ? { ...s, lyrics: 'D, softly' } : s);
  draft.sections.push(section('own', 'tag', 'Amen'));
  draft.arrangement = ['v', 'c', 'own'];
  const entries = fromServiceDraft(draft, version);
  const item = { songId: 's', arrangement: entries };
  assert.deepEqual(resolveSong(item, version).map(s => [s.kind, s.lyrics]), [['verse', 'A\nB\n\nC'], ['chorus', 'D, softly'], ['tag', 'Amen']]);
  assert.deepEqual(serviceDraft(item, version).arrangement, ['v', 'c', 'own'], 'reopening shows the same arrangement');
  assert.equal(entries[0].variation, undefined, 'unchanged parts carry no variation');
});
test('the caret picks the slide it is in', async () => {
  const { slideAtCaret } = await import('./song.ts');
  const lyrics = 'a\nb\n\nc\nd\n\ne';
  assert.deepEqual([0, 2, 4, 5, 6, 9, 11, 12].map(caret => slideAtCaret(lyrics, caret)), [0, 0, 0, 1, 1, 1, 2, 2]);
  assert.equal(slideAtCaret('\n\nonly\n\n', 7), 0);
});
