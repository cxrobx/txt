import { test } from 'node:test';
import assert from 'node:assert/strict';
import { recentRows } from '../src/recents.mjs';

const notes = [{ id: 'a', modified: 3 }, { id: 'b', modified: 9 }, { id: 'c', modified: 5 }, { id: 'd', modified: 1 }];

test('closed notes come first, newest first, without the deleted or the open', () => {
  const closed = [{ id: 'gone', closed_at: 9 }, { id: 'c', closed_at: 8 }, { id: 'a', closed_at: 7 }, { id: 'd', closed_at: 6 }];
  const { closed: shut, recent } = recentRows(notes, closed, new Set(['a']));
  assert.deepEqual(shut.map(r => r.note.id), ['c', 'd']);
  assert.equal(shut[0].closedAt, 8000);
  assert.deepEqual(recent.map(n => n.id), ['b', 'a'], 'the rest by last edit, closed ones not repeated');
});

test('limits', () => {
  const closed = notes.map((n, i) => ({ id: n.id, closed_at: i }));
  const { closed: shut, recent } = recentRows(notes, closed, new Set(), { closedLimit: 2, recentLimit: 1 });
  assert.deepEqual(shut.map(r => r.note.id), ['a', 'b']);
  assert.deepEqual(recent.map(n => n.id), ['c']);
});
