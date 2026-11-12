import test from 'node:test';
import assert from 'node:assert/strict';

import { browserHost, isOffline } from '../js/host.js';
import { createStore } from '../js/editor/stores.js';

test('a subscriber that throws is reported and the others still run', () => {
  const reports = [];
  const store = createStore({ a: 0 }, (what, err) => reports.push([what, err]));
  const seen = [];
  store.subscribe(['a'], () => seen.push('first'));
  store.subscribe(['a'], () => { throw new TypeError('page bug'); });
  store.subscribe(['a'], () => seen.push('third'));
  store.set({ a: 1 });
  assert.deepEqual(seen, ['first', 'third']);
  assert.equal(store.get().a, 1);
  assert.equal(reports.length, 1);
  assert.match(reports[0][0], /failed to follow/);
  assert.ok(reports[0][1] instanceof TypeError);
});

test('only a failed fetch is offline: a TypeError from page code is not', async () => {
  assert.equal(isOffline(new TypeError('x is undefined')), false);
  const original = globalThis.fetch;
  try {
    globalThis.fetch = async () => { throw new TypeError('Failed to fetch'); };
    await assert.rejects(browserHost().elements(), (e) => isOffline(e));
    globalThis.fetch = async () => { throw new RangeError('not the network'); };
    await assert.rejects(browserHost().elements(), (e) => !isOffline(e));
  } finally {
    globalThis.fetch = original;
  }
});
