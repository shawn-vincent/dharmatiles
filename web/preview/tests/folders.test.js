import test from 'node:test';
import assert from 'node:assert/strict';

import {
  KINDS, canDrop, contextForFolder, kindOf, levelExists, levelOfParams, levelOf, moveTargets, parentLevel, placeIndex, placeKey, placeParams,
  placesOf, relOf, renamedDir, shortPath, sizeLine, sizeSquares, stemOf, whereIs, wherePath, writable,
} from '../js/folders.js';
import { buildHash, parseHash } from '../js/nav.js';

const HOME = '/Users/me';
const roots = [
  { index: 0, label: 'My tiles', kind: 'dir', path: `${HOME}/dharmatiles`, role: 'home', writable: true, exists: true },
  { index: 1, label: 'Library', kind: 'library', path: 'lib:tiles/', role: 'library', writable: false },
  { index: 2, label: 'Tests', kind: 'library', path: 'lib:tests/', role: 'tests', writable: false },
  { index: 3, label: 'Dungeon dressing', kind: 'dir', path: '/Volumes/x/Dungeon dressing', role: 'added', writable: true, exists: true },
];
const tile = (root, path, extra = {}) => ({ id: `id:${path}`, root, path, name: stemOf(path), status: 'idle', ...extra });
const tiles = [
  tile(0, `${HOME}/dharmatiles/pond.dtile`, { size: '1x1' }),
  tile(0, `${HOME}/dharmatiles/dungeon/combo/tree.dtile`, { size: '2x1' }),
  tile(0, `${HOME}/dharmatiles/dungeon/combo/Rock 10.dtile`),
  tile(0, `${HOME}/dharmatiles/dungeon/combo/Rock 2.dtile`),
  tile(0, `${HOME}/dharmatiles/dungeon/hall.dtile`),
  tile(1, 'lib:tiles/ground/1x1-grass.dtile'),
  tile(1, 'lib:tiles/ground/1x1-flat.dtile'),
  tile(1, 'lib:tiles/water/2x1-pool.dtile'),
  tile(3, '/Volumes/x/Dungeon dressing/door.dtile'),
];
const folders = [
  { root: 0, path: 'dungeon' }, { root: 0, path: 'dungeon/combo' }, { root: 0, path: 'empty' }, { root: 0, path: 'dungeon/empty-too' },
];
const names = (v) => v.items.map((i) => stemOf(i.tile.path));

test('the places are listed My tiles first, then the folders you added, the library, the tests', () => {
  assert.deepEqual(placesOf([roots[2], roots[3], roots[1], roots[0]]).map((r) => r.label), ['My tiles', 'Dungeon dressing', 'Library', 'Tests']);
  assert.equal(writable(roots[0]), true);
  assert.equal(writable(roots[1]), false);
  assert.equal(placeKey(roots[1]), 'lib:tiles/');
  assert.equal(placeIndex(roots, `${HOME}/dharmatiles`), 0);
  assert.equal(placeIndex(roots, 'nowhere'), -1);
});

test('a tile has names below its place, and a place and a folder it is in', () => {
  assert.deepEqual(relOf(roots[0], `${HOME}/dharmatiles/dungeon/combo/tree.dtile`), ['dungeon', 'combo', 'tree.dtile']);
  assert.deepEqual(relOf(roots[1], 'lib:tiles/ground/1x1-grass.dtile'), ['ground', '1x1-grass.dtile']);
  assert.deepEqual(relOf({ kind: 'files', path: null }, '/a/b/c.dtile'), ['c.dtile']);
  assert.deepEqual(whereIs(roots, tiles[1]), { root: 0, dir: 'dungeon/combo' });
  assert.deepEqual(whereIs(roots, tiles[0]), { root: 0, dir: '' });
  assert.deepEqual(whereIs(roots, tile(9, '/x.dtile')), null);
});

