import assert from 'node:assert/strict';
import test from 'node:test';
import { splitSlides, slides, decodeSong, emptySong, songSteps } from './song.ts';

test('single line breaks stay within a slide; blank lines start the next', () => {
  assert.deepEqual(splitSlides('First line\nSecond line\n\nThird line\nFourth line'), ['First line\nSecond line', 'Third line\nFourth line']);
});
test('handles whitespace-only blank lines, CRLF, and extra empty slides', () => {
  assert.deepEqual(splitSlides('\r\n First\r\nSecond\r\n \t\r\n\r\nThird\r\n\r\n'), ['First\nSecond', 'Third']);
  assert.deepEqual(splitSlides('\n \n'), []);
});
test('arranged repeated parts preserve multiline slides in order', () => {
  const part = { id: 'verse', type: 'Verse', number: 1, lyrics: 'A\nB\n\nC' };
  assert.deepEqual(songSteps({ ...emptySong(), parts: [part], arrangement: ['verse', 'verse'] }), ['A\nB', 'C', 'A\nB', 'C']);
});
test('old single-line documents keep their original projection positions', () => {
  const old = { format: 'magi.song.v1', parts: [{ id: 'v', type: 'Verse', number: 1, lyrics: 'A\nB' }], arrangement: ['v'], background: '' };
  const migrated = decodeSong(JSON.stringify(old));
  assert.equal(migrated.slideSeparator, 'blank-line');
  assert.deepEqual(songSteps(migrated), ['A', 'B']);
  assert.equal(migrated.parts[0].lyrics, 'A\n\nB');
});
test('new song documents retain ordinary line breaks when saved and reopened', () => {
  const part = { id: 'v', type: 'Verse', number: 1, lyrics: 'A\nB\n\nC' };
  const song = decodeSong(JSON.stringify({ ...emptySong(), parts: [part], arrangement: ['v'] }));
  assert.deepEqual(slides(song.parts[0]), ['A\nB', 'C']);
});

test('arrangement drops use insertion gaps without losing repeated occurrences', async () => {
  const { moveArrangement } = await import('./song.ts');
  assert.deepEqual(moveArrangement(['v', 'c', 'v', 'b'], 0, 4), ['c', 'v', 'b', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v', 'b'], 3, 1), ['v', 'b', 'c', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v'], 1, 2), ['v', 'c', 'v']);
  assert.deepEqual(moveArrangement(['v', 'c', 'v'], 1, 0), ['c', 'v', 'v']);
});
test('author / singer survives saving and reopening', () => {
  assert.equal(decodeSong(JSON.stringify({ ...emptySong(), authorSinger: 'Example singer' })).authorSinger, 'Example singer');
});

test('shared normalization fixtures match the Rust parser expectations', async () => {
  const { readFileSync } = await import('node:fs');
  const { arrangedParts, partCode, partColor } = await import('./song.ts');
  const fixtures = JSON.parse(readFileSync(new URL('../../../../fixtures/song-normalization.json', import.meta.url), 'utf8'));
  for (const fixture of fixtures.cases) {
    const song = decodeSong(fixture.song);
    assert.deepEqual(songSteps(song), fixture.steps, fixture.name);
    assert.deepEqual(arrangedParts(song).map(part => ({ part: part.id, code: partCode(part), color: partColor(part), slides: slides(part) })), fixture.occurrences, fixture.name);
  }
});
