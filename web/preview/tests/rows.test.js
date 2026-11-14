import test from 'node:test';
import assert from 'node:assert/strict';

import {
  addPlace, canStep, coverage, entryPlace, groupAt, idAtByte, idFlaw, idOf, landing, linePoints, markId, maskThumb, movedNote, nameOf, orderRefusal, ownNode,
  planMove, refusalLine, seedSwap, slotAt, stepSlot, thumbBox, topNode, writtenIn,
} from '../js/rows.js';

const F = '/tiles/t.dtile';
const it = (key, what, at, extra = {}) => ({ key, what, label: what, element: what, file: F, span: [at, what.length], call: null, ...extra });
// base, dirt, rock, dirt, grass: the base is a line of the stack that is not an entry
const items = [
  it('tile/connector', 'base', 40, { element: 'connector' }),
  it('tile/dirt/0', 'dirt', 60),
  it('tile/rock/0', 'rock', 80),
  it('tile/dirt/1', 'dirt', 100),
  it('tile/grass/0', 'grass', 120),
];
// a call of a definition at byte 200 that makes two entries in another file, and a scatter written inside the dirt at 60..100
const called = (key, what) => it(key, what, 10, { file: '/lib/parts/grass.dtile', call: { file: F, span: [200, 20] } });

test('an entry is a node of this document when it is written here or its call is', () => {
  assert.ok(writtenIn(items[1], F));
  assert.ok(!writtenIn({ ...items[1], file: '/lib/part.dtile' }, F), 'another file is not this document\'s');
  assert.ok(!writtenIn({ ...items[1], span: null }, F));
  assert.equal(ownNode(items[1], F), 60);
  // an entry a call makes is the call, as this document writes it; a call written in another file is not this document's
  const c = called('tile/grass/0', 'grass');
  assert.equal(ownNode(c, F), 200);
  assert.equal(ownNode({ ...c, call: { file: '/lib/x.dtile', span: [1, 2] } }, F), null);
});

test('a scatter written inside a dirt moves with it, and what one call makes is one node', () => {
  const dirt = it('tile/dirt/0', 'dirt', 60, { span: [60, 50] });
  const hoisted = it('tile/scatter/0', 'scatter', 70, { span: [70, 30] });
  const next = it('tile/dirt/1', 'dirt', 130);
  const list = [items[0], dirt, hoisted, next, called('tile/grass/0', 'grass'), called('tile/grass/1', 'grass')];
  assert.equal(topNode(hoisted, list, F), 60, 'the dirt that holds it');
  assert.equal(ownNode(hoisted, F), 70, 'a hide or a delete is of its own node');
  assert.deepEqual(groupAt(list, 2, F), [1, 2]);
  assert.deepEqual(groupAt(list, 4, F), [4, 5]);
  assert.deepEqual(groupAt(list, 3, F), [3, 3]);
  // the group is not moved to a place inside itself
  assert.equal(planMove(list, 1, 2, F), null);
  assert.equal(planMove(list, 2, 3, F), null);
  // the dirt goes below the dirt/1: before the call, the whole call moving as one at its own byte
  assert.deepEqual(planMove(list, 1, 4, F), { at: 60, before: 200 });
  assert.deepEqual(planMove(list, 5, 3, F), { at: 200, before: 130 });
  // one step down from the dirt carries its scatter past the next node
  assert.equal(stepSlot(list, 1, 1, F), 4);
  assert.equal(stepSlot(list, 2, -1, F), 0, 'a step up from a part is a step of the whole');
  assert.equal(stepSlot(list, 4, 1, F), null);
  assert.equal(landing(list, 2, 4, F), 3, 'the scatter ends one below its dirt, which now follows dirt/1');
});

test('a new entry goes above the node at its slot, or at the end', () => {
  assert.deepEqual(addPlace(items, 2, F), { place: 'before', at: 80, index: 1 }, 'the base is not an entry: one entry before the rock');
  assert.deepEqual(addPlace(items, 0, F), { place: 'before', at: 40, index: 0 });
  assert.deepEqual(addPlace(items, 5, F), { place: 'stack', index: 4 });
  // above an entry made elsewhere: the next written one
  const mixed = [items[1], { ...items[2], file: '/lib/p.dtile' }, items[3]];
  assert.deepEqual(addPlace(mixed, 1, F), { place: 'before', at: 100, index: 2 }, 'the entry made elsewhere is before it in the plan');
  assert.equal(entryPlace(items, 5), 4);
  // a slot inside what one node makes is after it, never inside it
  const dirt = it('tile/dirt/0', 'dirt', 60, { span: [60, 50] });
  const hoisted = it('tile/scatter/0', 'scatter', 70, { span: [70, 30] });
  assert.deepEqual(addPlace([items[0], dirt, hoisted, it('tile/dirt/1', 'dirt', 130)], 2, F), { place: 'before', at: 130, index: 2 });
});