test('a place lists its subfolders and then its tiles, each by name, with what is under each folder', () => {
  const top = levelOf({ roots, tiles, folders, root: 0 });
  assert.deepEqual(top.folders.map((f) => [f.name, f.dir, f.tiles, f.folders]), [['dungeon', 'dungeon', 4, 2], ['empty', 'empty', 0, 0]]);
  assert.deepEqual(names(top), ['pond']);
  assert.equal(top.total, 5);
  assert.equal(top.here, 1);
  const d = levelOf({ roots, tiles, folders, root: 0, dir: 'dungeon' });
  assert.deepEqual(d.folders.map((f) => [f.name, f.dir, f.tiles]), [['combo', 'dungeon/combo', 3], ['empty-too', 'dungeon/empty-too', 0]]);
  assert.deepEqual(names(d), ['hall']);
  // names sort the way a person counts: Rock 2 before Rock 10
  assert.deepEqual(names(levelOf({ roots, tiles, folders, root: 0, dir: 'dungeon/combo' })), ['Rock 2', 'Rock 10', 'tree']);
});

test('the library\'s groups are its folders, from its tiles\' paths alone', () => {
  const lib = levelOf({ roots, tiles, folders, root: 1 });
  assert.deepEqual(lib.folders.map((f) => [f.name, f.tiles]), [['ground', 2], ['water', 1]]);
  assert.deepEqual(names(levelOf({ roots, tiles, folders, root: 1, dir: 'ground' })), ['1x1-flat', '1x1-grass']);
});

test('a place with nothing in it lists nothing, and a place that is not there is null', () => {
  assert.deepEqual(levelOf({ roots, tiles: [], folders: [], root: 0 }).items, []);
  assert.equal(levelOf({ roots, tiles, folders, root: 7 }), null);
});

test('a filter searches the whole place, at any depth, and says where each tile is', () => {
  const q = levelOf({ roots, tiles, folders, root: 0, query: 'ROCK' });
  assert.deepEqual(q.items.map((i) => [stemOf(i.tile.path), i.where]), [['Rock 2', 'dungeon/combo'], ['Rock 10', 'dungeon/combo']]);
  assert.deepEqual(q.folders, []);
  assert.deepEqual(levelOf({ roots, tiles, folders, root: 0, query: 'dungeon/hall' }).items.map((i) => stemOf(i.tile.path)), ['hall']);
  assert.deepEqual(levelOf({ roots, tiles, folders, root: 0, query: 'zzz' }).items, []);
});

test('a level exists when its place is listed and the folder is one of it', () => {
  assert.equal(levelExists({ roots, tiles, folders, root: 0, dir: 'dungeon/combo' }), true);
  assert.equal(levelExists({ roots, tiles, folders, root: 0, dir: 'empty' }), true);
  assert.equal(levelExists({ roots, tiles, folders, root: 1, dir: 'ground' }), true);
  assert.equal(levelExists({ roots, tiles, folders, root: 0, dir: 'gone' }), false);
  assert.equal(levelExists({ roots, tiles, folders, root: 9, dir: '' }), false);
  assert.equal(levelExists({ roots, tiles, folders, root: 0, dir: '' }), true);
});

test('up goes from a folder to its parent, from a place to Places, and stops there', () => {
  assert.deepEqual(parentLevel({ root: 0, dir: 'dungeon/combo' }), { root: 0, dir: 'dungeon' });
  assert.deepEqual(parentLevel({ root: 0, dir: 'dungeon' }), { root: 0, dir: '' });
  assert.deepEqual(parentLevel({ root: 0, dir: '' }), { root: null, dir: '' });
  assert.equal(parentLevel({ root: null, dir: '' }), null);
});

