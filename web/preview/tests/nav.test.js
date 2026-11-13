import test from 'node:test';
import assert from 'node:assert/strict';

import {
  atOf, badgeOf, buildHash, caretLineFor, contextFor, crumbsFor, depthOf, levelKey, levelOfHash, levelParams, moveRow, orderStack, ordinal,
  parseHash, placeOf, sameLevel, selOfAt, sizeWords, standing, up,
} from '../js/nav.js';
import { innerRange, simpleWeightings, weightedAs } from '../js/view/property.js';

const tileSel = { kind: 'tile', id: 't1' };
const entry = (id, piece = null) => ({ kind: 'entry', id, piece });

test('a level is the tile, one thing of it, or one control of that', () => {
  assert.equal(depthOf(null), 0);
  assert.equal(depthOf(tileSel), 0);
  assert.equal(depthOf(entry('tile/rock/0')), 1);
  assert.equal(depthOf({ kind: 'region', id: 'bank' }), 1);
  assert.equal(depthOf({ kind: 'line', id: 'shore' }), 1);
  assert.equal(depthOf(entry('tile/rock/0'), { name: 'size', group: 0 }), 2);
  // a control belongs to a thing: on the tile there is none
  assert.equal(depthOf(tileSel, { name: 'size', group: 0 }), 0);
});

test('a level keeps its scroll under a key that ignores the piece', () => {
  assert.equal(levelKey(null), 'tile');
  assert.equal(levelKey(tileSel), 'tile');
  assert.equal(levelKey(entry('tile/rock/0', 2)), 'entry:tile/rock/0');
  assert.equal(levelKey(entry('tile/rock/0'), { name: 'size', group: 1 }), 'entry:tile/rock/0/size@1');
  assert.ok(sameLevel({ sel: entry('a', 1), control: null }, { sel: entry('a', 3), control: null }));
  assert.ok(!sameLevel({ sel: entry('a'), control: null }, { sel: entry('a'), control: { name: 'x' } }));
});

test('up goes from a control to its thing and from a thing to the tile, and stops at the tile', () => {
  const rock = entry('tile/rock/0', 1);
  assert.deepEqual(up({ sel: rock, control: { name: 'size', group: 0 } }, tileSel), { sel: rock, control: null });
  assert.deepEqual(up({ sel: rock, control: null }, tileSel), { sel: tileSel, control: null });
  assert.equal(up({ sel: tileSel, control: null }, tileSel), null);
  assert.equal(up({ sel: null, control: null }, tileSel), null);
});

test('ordinals read as English', () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111].map(ordinal), ['1st', '2nd', '3rd', '4th', '11th', '12th', '13th', '21st', '22nd', '23rd', '101st', '111th']);
  assert.equal(sizeWords('"2x1"'), '2×1');
});

const item = (key, over = {}) => ({ key, label: key, element: 'rock', what: 'rock', file: 'a.dtile', lines: [1, 1], place: [], ...over });

test('the stack is listed as it is written: the base, which the program lists last, goes where its line is', () => {
  const items = [item('dirt', { lines: [12, 12] }), item('grass', { lines: [13, 13] }), item('base', { element: 'connector', what: 'base', lines: [11, 11] })];
  assert.deepEqual(orderStack(items).map((i) => i.key), ['base', 'dirt', 'grass']);
  // a base written last stays last; no base, nothing moves
  const last = [item('dirt', { lines: [12, 12] }), item('base', { element: 'connector', lines: [14, 14] })];
  assert.deepEqual(orderStack(last).map((i) => i.key), ['dirt', 'base']);
  assert.deepEqual(orderStack([item('a'), item('b')]).map((i) => i.key), ['a', 'b']);
  // a part from another file is not ordered by lines of this one
  const other = [item('p', { file: 'parts.dtile', lines: [30, 30] }), item('base', { element: 'connector', lines: [3, 3] })];
  assert.deepEqual(orderStack(other).map((i) => i.key), ['p', 'base']);
});

test('an entry says where it stands and where it is put', () => {
  const items = [item('a'), item('b', { place: [{ name: 'at', text: 'bank' }] })];
  assert.deepEqual(standing(items, 'b'), { n: 2, of: 2 });
  assert.equal(standing(items, 'zzz'), null);
  assert.equal(placeOf(items[1]), 'at=bank');
  assert.equal(placeOf(items[0]), '');
});

test('a badge is the worst of an entry\'s messages', () => {
  assert.equal(badgeOf(undefined), null);
  assert.equal(badgeOf([]), null);
  assert.deepEqual(badgeOf([{ severity: 'note', text: 'steep' }]), { tone: 'note', n: 1, title: 'steep' });
  assert.deepEqual(badgeOf([{ severity: 'note', text: 'steep' }, { severity: 'error', text: 'no room' }]), { tone: 'error', n: 2, title: 'steep\nno room' });
});