test('a move asks the source for the node and the sibling it goes above', () => {
  // the dirt (1) to stand before grass (slot 4): above grass
  assert.deepEqual(planMove(items, 1, 4, F), { at: 60, before: 120 });
  // to the end
  assert.deepEqual(planMove(items, 1, 5, F), { at: 60, before: null });
  // up: the grass above the rock
  assert.deepEqual(planMove(items, 4, 2, F), { at: 120, before: 80 });
  // where it is already (its own slot, or the one after it) nothing moves
  assert.equal(planMove(items, 2, 2, F), null);
  assert.equal(planMove(items, 2, 3, F), null);
  // an entry that is not this document's is not moved; one above it can still be
  const other = items.map((x, i) => (i === 2 ? { ...x, file: '/lib/p.dtile' } : x));
  assert.equal(planMove(other, 2, 0, F), null);
  assert.deepEqual(planMove(other, 1, 3, F), { at: 60, before: 100 }, 'goes above the next entry this document writes');
  // up past nothing this document writes: nowhere to say
  assert.equal(planMove(other, 3, 2, F), null);
});

test('one step up or down is a move to the slot above or two below', () => {
  assert.equal(stepSlot(items, 3, -1, F), 2);
  assert.equal(stepSlot(items, 3, 1, F), 5);
  assert.equal(stepSlot(items, 0, -1, F), null);
  assert.equal(stepSlot(items, 4, 1, F), null);
  assert.ok(canStep(items, 3, -1, F));
  assert.ok(!canStep(items, 0, -1, F), 'nothing above the first');
  assert.ok(!canStep(items, 4, 1, F), 'nothing below the last');
  assert.deepEqual(planMove(items, 3, stepSlot(items, 3, 1, F), F), { at: 100, before: null }, 'one step down from the one above the last is the end');
  assert.deepEqual(planMove(items, 1, stepSlot(items, 1, 1, F), F), { at: 60, before: 100 });
  assert.deepEqual(planMove(items, 2, stepSlot(items, 2, 1, F), F), { at: 80, before: 120 });
});

test('the pointer picks the slot by the rows\' middles', () => {
  const mids = [20, 66, 112];
  assert.equal(slotAt(mids, 0), 0);
  assert.equal(slotAt(mids, 30), 1);
  assert.equal(slotAt(mids, 100), 2);
  assert.equal(slotAt(mids, 500), 3);
  assert.equal(slotAt([], 10), 0);
});

test('a move across a same-named entry with no id swaps the two looks, and says so', () => {
  // dirt 0 below dirt 1 (it steps over the rock and dirt/1)
  assert.equal(seedSwap(items, 1, 4), 'dirt');
  // over a rock and nothing like it: the counts do not change
  assert.equal(seedSwap(items, 1, 3), null);
  // upward across the dirt above
  assert.equal(seedSwap(items, 3, 1), 'dirt');
  // a named entry keeps its look wherever it goes
  const named = items.map((x) => (x.key === 'tile/dirt/1' ? { ...x, key: 'tile/dirt/bank' } : x));
  assert.equal(seedSwap(named, 3, 1), null);
  assert.equal(seedSwap(named, 1, 4), null, 'the other dirt has an id: its count is not the one that moves');
  assert.match(movedNote(items, 1, 4, 'dirt'), /^Moved dirt below grass\. .*swapped looks/);
  assert.equal(movedNote(items, 4, 0, 'grass'), 'Moved grass to the bottom');
  assert.equal(movedNote(items, 2, 5, 'rock'), 'Moved rock to the top');
  assert.equal(movedNote(items, 1, 3, 'dirt'), 'Moved dirt below dirt');
});

test('the plan\'s refusal of an order is found among the errors and said in its own words', () => {
  const early = { code: 'dt::order', message: 'this stands on the ground, and the ground is painted after it in the stack', help: 'list a `ground` or `dirt` before it; `dirt` (line 6) comes after it.' };
  assert.equal(orderRefusal([{ code: 'dt::look', message: 'x' }, early]), early);
  assert.equal(orderRefusal([{ code: 'dt::plan', message: 'reads every rock' }]).message, 'reads every rock');
  assert.equal(orderRefusal([{ code: 'dt::look' }]), null);
  assert.equal(orderRefusal(null), null);
  assert.equal(
    refusalLine(early),
    'This stands on the ground, and the ground is painted after it in the stack. List a `ground` or `dirt` before it; `dirt` (line 6) comes after it',
  );
});

