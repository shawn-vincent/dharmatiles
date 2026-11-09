import test from 'node:test';
import assert from 'node:assert/strict';
import { regionTriangles } from '../js/overlay.js';

const map = { cols: 2, rows: 1, cells: 2 };

test('a region is flat at zero, one quad per run of nodes, in mm', () => {
  // nodes 0..4 by 0..2; the middle row has nodes 1 and 2 set
  const r = { nx: 5, ny: 3, bits: '00000' + '01100' + '00000' };
  const t = regionTriangles(r, map, 35);
  assert.equal(t.length, 18);
  for (let i = 2; i < t.length; i += 3) assert.equal(t[i], 0);
  const xs = [...t].filter((_, i) => i % 3 === 0), ys = [...t].filter((_, i) => i % 3 === 1);
  assert.equal(Math.min(...xs), 8.75);
  assert.equal(Math.max(...xs), 43.75);
  assert.equal(Math.min(...ys), 8.75);
  assert.equal(Math.max(...ys), 26.25);
});

test('cells are clipped to the tile', () => {
  const r = { nx: 5, ny: 3, bits: '11111'.repeat(3) };
  const t = regionTriangles(r, map, 35);
  const xs = [...t].filter((_, i) => i % 3 === 0), ys = [...t].filter((_, i) => i % 3 === 1);
  assert.equal(Math.min(...xs), 0);
  assert.equal(Math.max(...xs), 70);
  assert.equal(Math.max(...ys), 35);
});

test('an empty region has no triangles', () => {
  assert.equal(regionTriangles({ nx: 3, ny: 3, bits: '0'.repeat(9) }, map, 35).length, 0);
});
