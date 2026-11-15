import test from 'node:test';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

import { clauseGroups, clauseMenu, clauseTitle, combineNote, describeDistance, placeChoices, regionKind } from '../js/editor/region-sheet.js';
import { MAP, regionOf } from './map-fixture.js';


test('a distance reads as the source says it, with the program\'s printed size only where it differs', () => {
  // DungeonBlocks: printed as written, the program sends none, and the page adds no unit word
  assert.equal(describeDistance({ text: '"[8dmm .. 14dmm]"' }, null), '[8dmm .. 14dmm]');
  assert.equal(describeDistance({ text: '"7dmm"' }, null), '7dmm');
  assert.equal(describeDistance({ text: '6.5' }, null), '6.5', 'a bare number gets no mm from the page');
  // OpenLOCK: the program's answer is shown beside what is written
  assert.equal(describeDistance({ text: '"7dmm"' }, ['5.08']), '7dmm (prints 5.08 mm)');
  assert.equal(describeDistance({ text: '"[8dmm .. 14dmm]"' }, ['5.81', '10.16']), '[8dmm .. 14dmm] (prints 5.81–10.16 mm)');
});

test('the clause line says no unit of its own: nothing in region-sheet.js writes mm or works out a size', () => {
  const src = readFileSync(new URL('../js/editor/region-sheet.js', import.meta.url), 'utf8')
    .split('\n').filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l)).join('\n');
  // the one `mm` the page writes follows the program's printed answer, in "(prints 5.08 mm)"
  assert.equal((src.match(/\bmm\b/g) || []).length, 1);
  assert.match(src, /\(prints \$\{printed\.join\('–'\)\} mm\)/);
  assert.doesNotMatch(src, /\.mm\b|\.range\b|toFixed/, 'no number of mm is worked out here');
});

const places = [{ name: 'northEdge', kind: 'edge' }, { name: 'shore', kind: 'line' }, { name: 'crag', kind: 'region' }, { name: 'apron', kind: 'region' }, { name: 'center', kind: 'point' }];
const crag = {
  clauses: [
    { kind: 'within', at: 10, distance: { text: '"[8dmm .. 14dmm]"', span: [20, 10] }, of: { text: 'westEdge', written: 'westEdge', span: [34, 8] }, printed: null },
    { kind: 'except', at: 50, regions: { text: 'pool', span: [57, 4] } },
  ],
};

test('a region of places shows each clause and how they combine', () => {
  assert.equal(regionKind(crag), 'places');
  assert.equal(regionKind({ boundedBy: { text: 'shore' }, fill: { text: '"1,1"' }, clauses: [] }), 'bucket');
  assert.equal(regionKind({ boundedBy: { text: 'shore' }, clauses: crag.clauses }), 'mixed');
  assert.equal(clauseTitle(crag.clauses[0]), 'within [8dmm .. 14dmm] of westEdge');
  assert.equal(clauseTitle(crag.clauses[1]), 'except pool');
  assert.match(combineNote(crag), /added together.*then the except regions taken away/);
  assert.match(combineNote({ clauses: [crag.clauses[1]] }), /^The whole tile, less the regions it excepts/);
  assert.match(combineNote({ boundedBy: { text: 'a' }, clauses: [] }), /enclose/);
  const asW = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: c.span });
  const row = (node, name) => ({ name, unit: node === 'within' && name === 'distance' ? 'dmm' : 'word', required: true });
  const groups = clauseGroups({ name: 'crag' }, crag, { places, regions: [{ name: 'crag' }, { name: 'pool' }] }, asW, row);
  assert.equal(groups.length, 2);
  assert.deepEqual(groups[0].controls.map((c) => c.name), ['distance', 'of']);
  assert.deepEqual(groups[0].written.map((w) => w.text), ['distance="[8dmm .. 14dmm]"', 'of=westEdge']);
  // the place is a choice among the tile's places, not the region itself, and keeps what is written
  assert.deepEqual(groups[0].controls[1].words.map((w) => w.word), ['westEdge', 'northEdge', 'shore', 'apron', 'center']);
  assert.deepEqual(groups[1].controls[0].words.map((w) => w.word), ['pool']);
});

test('place choices are ordered edges, lines, regions, points', () => {
  assert.deepEqual(placeChoices(places, 'x', '"(0.3, 0.7)"').map((w) => w.word), ['"(0.3, 0.7)"', 'northEdge', 'shore', 'crag', 'apron', 'center']);
});

test('the add-clause menu offers within, except and boundedBy, and says why one cannot be written', () => {
  const map2 = { ...MAP, lines: [MAP.lines[0]], regions: [regionOf(5, 5, '0'.repeat(25), 'pool'), regionOf(5, 5, '0'.repeat(25), 'crag')] };
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
  const alone = clauseMenu({ name: 'pool' }, places, { ...MAP, lines: [], regions: [regionOf(5, 5, '0'.repeat(25), 'pool')] });
  assert.equal(alone[1].disabled, true);
  assert.equal(alone[2].disabled, true);
  assert.match(alone[1].why, /no other region/);
});
