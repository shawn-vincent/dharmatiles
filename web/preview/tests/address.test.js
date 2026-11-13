// The path and its address: the crumbs and the URL are two drawings of one path (js/nav.js, editor/address.js).
import test from 'node:test';
import assert from 'node:assert/strict';
import { OWNED, addressLevel, buildHash, crumbsFor, levelOfHash, levelParams, stepsHistory } from '../js/nav.js';

const tileSel = { kind: 'tile' };

test('a map\'s levels are the tile\'s trail: the document, its pieces, a property of them', () => {
  const place = { root: 0, label: 'My tiles', dir: '' };
  const at = (depth, what = '', control = '') => crumbsFor({ place, tileName: 'first-plate.dmap', depth, what, control }).map((c) => [c.label, c.depth, c.current]);
  assert.deepEqual(at(0), [['Places', -1, false], ['My tiles', -1, false], ['first-plate.dmap', 0, true]]);
  assert.deepEqual(at(1, '8 pieces').slice(-2), [['first-plate.dmap', 0, false], ['8 pieces', 1, true]]);
  assert.deepEqual(at(2, 'water · p2', 'Seed').slice(-2), [['water · p2', 1, false], ['Seed', 2, true]]);
});

test('the address of a map\'s levels: the pieces, then the property, and back again', () => {
  assert.deepEqual(levelParams({ sel: tileSel, control: null, face: 'tile', pieces: ['p2', 'p3'] }), { pieces: 'p2,p3' });
  assert.deepEqual(levelParams({ sel: tileSel, control: null, face: 'tile', pieces: ['p2'], prop: 'seed' }), { pieces: 'p2', prop: 'seed' });
  assert.deepEqual(levelParams({ sel: tileSel, control: null, face: 'tile', pieces: [], prop: 'seed' }), {}, 'a property needs its pieces');
  const h = buildHash({ tile: '/x.dmap', ...levelParams({ sel: tileSel, control: null, face: 'tile', pieces: ['p2'], prop: 'seed' }) });
  assert.equal(h, '#tile=%2Fx.dmap&pieces=p2&prop=seed');
  const back = levelOfHash(h);
  assert.deepEqual([back.pieces, back.prop], [['p2'], 'seed']);
  assert.equal(levelOfHash('#tile=x&pieces=p2&prop=nope').prop, undefined);
  assert.equal(levelOfHash('#tile=x').pieces, undefined);
});

test('one rule says when the address takes a step of history', () => {
  const tile = (extra = {}) => ({ tile: '/a.dtile', ...extra });
  // folders: another level is a step, the same level is not, nor is the first address over a blank one
  assert.equal(stepsHistory({ place: 'home' }, { place: 'home', dir: 'a' }), true);
  assert.equal(stepsHistory({ place: 'home', dir: 'a' }, { place: 'home', dir: 'a' }), false);
  assert.equal(stepsHistory(tile(), { place: 'home' }), true);
  assert.equal(stepsHistory({}, { place: '' }), false);
  // from a folder into a document is a step (Back goes to the folder)
  assert.equal(stepsHistory({ place: 'home' }, tile()), true);
  // a document's new level is a step; the same level, the source, another piece of the entry, another document replace
  assert.equal(stepsHistory(tile(), tile({ at: 'tile/rock/0' })), true);
  assert.equal(stepsHistory(tile({ at: 'tile/rock/0' }), tile({ at: 'tile/rock/0', control: 'size' })), true);
  assert.equal(stepsHistory(tile({ at: 'tile/rock/0' }), tile({ at: 'tile/rock/0', piece: '2' })), false);
  assert.equal(stepsHistory(tile({ at: 'tile/rock/0' }), tile({ at: 'tile/rock/0', face: 'source' })), false);
  assert.equal(stepsHistory(tile(), tile({ at: 'tile/rock/0', face: 'source' })), false);
  assert.equal(stepsHistory(tile(), { tile: '/b.dtile', at: 'tile/rock/0' }), false);
  // a map: its pieces are a level, another selection of them is not, the property is
  assert.equal(stepsHistory(tile(), tile({ pieces: 'p1' })), true);
  assert.equal(stepsHistory(tile({ pieces: 'p1' }), tile({ pieces: 'p2,p3' })), false);
  assert.equal(stepsHistory(tile({ pieces: 'p1' }), tile({ pieces: 'p1', prop: 'seed' })), true);
  assert.equal(addressLevel(tile({ pieces: 'p1', prop: 'seed' })), 'pieces/seed');
});

test("a folder of the Add step is a level of the map's trail with its own address", () => {
  const add = (place, dir = '') => levelParams({ sel: tileSel, control: null, face: 'tile', pieces: [], add: { place, ...(dir && { dir }) } });
  assert.deepEqual(add(''), { add: '' }, 'Places of the Add step');
  assert.deepEqual(add('/Users/x/tiles'), { add: '/Users/x/tiles' });
  assert.deepEqual(add('/Users/x/tiles', 'dungeon/rooms'), { add: '/Users/x/tiles', adir: 'dungeon/rooms' });
  assert.deepEqual(levelParams({ sel: tileSel, control: null, face: 'tile', pieces: ['p2'], add: { place: '' } }), { pieces: 'p2' }, "a piece selected is the piece's level");
  const h = buildHash({ tile: '/x.dmap', ...add('/Users/x/tiles', 'dungeon') });
  assert.equal(h, '#tile=%2Fx.dmap&add=/Users/x/tiles&adir=dungeon');
  assert.deepEqual(levelOfHash(h).add, { place: '/Users/x/tiles', dir: 'dungeon' });
  assert.deepEqual(levelOfHash('#tile=x&add=').add, { place: '', dir: '' }, 'Places has an empty key');
  assert.equal(levelOfHash('#tile=x&pieces=p2&add=').add, undefined);
  assert.equal(levelOfHash('#tile=x').add, undefined);
  for (const k of ['add', 'adir']) assert.ok(OWNED.includes(k), `${k} is the address's`);
  // each folder is its own level of the address, so a step into one is a step of the history, and the same folder again is not
  const a = (place, dir) => ({ tile: '/a.dmap', add: place, ...(dir && { adir: dir }) });
  assert.notEqual(addressLevel(a('/t', '')), addressLevel(a('/t', 'dungeon')));
  assert.notEqual(addressLevel(a('', '')), addressLevel(a('/t', '')));
  assert.equal(addressLevel(a('/t', 'x')), addressLevel(a('/t', 'x')));
  const tile = (extra = {}) => ({ tile: '/a.dmap', ...extra });
  assert.equal(stepsHistory(tile(), a('/t')), true, 'the map into Add');
  assert.equal(stepsHistory(a('/t'), a('/t', 'dungeon')), true, 'a folder into a folder');
  assert.equal(stepsHistory(a('/t', 'dungeon'), a('/t')), true, 'and back up to one');
  assert.equal(stepsHistory(a('/t', 'dungeon'), a('/t', 'dungeon')), false);
  assert.equal(stepsHistory(a('/t', 'dungeon'), tile({ pieces: 'p1' })), true);
  const place = { root: 0, label: 'My tiles', dir: '' };
  assert.deepEqual(crumbsFor({ place, tileName: 'first-plate.dmap', add: place }).slice(-4).map((c) => [c.label, c.current]), [['first-plate.dmap', false], ['Add', false], ['Places', false], ['My tiles', true]]);
});
