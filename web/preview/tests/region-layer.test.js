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

import { discTriangles, outlineTriangles, refAnchor, refItems, regionAnchor, ribbonTriangles } from '../js/overlay.js';
import { clauseGroups, clauseMenu, clauseTitle, combineNote, describeDistance, placeChoices, regionKind } from '../js/editor/region-sheet.js';

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
  const map2 = { cols: 1, rows: 1, cells: 4 };
  // a 2x2 block of nodes in the middle of a 5x5 lattice: four sides of two cells each
  const bits = Array.from({ length: 25 }, (_, n) => ([1, 2].includes(n % 5) && [1, 2].includes(Math.floor(n / 5)) ? '1' : '0')).join('');
  const t = outlineTriangles({ nx: 5, ny: 5, bits }, map2, 35, 0.5);
  assert.equal(t.length / 36, 8);
  const full = outlineTriangles({ nx: 5, ny: 5, bits: '1'.repeat(25) }, map2, 35, 0.5);
  assert.equal(full.length, 0);
});

test('a point is a flat disc', () => {
  const t = discTriangles([1, 1], 35, 2, 8);
  assert.equal(t.length, 8 * 9);
  for (let i = 2; i < t.length; i += 3) assert.equal(t[i], 0);
});

test('labels hang on the middle of a line, a point, and the middle of a region', () => {
  const map2 = { cols: 2, rows: 2, cells: 2, regions: [{ name: 'crag', nx: 5, ny: 5, bits: '0'.repeat(10) + '00100' + '0'.repeat(10) }] };
  assert.deepEqual(refAnchor({ kind: 'edge', name: 'westEdge', pts: [[0, 0], [0, 2]] }, map2), [0, 1]);
  assert.deepEqual(refAnchor({ kind: 'point', at: [0.5, 0.5] }, map2), [0.5, 0.5]);
  // a loop is named on its first side
  assert.deepEqual(refAnchor({ kind: 'edge', name: 'edges', pts: [[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]] }, map2), [1, 0]);
  assert.deepEqual(refAnchor({ kind: 'region', name: 'crag' }, map2), [1, 1]);
  assert.equal(regionAnchor({ nx: 2, ny: 2, bits: '0000' }, map2), null);
});

test('what a region is made from is drawn: lines as ribbons, a region faint inside its outline', () => {
  const map2 = { cols: 2, rows: 2, cells: 2, regions: [{ name: 'crag', nx: 5, ny: 5, bits: '0'.repeat(10) + '00100' + '0'.repeat(10) }] };
  const apron = { refs: [{ kind: 'region', name: 'crag' }, { kind: 'edge', name: 'westEdge', pts: [[0, 0], [0, 2]] }, { kind: 'point', name: 'knoll', at: [1, 1] }, { kind: 'unknown', name: 'x' }] };
  const items = refItems(map2, apron, 35, '#abc');
  // the point is a marker with its label, not a disc here
  assert.deepEqual(items.map((i) => i.role), ['fill', 'line', 'line']);
  assert.ok(items.every((i) => i.color === '#abc'));
  assert.ok(items[0].dz < items[1].dz, 'the fill lies under its outline');
});

test('a distance reads as the source says it', () => {
  assert.equal(describeDistance({ text: '"[8 .. 14]"' }, 11, [8, 14]), '8–14 mm');
  assert.equal(describeDistance({ text: '6.5' }, 6.5, null), '6.5 mm');
  assert.equal(describeDistance({ text: '"shoreMm * 2"' }, 7, null), 'shoreMm * 2 (7 mm)');
});

const places = [{ name: 'northEdge', kind: 'edge' }, { name: 'shore', kind: 'line' }, { name: 'crag', kind: 'region' }, { name: 'apron', kind: 'region' }, { name: 'center', kind: 'point' }];
const crag = {
  clauses: [
    { kind: 'within', at: 10, distance: { text: '"[8 .. 14]"', span: [20, 10] }, of: { text: 'westEdge', written: 'westEdge', span: [34, 8] }, mm: 11, range: [8, 14] },
    { kind: 'except', at: 50, regions: { text: 'pool', span: [57, 4] } },
  ],
};

test('a region of places shows each clause and how they combine', () => {
  assert.equal(regionKind(crag), 'places');
  assert.equal(regionKind({ boundedBy: { text: 'shore' }, fill: { text: '"1,1"' }, clauses: [] }), 'bucket');
  assert.equal(regionKind({ boundedBy: { text: 'shore' }, clauses: crag.clauses }), 'mixed');
  assert.equal(clauseTitle(crag.clauses[0]), 'within 8–14 mm of westEdge');
  assert.equal(clauseTitle(crag.clauses[1]), 'except pool');
  assert.match(combineNote(crag), /added together.*then the except regions taken away/);
  assert.match(combineNote({ clauses: [crag.clauses[1]] }), /^The whole tile, less the regions it excepts/);
  assert.match(combineNote({ boundedBy: { text: 'a' }, clauses: [] }), /enclose/);
  const asW = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: c.span });
  const row = (node, name) => ({ name, unit: node === 'within' && name === 'distance' ? 'dmm' : 'word', required: true });
  const groups = clauseGroups({ name: 'crag' }, crag, { places, regions: [{ name: 'crag' }, { name: 'pool' }] }, asW, row);
  assert.equal(groups.length, 2);
  assert.deepEqual(groups[0].controls.map((c) => c.name), ['distance', 'of']);
  assert.deepEqual(groups[0].written.map((w) => w.text), ['distance="[8 .. 14]"', 'of=westEdge']);
  // the place is a choice among the tile's places, not the region itself, and keeps what is written
  assert.deepEqual(groups[0].controls[1].words.map((w) => w.word), ['westEdge', 'northEdge', 'shore', 'apron', 'center']);
  assert.deepEqual(groups[1].controls[0].words.map((w) => w.word), ['pool']);
});

test('place choices are ordered edges, lines, regions, points', () => {
  assert.deepEqual(placeChoices(places, 'x', '"(0.3, 0.7)"').map((w) => w.word), ['"(0.3, 0.7)"', 'northEdge', 'shore', 'crag', 'apron', 'center']);
});

test('a flat band has no curtain', () => {
  assert.equal(ribbonTriangles([[0, 0], [1, 0]], 35, 1, false, false).length, 18);
});

test('the add-clause menu offers within, except and boundedBy, and says why one cannot be written', () => {
  const map2 = { lines: [{ name: 'shore' }], regions: [{ name: 'pool' }, { name: 'crag' }] };
  const bucket = { boundedBy: { span: [1, 2] }, fill: { span: [3, 4] }, clauses: [] };
  const menu = clauseMenu({ name: 'pool' }, bucket, map2);
  assert.deepEqual(menu.map((c) => c.kind), ['within', 'except', 'boundedBy']);
  assert.equal(menu[0].node, 'within 6 of=center');
  assert.equal(menu[1].node, 'except crag'); // another region, never itself
  assert.equal(menu[2].disabled, true); // its boundedBy is written already
  const places = { clauses: [{ kind: 'within' }] };
  const m2 = clauseMenu({ name: 'pool' }, places, map2);
  assert.equal(m2[2].node, 'boundedBy shore; fill center'); // a bucket needs its fill point
  assert.equal(clauseMenu({ name: 'pool' }, { fill: { span: [1, 1] } }, map2)[2].node, 'boundedBy shore');
  const alone = clauseMenu({ name: 'pool' }, places, { lines: [], regions: [{ name: 'pool' }] });
  assert.equal(alone[1].disabled, true);
  assert.equal(alone[2].disabled, true);
  assert.match(alone[1].why, /no other region/);
});