test('the arrow keys move between rows without wrapping', () => {
  assert.equal(moveRow(0, 'ArrowDown', 4), 1);
  assert.equal(moveRow(3, 'ArrowDown', 4), 3);
  assert.equal(moveRow(0, 'ArrowUp', 4), 0);
  assert.equal(moveRow(2, 'ArrowUp', 4), 1);
  assert.equal(moveRow(-1, 'ArrowDown', 4), 0);
  assert.equal(moveRow(-1, 'ArrowUp', 4), 3);
  assert.equal(moveRow(2, 'Home', 4), 0);
  assert.equal(moveRow(1, 'End', 4), 3);
  assert.equal(moveRow(1, 'ArrowDown', 0), -1);
});

test('the crumbs run from Places through the folders to the level, which is the current one', () => {
  const place = { root: 0, label: 'My tiles', dir: 'dungeon/combo' };
  const at = (sel, control) => crumbsFor({ place, tileName: 'tree', sel, what: 'trunk', control });
  const names = (l) => l.map((c) => [c.label, c.current]);
  assert.deepEqual(names(at(tileSel)), [['Places', false], ['My tiles', false], ['dungeon', false], ['combo', false], ['tree', true]]);
  assert.deepEqual(names(at(entry('tile/grove/0'))).slice(-2), [['tree', false], ['trunk', true]]);
  assert.deepEqual(at(entry('tile/grove/0'), { name: 'height', group: 0 }).map((c) => c.label), ['Places', 'My tiles', 'dungeon', 'combo', 'tree', 'trunk', 'height']);
  assert.deepEqual(at(entry('tile/grove/0')).map((c) => c.depth), [-1, -1, -1, -1, 0, 1]);
  // a folder crumb says where it goes: Places goes to no place, a folder to its own path
  const l = at(tileSel);
  assert.equal(l[0].place, null);
  assert.deepEqual(l[1].place, { root: 0, dir: '' });
  assert.deepEqual(l[2].place, { root: 0, dir: 'dungeon' });
  assert.deepEqual(l[3].place, { root: 0, dir: 'dungeon/combo' });
});

test('on a folder level the last folder is the current crumb, and Places alone is the top', () => {
  assert.deepEqual(crumbsFor({ place: { root: 0, label: 'My tiles', dir: 'dungeon' } }).map((c) => [c.label, c.current]), [['Places', false], ['My tiles', false], ['dungeon', true]]);
  assert.deepEqual(crumbsFor({ place: { root: 2, label: 'Library', dir: '' } }).map((c) => [c.label, c.current]), [['Places', false], ['Library', true]]);
  assert.deepEqual(crumbsFor({ place: null }).map((c) => [c.label, c.current]), [['Places', true]]);
});

test('the strip says what a level sits in, never the parent\'s own panel', () => {
  const tile = { name: 'combo', size: '"1x1"', seed: '62', system: 'DungeonBlocks', printer: 'default', entries: 5, lines: 2, regions: 1 };
  assert.deepEqual(contextFor({ depth: 0, tile }), ['5 entries · 2 lines · 1 region', 'DungeonBlocks · default']);
  assert.deepEqual(contextFor({ depth: 0, tile: { ...tile, entries: 1, lines: 0, regions: 0 } }), ['1 entry', 'DungeonBlocks · default']);
  assert.deepEqual(contextFor({ depth: 1, tile, entry: { standing: { n: 3, of: 5 }, place: 'at=bank' } }),
    ['combo · 1×1 · seed 62 · DungeonBlocks', '3rd of 5 in the stack · at=bank']);
  assert.deepEqual(contextFor({ depth: 1, tile, entry: { note: 'line shore' } }), ['combo · 1×1 · seed 62 · DungeonBlocks', 'line shore']);
  assert.deepEqual(contextFor({ depth: 2, tile, entry: { standing: { n: 2, of: 5 }, place: '' }, thing: 'grass', control: 'length', value: '3dmm .. 6dmm' }),
    ['grass · 2nd of 5 in the stack', 'length = 3dmm .. 6dmm']);
  assert.deepEqual(contextFor({ depth: 2, tile, entry: null, thing: 'grass', control: 'length' }), ['grass', 'length = not set']);
});

test('a level is an address: at, piece, control, group, face', () => {
  assert.equal(atOf(tileSel), null);
  assert.equal(atOf(entry('tile/rock/2')), 'tile/rock/2');
  assert.equal(atOf({ kind: 'region', id: 'bank' }), 'region:bank');
  assert.deepEqual(selOfAt(null), { kind: 'tile' });
  assert.deepEqual(selOfAt('tile'), { kind: 'tile' });
  assert.deepEqual(selOfAt('tile/rock/2'), { kind: 'entry', id: 'tile/rock/2' });
  assert.deepEqual(selOfAt('line:shore-north'), { kind: 'line', id: 'shore-north' });
  assert.deepEqual(levelParams({ sel: tileSel, control: null, face: 'tile' }), {});
  assert.deepEqual(levelParams({ sel: entry('tile/rock/2', 1), control: { name: 'size', group: 1 }, face: 'tile' }),
    { at: 'tile/rock/2', piece: '1', control: 'size', group: '1' });
  assert.deepEqual(levelParams({ sel: entry('tile/rock/2'), control: { name: 'size', group: 0 }, face: 'source' }), { at: 'tile/rock/2', control: 'size', face: 'source' });
  // a control with no thing is nothing
  assert.deepEqual(levelParams({ sel: tileSel, control: { name: 'size', group: 0 }, face: 'tile' }), {});
});

