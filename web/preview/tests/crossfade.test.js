// The map's crossfade (js/viewer/crossfade.js): the layers of a piece share the dither's range exactly, a swap
// during a swap goes on from the blend it has, and a finished layer lets the older ones go.

import test from 'node:test';
import assert from 'node:assert/strict';
import { ALL, CROSSFADE_MS, MAX_LAYERS, intervals, newFade, progress, running, swap } from '../js/viewer/crossfade.js';

const covers = (iv) => {
  assert.equal(iv[0][0], 0);
  for (let i = 1; i < iv.length; i++) assert.equal(iv[i][0], iv[i - 1][1]);
  assert.equal(iv[iv.length - 1][1], ALL);
};

test('a piece at rest holds no fade', () => {
  assert.equal(running(newFade()), false);
});

test('a swap starts at the old mesh whole and ends at the new one whole', () => {
  const f = newFade();
  swap(f, 0, 'a', 'b');
  assert.equal(running(f), true);
  let iv = intervals(f, 0);
  assert.deepEqual(iv, [[0, 0], [0, ALL]]);
  covers(iv);
  iv = intervals(f, CROSSFADE_MS / 2);
  assert.ok(iv[0][1] > 0.4 && iv[0][1] < 0.6, 'half way is about half the pixels');
  covers(iv);
  iv = intervals(f, CROSSFADE_MS);
  assert.deepEqual(iv, [[0, ALL]]);
  assert.equal(running(f), false);
});

test('every pixel goes to exactly one layer, at every moment', () => {
  const f = newFade();
  swap(f, 0, 'a', 'b');
  swap(f, 100, 'b', 'c');
  for (let t = 0; t <= 500; t += 10) {
    const iv = intervals(f, t);
    covers(iv);
    for (let d = 0; d < 1; d += 1 / 64) assert.equal(iv.filter(([lo, hi]) => d >= lo && d < hi).length, 1);
  }
});

test('a swap during a swap goes on from the blend, not from blank', () => {
  const f = newFade();
  swap(f, 0, 'a', 'b');
  const before = intervals(f, 150);
  swap(f, 150, 'b', 'c');
  const after = intervals(f, 150);
  // the new layer has nothing yet; b and a hold what they held
  assert.deepEqual(after[0], [0, 0]);
  assert.deepEqual(after.slice(1), before);
});

test('a layer that finishes takes what is left and the older ones go', () => {
  const f = newFade();
  swap(f, 0, 'a', 'b');
  swap(f, 100, 'b', 'c');
  intervals(f, 100 + CROSSFADE_MS);
  assert.equal(f.layers.length, 1);
  assert.equal(f.layers[0].key, 'c');
});

test('a piece holds a few layers at most', () => {
  const f = newFade();
  swap(f, 0, 'a', 'b');
  for (let i = 0; i < 6; i++) swap(f, 10 * i, 'x', 'y' + i);
  assert.equal(f.layers.length, MAX_LAYERS);
  covers(intervals(f, 60));
});

test('progress is eased and clamped', () => {
  assert.equal(progress({ t0: 0 }, -5), 0);
  assert.equal(progress({ t0: 0 }, 1e6), 1);
  assert.ok(progress({ t0: 0 }, CROSSFADE_MS * 0.25) < 0.25);
});
