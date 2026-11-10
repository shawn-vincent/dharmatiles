import test from 'node:test';
import assert from 'node:assert/strict';
import { CLASSES, legendRows, segments, sheetGeometry, sheetTokens } from '../js/debug-layer.js';

test('polylines become segment pairs', () => {
  const s = segments([[0, 0, 0, 1, 0, 0, 1, 1, 0], [5, 5, 5]]);
  assert.deepEqual([...s], [0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0]);
});

test('a sheet draws only cells with four corners, outlined where it ends', () => {
  // 3 × 2 nodes, the last column outside the region: one cell drawn
  const g = sheetGeometry({ nx: 3, ny: 2, step: 2, z: [1, 1, null, 1, 1, null] });
  assert.equal(g.positions.length, 2 * 9);
  assert.equal(g.contour.length, 4 * 6, 'a lone cell has four edges');
  assert.ok([...g.positions].every((v) => v === 0 || v === 2 || v === 1));
});

test('the legend lists the classes sent, in order, and a row per banded entry', () => {
  const d = {
    dots: { crowded: [0, 0, 0], grew: [0, 0, 0, 1, 1, 1] },
    trails: { spine: [[0, 0, 0, 1, 1, 1]] },
    sheets: [
      { entry: 1, owner: 'tile/dirt/1', class: 'band-lo', nx: 1, ny: 1, step: 1, z: [0] },
      { entry: 1, owner: 'tile/dirt/1', class: 'band-hi', nx: 1, ny: 1, step: 1, z: [1] },
    ],
  };
  const rows = legendRows(d);
  assert.deepEqual(rows.map((r) => r.label), ['Blade spine', 'Start: grew', 'Start: crowded', 'dirt: band low and high']);
  assert.deepEqual(rows.map((r) => r.n), [1, 2, 1, 1]);
  assert.deepEqual(sheetTokens(d), ['--dbg-band-1', '--dbg-band-1']);
  assert.deepEqual(legendRows(null), []);
});

test('every class the grass sends has a theme token', async () => {
  const { readFile } = await import('node:fs/promises');
  const theme = await readFile(new URL('../../../docs/assets/theme.css', import.meta.url), 'utf8');
  for (const [, token] of CLASSES) assert.ok(theme.includes(`${token}:`), token);
  const rust = await readFile(new URL('../../../crates/dt-elements/src/grass/fronds.rs', import.meta.url), 'utf8');
  const sent = [...rust.matchAll(/Self::\w+ => "([a-z-]+)"/g)].map((m) => m[1]);
  assert.ok(sent.length >= 10);
  for (const c of sent) assert.ok(CLASSES.some((k) => k[0] === c), c);
});
