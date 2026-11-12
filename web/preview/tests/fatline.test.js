import test from 'node:test';
import assert from 'node:assert/strict';

import { fatGeometry } from '../js/fatline.js';

test('each segment is a quad: its two ends twice, pushed to opposite sides', () => {
  const pos = new Float32Array([0, 0, 1, 0, 0, 5, 2, 3, 4, 2, 3, 9]);
  const rgba = new Float32Array([1, 0, 0, 0.2, 1, 0, 0, 0.8, 0, 1, 0, 0.5, 0, 1, 0, 0.6]);
  const g = fatGeometry(pos, rgba);
  assert.equal(g.n, 2);
  assert.equal(g.position.length, 24);
  assert.equal(g.index.length, 12);
  // first segment: A, A, B, B; each vertex knows the other end
  assert.deepEqual([...g.position.slice(0, 12)], [0, 0, 1, 0, 0, 1, 0, 0, 5, 0, 0, 5]);
  assert.deepEqual([...g.other.slice(0, 12)], [0, 0, 5, 0, 0, 5, 0, 0, 1, 0, 0, 1]);
  // the shader measures the direction toward `other`, so the far end's sides are swapped:
  // A+ and B (side -1) lie on the same side of the segment
  assert.deepEqual([...g.side.slice(0, 4)], [1, -1, -1, 1]);
  // opacity at each end: A's for the first two vertices, B's for the last two
  assert.deepEqual([...g.color.slice(0, 16)].filter((_, i) => i % 4 === 3), [0.2, 0.2, 0.8, 0.8].map(Math.fround));
  // the second segment indexes its own four vertices
  assert.deepEqual([...g.index.slice(6)], [4, 5, 6, 5, 7, 6]);
});

test('nothing in, nothing out', () => {
  const g = fatGeometry(new Float32Array(0), new Float32Array(0));
  assert.equal(g.n, 0);
  assert.equal(g.index.length, 0);
});
