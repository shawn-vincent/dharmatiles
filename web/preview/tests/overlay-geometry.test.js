// @ts-check
// The overlay's geometry (js/overlay-geometry.js): a region's cells, a line's ribbon, a point's disc, and where a label hangs.  Every map and region
// it draws is a whole one, the fixture's (map-fixture.js) with its own size, lattice and cells.
import test from 'node:test';
import assert from 'node:assert/strict';
import { discTriangles, outlineTriangles, refAnchor, refItems, regionAnchor, regionTriangles, ribbonTriangles } from '../js/overlay.js';
import { MAP, mapOf, regionOf } from './map-fixture.js';

const map = mapOf(2, 1, 2);

test('a region is flat at zero, one quad per run of nodes, in mm', () => {
  // nodes 0..4 by 0..2; the middle row has nodes 1 and 2 set
  const r = regionOf(5, 3, '00000' + '01100' + '00000');
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
  const r = regionOf(5, 3, '11111'.repeat(3));
  const t = regionTriangles(r, map, 35);
  const xs = [...t].filter((_, i) => i % 3 === 0), ys = [...t].filter((_, i) => i % 3 === 1);
  assert.equal(Math.min(...xs), 0);
  assert.equal(Math.max(...xs), 70);
  assert.equal(Math.max(...ys), 35);
});

test('an empty region has no triangles', () => {
  assert.equal(regionTriangles(regionOf(3, 3, '0'.repeat(9)), map, 35).length, 0);
});


test('a line is a flat strip and a curtain, in mm, centred on level zero', () => {
  const t = ribbonTriangles([[0, 0], [1, 0]], 35, 1);
  assert.equal(t.length, 36); // two quads of two triangles
  const zs = [...t].filter((_, i) => i % 3 === 2);
  assert.equal(Math.min(...zs), -1);
  assert.equal(Math.max(...zs), 1);
  const xs = [...t].filter((_, i) => i % 3 === 0);
  assert.equal(Math.min(...xs), -1); // butt ends meet their neighbours
  assert.equal(Math.max(...xs), 36);
  assert.equal(ribbonTriangles([[0, 0]], 35).length, 0);
});

test('a region outline runs round its set cells and not along the tile border', () => {
  const map2 = mapOf(1, 1, 4);
  // a 2x2 block of nodes in the middle of a 5x5 lattice: four sides of two cells each
  const bits = Array.from({ length: 25 }, (_, n) => ([1, 2].includes(n % 5) && [1, 2].includes(Math.floor(n / 5)) ? '1' : '0')).join('');
  const t = outlineTriangles(regionOf(5, 5, bits), map2, 35, 0.5);
  assert.equal(t.length / 36, 8);
  const full = outlineTriangles(regionOf(5, 5, '1'.repeat(25)), map2, 35, 0.5);
  assert.equal(full.length, 0);
});

test('a point is a flat disc', () => {
  const t = discTriangles([1, 1], 35, 2, 8);
  assert.equal(t.length, 8 * 9);
  for (let i = 2; i < t.length; i += 3) assert.equal(t[i], 0);
});

test('labels hang on the middle of a line, a point, and the middle of a region', () => {
  const map2 = mapOf(2, 2, 2, [regionOf(5, 5, '0'.repeat(10) + '00100' + '0'.repeat(10), 'crag')]);
  assert.deepEqual(refAnchor({ kind: 'edge', name: 'westEdge', pts: [[0, 0], [0, 2]] }, map2), [0, 1]);
  assert.deepEqual(refAnchor({ kind: 'point', name: 'knoll', at: [0.5, 0.5] }, map2), [0.5, 0.5]);
  // a loop is named on its first side
  assert.deepEqual(refAnchor({ kind: 'edge', name: 'edges', pts: [[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]] }, map2), [1, 0]);
  assert.deepEqual(refAnchor({ kind: 'region', name: 'crag' }, map2), [1, 1]);
  assert.equal(regionAnchor(regionOf(2, 2, '0000'), map2), null);
});

test('what a region is made from is drawn: lines as ribbons, a region faint inside its outline', () => {
  const map2 = mapOf(2, 2, 2, [regionOf(5, 5, '0'.repeat(10) + '00100' + '0'.repeat(10), 'crag')]);
  /** @type {import('../js/api').PlaceRef[]} */
  const refs = [{ kind: 'region', name: 'crag' }, { kind: 'edge', name: 'westEdge', pts: [[0, 0], [0, 2]] }, { kind: 'point', name: 'knoll', at: [1, 1] }, { kind: 'unknown', name: 'x' }];
  const apron = { ...MAP.regions[0], refs };
  const items = refItems(map2, apron, 35, '#abc');
  // the point is a marker with its label, not a disc here
  assert.deepEqual(items.map((i) => i.role), ['fill', 'line', 'line']);
  assert.ok(items.every((i) => i.color === '#abc'));
  assert.ok(items[0].dz < items[1].dz, 'the fill lies under its outline');
});

test('a flat band has no curtain', () => {
  assert.equal(ribbonTriangles([[0, 0], [1, 0]], 35, 1, false, false).length, 18);
});