test('an id is what the key says, and a rename must be a plain name no sibling has', () => {
  assert.equal(idOf('tile/rock/oak'), 'oak');
  assert.equal(idOf('tile/rock/0'), null);
  assert.equal(idOf('tile/connector'), null);
  assert.equal(idOf('tile/oak/a/b'), null, 'one made by a call is named by the call');
  assert.equal(nameOf({ key: 'tile/dirt/bank', label: 'dirt in bank' }), 'bank');
  assert.equal(nameOf({ key: 'tile/dirt/0', label: 'dirt in bank' }), 'dirt in bank');
  const bank = { ...items[1], key: 'tile/dirt/bank' };
  const list = [bank, items[3]];
  assert.equal(idFlaw('lawn', list, items[3]), null);
  assert.equal(idFlaw('', list, bank), null, 'empty takes the id out');
  assert.equal(idFlaw('bank', list, items[3]), 'Another dirt is already called bank');
  assert.equal(idFlaw('bank', list, bank), null, 'its own name is not a clash');
  assert.match(idFlaw('12', list, bank), /digits/);
  assert.match(idFlaw('a/b', list, bank), /letters/);
  assert.match(idFlaw('old oak', list, bank), /letters/);
  // the same name on another element is no clash: the id names it among same-named siblings
  assert.equal(idFlaw('bank', [bank, items[2]], items[2]), null);
});

// ---- the pictures ---------------------------------------------------------------------

test('a region\'s mask is boxed down with the top row first and coverage as the value', () => {
  // 4 by 4, the south (row 0) fully set and the rest clear: the picture's bottom row is the only filled one
  const bits = '1111' + '0000' + '0000' + '0000';
  const t = maskThumb({ nx: 4, ny: 4, bits }, 2);
  assert.equal(t.w, 2);
  assert.equal(t.h, 2);
  assert.equal(t.a[2], 128, 'half of the bottom block is the south row');
  assert.equal(t.a[3], 128);
  assert.equal(t.a[0], 0);
  // smaller than the box: kept as it is
  const s = maskThumb({ nx: 2, ny: 1, bits: '10' }, 64);
  assert.deepEqual([s.w, s.h, [...s.a]], [2, 1, [255, 0]]);
  // a tall mask keeps its shape
  const tall = maskThumb({ nx: 2, ny: 8, bits: '1'.repeat(16) }, 4);
  assert.deepEqual([tall.w, tall.h], [1, 4]);
  assert.ok([...tall.a].every((v) => v === 255));
});

test('how much of the tile a region covers, and the line\'s points as pixels', () => {
  assert.equal(coverage({ bits: '1100' }), 0.5);
  assert.equal(coverage({ bits: '' }), 0);
  // a line across the middle of a 1x1 tile, in a 20 x 20 box with 2 px kept clear: y turned over
  assert.deepEqual(linePoints([[0, 0.5], [1, 0.5]], 1, 1, 20, 20), [[2, 10], [18, 10]]);
  assert.deepEqual(linePoints([[0, 0], [1, 1]], 1, 1, 20, 20), [[2, 18], [18, 2]]);
  // a 2x1 tile keeps its own shape in the box
  assert.deepEqual(linePoints([[2, 1]], 2, 1, 36, 18), [[34, 2]]);
  assert.deepEqual(linePoints(null, 1, 1, 20, 20), []);
  assert.deepEqual(thumbBox(1, 1), [36, 36]);
  assert.deepEqual(thumbBox(2, 1), [36, 18]);
  assert.deepEqual(thumbBox(1, 3), [12, 36]);
});

test('an edit names a node by the id the check listed: the node at a byte, else the innermost that holds it', () => {
  const nodes = [
    { id: 'tile', what: 'tile', name: null, at: 0, len: 100 },
    { id: 'tile/line shore', what: 'line', name: 'shore', at: 20, len: 30 },
    { id: 'tile/line shore/through', what: 'through', name: null, at: 35, len: 10 },
    { id: 'tile/stack', what: 'stack', name: null, at: 60, len: 30 },
    { id: 'tile/stack/rock#2', what: 'rock', name: null, at: 70, len: 8 },
  ];
  assert.equal(idAtByte(nodes, 70), 'tile/stack/rock#2');
  assert.equal(idAtByte(nodes, 36), 'tile/line shore/through', 'inside a node: the innermost');
  assert.equal(idAtByte(nodes, 25), 'tile/line shore');
  assert.equal(idAtByte(nodes, 500), null, 'no node there');
  assert.equal(idAtByte(null, 70), null, 'nothing checked yet');
  assert.equal(idAtByte(nodes, null), null);
  assert.equal(markId(nodes, 'line', 'shore'), 'tile/line shore');
  assert.equal(markId(nodes, 'region', 'shore'), null);
});