test('the hash keeps its order, its slashes and what else is in it', () => {
  const h = buildHash({ theme: 'dark', tile: '/a b/c.dtile', at: 'tile/rock/2', control: 'size', place: '/x/y', dir: 'a/b' });
  assert.equal(h, '#tile=%2Fa%20b%2Fc.dtile&place=/x/y&dir=a/b&at=tile/rock/2&control=size&theme=dark');
  assert.deepEqual(parseHash(h), { tile: '/a b/c.dtile', place: '/x/y', dir: 'a/b', at: 'tile/rock/2', control: 'size', theme: 'dark' });
  assert.equal(buildHash({ tile: 'x', at: null }), '#tile=x');
  assert.deepEqual(parseHash('#'), {});
  assert.deepEqual(parseHash('#tile=%E0%A4%A'), { tile: '%E0%A4%A' }); // a broken escape is kept as it is, not thrown
});

test('a level goes into the hash and comes back the same', () => {
  const levels = [
    { sel: tileSel, control: null, face: 'tile' },
    { sel: entry('tile/rock/2'), control: null, face: 'tile' },
    { sel: entry('tile/scatter/0', 3), control: { name: 'size', group: 1 }, face: 'tile' },
    { sel: { kind: 'region', id: 'bank' }, control: null, face: 'source' },
    { sel: { kind: 'line', id: 'shore-north' }, control: null, face: 'tile' },
  ];
  for (const l of levels) {
    const hash = buildHash({ tile: '/x.dtile', ...levelParams(l) });
    const back = levelOfHash(hash);
    assert.equal(levelKey(back.sel, back.control), levelKey(l.sel, l.control), hash);
    assert.equal(back.face, l.face);
    assert.equal(back.sel.piece ?? null, l.sel.piece ?? null);
    assert.equal(back.control?.group ?? null, l.control?.group ?? null);
  }
  // an address with nothing in it is the tile; nonsense for a group is the entry's own
  assert.deepEqual(levelOfHash('#tile=x'), { sel: { kind: 'tile' }, control: null, face: 'tile' });
  assert.deepEqual(levelOfHash('#tile=x&at=tile/a/0&control=size&group=zz').control, { name: 'size', group: 0 });
  assert.equal(levelOfHash('#tile=x&control=size').control, null);
});

test('the source puts its caret on the first line of what the level is, and nowhere for the tile', () => {
  assert.equal(caretLineFor([18, 20]), 18);
  assert.equal(caretLineFor(null), null);
  assert.equal(caretLineFor([0, 0]), null);
});

test('a range inside its weighting comes out, and goes back in', () => {
  const names = ['flat', 'low', 'high', 'bell', 'steps'];
  assert.deepEqual(innerRange('low(3dmm .. 6dmm)', names), { name: 'low', range: '3dmm .. 6dmm' });
  assert.deepEqual(innerRange('bell(3dmm .. 6dmm, 0.3, 0.5)', names), { name: 'bell', range: '3dmm .. 6dmm' });
  assert.deepEqual(innerRange('low(min(1, 2) .. 6dmm, 4)', names), { name: 'low', range: 'min(1, 2) .. 6dmm' });
  assert.deepEqual(innerRange('3dmm .. 6dmm', names), { name: null, range: '3dmm .. 6dmm' });
  assert.deepEqual(innerRange('abs(3)', names), { name: null, range: 'abs(3)' });
  assert.equal(weightedAs('3dmm .. 6dmm', 'low'), 'low(3dmm .. 6dmm)');
  assert.equal(weightedAs('3dmm .. 6dmm', 'flat'), '3dmm .. 6dmm');
});

test('only a weighting that needs nothing but a range is offered as a button', () => {
  const list = [
    { name: 'flat', call: 'flat(range)', args: [], rest: null },
    { name: 'low', call: 'low(range, ratio = 4)', args: [{ name: 'ratio', default: 4 }], rest: null },
    { name: 'steps', call: 'steps(range, weight, weight, …)', args: [], rest: { name: 'weight' } },
    { name: 'mix', call: 'mix(part, weight, part, weight, …)', args: [], rest: { name: 'part' } },
  ];
  assert.deepEqual(simpleWeightings(list).map((w) => w.name), ['flat', 'low']);
});