test('the strip says how many tiles a folder holds and where it is on disk, short', () => {
  const ctx = (level) => contextForFolder({ roots, level, view: levelOf({ roots, tiles, folders, ...level }), home: HOME });
  assert.deepEqual(ctx({ root: 0, dir: '' }), ['5 tiles (1 here) · 2 folders', '~/dharmatiles']);
  assert.deepEqual(ctx({ root: 0, dir: 'dungeon/combo' }), ['3 tiles', '~/dharmatiles/dungeon/combo']);
  assert.deepEqual(ctx({ root: 1, dir: 'ground' }), ['2 tiles', 'library · tiles/ground · read only']);
  assert.deepEqual(ctx({ root: 2, dir: '' }), ['0 tiles', 'test tiles · tests · read only']);
  assert.deepEqual(contextForFolder({ roots, level: { root: null }, view: null, home: HOME }), ['4 places', 'your tiles, folders you added, the library']);
  assert.deepEqual(contextForFolder({ roots: [{ ...roots[0], exists: false }], level: { root: 0, dir: '' }, view: levelOf({ roots: [roots[0]], tiles: [], folders: [], root: 0 }), home: HOME }),
    ['0 tiles', '~/dharmatiles · not there yet']);
});

test('a long path keeps its end, and the home folder is a tilde', () => {
  assert.equal(shortPath(`${HOME}/dharmatiles`, HOME), '~/dharmatiles');
  assert.equal(shortPath(`${HOME}/Documents/games/dungeon dressing/combat/boss rooms`, HOME), '~/…/combat/boss rooms');
  assert.equal(shortPath('/Volumes/Big Drive/tiles/printed/finished/pieces', HOME), '/…/finished/pieces');
  assert.equal(shortPath('/a/b', HOME), '/a/b');
  assert.equal(wherePath(roots[3], 'x', HOME), '/Volumes/x/Dungeon dressing/x');
});

test('the kinds a folder lists are one table', () => {
  assert.deepEqual(KINDS.map((k) => k.kind), ['tile', 'map', 'inventory', 'stl']);
  assert.equal(kindOf('inventory.dinv').kind, 'inventory', 'the inventory is listed beside the tiles');
  assert.equal(kindOf('inventory.dinv').actions, false);
  assert.equal(kindOf('/a/b.DTILE').kind, 'tile');
  assert.equal(kindOf('kit.dmap').kind, 'map', 'a print list is listed in its folder');
  assert.equal(kindOf('kit.dmap').actions, false);
  assert.equal(kindOf('lib:tiles/x.dtile').actions, true);
  assert.equal(kindOf('x.stl').actions, false);
  assert.equal(kindOf('notes.txt'), undefined);
});

test('a tile row says its size, from the build once there is one', () => {
  assert.equal(sizeLine({ size: '2x1' }), '2×1');
  assert.equal(sizeLine({ size: '2x1', plan: { cols: 3, rows: 1 } }), '3×1');
  assert.equal(sizeLine({}), '');
  assert.deepEqual(sizeSquares({ size: '2x3' }), [2, 3]);
  assert.equal(sizeSquares({ size: 'big' }), null);
});

test('a folder level is an address: place and dir, never a tile', () => {
  assert.deepEqual(placeParams(roots, { root: null }), { place: '' });
  assert.deepEqual(placeParams(roots, { root: 0, dir: '' }), { place: `${HOME}/dharmatiles` });
  assert.deepEqual(placeParams(roots, { root: 0, dir: 'dungeon/combo' }), { place: `${HOME}/dharmatiles`, dir: 'dungeon/combo' });
  // the round trip through the URL's hash
  for (const level of [{ root: null, dir: '' }, { root: 0, dir: '' }, { root: 3, dir: 'a b/c' }, { root: 1, dir: 'ground' }]) {
    const hash = buildHash(placeParams(roots, level));
    assert.deepEqual(levelOfParams(roots, parseHash(hash)), level, hash);
  }
  assert.equal(buildHash(placeParams(roots, { root: null })), '#place=');
  assert.deepEqual(levelOfParams(roots, parseHash('#place=')), { root: null, dir: '' });
  // a tile's address, or no address, is not a folder level
  assert.equal(levelOfParams(roots, parseHash('#tile=%2Fa.dtile')), null);
  assert.equal(levelOfParams(roots, parseHash('')), null);
  // a place that is not listed (removed since) is said, not guessed
  assert.deepEqual(levelOfParams(roots, parseHash('#place=%2Fgone')), { missing: '/gone' });
  // a folder path is made of its names only
  assert.deepEqual(levelOfParams(roots, { place: `${HOME}/dharmatiles`, dir: '/dungeon/' }), { root: 0, dir: 'dungeon' });
});

