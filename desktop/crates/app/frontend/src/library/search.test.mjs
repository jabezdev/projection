import assert from 'node:assert/strict';
import { test } from 'node:test';
import { searchLibrary } from './search.ts';

const songs = [{ id: 1, title: 'Amazing Grace' }, { id: 2, title: 'Grace Alone' }, { id: 3, title: 'Tribes' }];
const media = [{ id: 9, title: 'Welcome loop', kind: 'video', path: 'C:\\Media\\grace-intro.mp4' }];

test('every word must match, titles starting with the query rank first', () => {
  const titles = searchLibrary('grace', songs, media).map(r => r.kind === 'song' ? r.song.title : r.media.title);
  assert.deepEqual(titles, ['Grace Alone', 'Amazing Grace', 'Welcome loop']);
  assert.deepEqual(searchLibrary('amazing grace', songs, media).map(r => r.song?.id), [1]);
});

test('media matches by file name, and an empty query lists everything', () => {
  assert.equal(searchLibrary('intro', songs, media)[0].media.id, 9);
  assert.equal(searchLibrary('', songs, media).length, 4);
  assert.equal(searchLibrary('nothing here', songs, media).length, 0);
});