test('a tile is dropped on a folder of a place you own, not on the one it is in, nor on the library', () => {
  const t = tiles[1]; // dungeon/combo/tree
  assert.deepEqual(canDrop(roots, t, { root: 0, dir: 'dungeon' }), { ok: true });
  assert.deepEqual(canDrop(roots, t, { root: 0, dir: '' }), { ok: true });
  assert.equal(canDrop(roots, t, { root: 0, dir: 'dungeon/combo' }).why, 'already in this folder');
  assert.match(canDrop(roots, t, { root: 1, dir: 'ground' }).why, /read only/);
  assert.deepEqual(canDrop(roots, t, { root: 3, dir: '' }), { ok: true }, 'into another place you own');
  assert.equal(canDrop(roots, t, { root: null }).ok, false, 'Places is no folder');
  // a library tile is copied, never moved
  assert.match(canDrop(roots, tiles[5], { root: 0, dir: '' }).why, /copied/);
  // only a tile document moves
  assert.equal(canDrop(roots, tile(0, `${HOME}/dharmatiles/x.stl`), { root: 0, dir: 'dungeon' }).ok, false);
});

test('a tile\'s Move menu offers every other folder you own, its place first', () => {
  const t = tiles[1];
  const list = moveTargets(roots, folders, t);
  assert.deepEqual(list.map((x) => [x.root, x.dir, x.label]), [
    [0, '', 'My tiles'], [0, 'dungeon', 'dungeon'], [0, 'dungeon/empty-too', 'dungeon/empty-too'], [0, 'empty', 'empty'], [3, '', 'Dungeon dressing'],
  ]);
  assert.deepEqual(moveTargets(roots, folders, tiles[5]), [], 'nothing to move a library tile to');
});

test('a renamed folder takes the folders below it, and the one you are in, with it', () => {
  assert.equal(renamedDir('dungeon', 'dungeon', 'keep'), 'keep');
  assert.equal(renamedDir('dungeon/combo', 'dungeon', 'keep'), 'keep/combo');
  assert.equal(renamedDir('dungeon2', 'dungeon', 'keep'), 'dungeon2');
  assert.equal(renamedDir('other', 'dungeon', 'keep'), 'other');
});

test('a level that is gone settles on its nearest parent that is there: the one rule for the browser and the Add tiles pane', async () => {
  const { settledLevel } = await import('../js/folders.js');
  const roots = [{ kind: 'dir', path: '/h', label: 'My tiles', writable: true }];
  const tiles = [{ id: 'a', root: 0, path: '/h/dungeon/a.dtile' }];
  const state = { roots, tiles, folders: [] };
  assert.deepEqual(settledLevel(state, { root: 0, dir: 'dungeon' }), { root: 0, dir: 'dungeon' });
  assert.deepEqual(settledLevel(state, { root: 0, dir: 'dungeon/gone/deeper' }), { root: 0, dir: 'dungeon' });
  assert.deepEqual(settledLevel(state, { root: 0, dir: 'nope' }), { root: 0, dir: '' });
  assert.deepEqual(settledLevel(state, { root: 5, dir: '' }), { root: null, dir: '' });
  assert.deepEqual(settledLevel(state, { root: null, dir: '' }), { root: null, dir: '' });
});
