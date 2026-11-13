import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_LIST, listFrom, matches, pillsWanted, sectionsOf } from '../js/map-list.js';
import { marks } from '../js/map-model.js';

const piece = (id, name, state, over = {}) => ({ id, name, canon: `lib:${name}`, at: [0, 0], shelf: state ? { state, text: state } : undefined, ...over });
const world = [
  piece('p1', 'grass', 'have'),
  piece('p2', 'grass', 'need'),
  piece('p3', 'Water', 'need'),
  piece('p4', 'cliff', 'other'),
  piece('p5', 'cliff', 'other'),
  piece('p6', 'cliff', 'have'),
  piece('p7', 'wall', 'skip'),
  piece('t1', 'tree', 'need', { at: null, count: 3 }),
  piece('t2', 'flowers', 'skip', { at: null, count: 1 }),
];
const marksOf = marks(world);
const names = (rows) => rows.map((r) => r.name);

test('the pieces fall in three sections: placed by tile, the tray by piece, and what is left off wherever it stands', () => {
  const s = sectionsOf(world, marksOf);
  assert.deepEqual(names(s.placed), ['cliff', 'grass', 'Water']);
  assert.deepEqual(names(s.tray), ['tree']);
  assert.deepEqual(names(s.off), ['flowers', 'wall']);
  assert.equal(s.shown, 9);
  const cliff = s.placed[0];
  assert.deepEqual([cliff.count, cliff.have, cliff.toPrint, cliff.other], [3, 1, 2, 2]);
  assert.equal(s.tray[0].count, 3);
  assert.equal(s.off[1].skip, 1);
});

test('empty sections are empty lists, so the panel hides them', () => {
  const s = sectionsOf([piece('p1', 'grass', 'have')], marks([piece('p1', 'grass', 'have')]));
  assert.deepEqual([s.placed.length, s.tray.length, s.off.length], [1, 0, 0]);
});

test('sorts: name ignores case; count and to print go biggest first; ties fall back to name, then id', () => {
  assert.deepEqual(names(sectionsOf(world, marksOf, { sort: 'name' }).placed), ['cliff', 'grass', 'Water']);
  assert.deepEqual(names(sectionsOf(world, marksOf, { sort: 'count' }).placed), ['cliff', 'grass', 'Water']);
  // cliff 2 to print, grass 1, Water 1: the tie of grass and Water breaks by name
  assert.deepEqual(names(sectionsOf(world, marksOf, { sort: 'print' }).placed), ['cliff', 'grass', 'Water']);
  const ps = [piece('b', 'moss', 'have'), piece('a', 'moss', 'have', { canon: 'lib:moss2' }), piece('c', 'alder', 'have')];
  const rows = sectionsOf(ps, marks(ps), { sort: 'count' }).placed;
  assert.deepEqual(rows.map((r) => r.firstId), ['c', 'a', 'b']); // equal counts and names: by id
  const flip = sectionsOf([...ps].reverse(), marks(ps), { sort: 'count' }).placed;
  assert.deepEqual(flip.map((r) => r.firstId), ['c', 'a', 'b']); // the input's order does not matter
  const pr = [piece('x', 'rock', 'have'), piece('y', 'moss', 'need'), piece('z', 'moss', 'need')];
  assert.deepEqual(names(sectionsOf(pr, marks(pr), { sort: 'print' }).placed), ['moss', 'rock']);
  assert.deepEqual(names(sectionsOf(pr, marks(pr), { sort: 'name' }).placed), ['moss', 'rock']);
});

test('filters take pieces out before the rows are made', () => {
  const print = sectionsOf(world, marksOf, { filter: 'print' });
  assert.deepEqual(names(print.placed), ['cliff', 'grass', 'Water']);
  assert.equal(print.placed[0].count, 2); // the cliff that is had is not in its row
  assert.equal(print.off.length, 0);
  const have = sectionsOf(world, marksOf, { filter: 'have' });
  assert.deepEqual(names(have.placed), ['cliff', 'grass']);
  assert.deepEqual([have.placed[0].count, have.tray.length, have.off.length], [1, 0, 0]);
  const other = sectionsOf(world, marksOf, { filter: 'other' });
  assert.deepEqual(names(other.placed), ['cliff']);
  assert.equal(other.shown, 2);
  assert.equal(sectionsOf(world, marksOf, { filter: 'all' }).shown, 9);
});

test('the search matches every word of it in a tile name, ignoring case', () => {
  assert.equal(matches({ name: '1x1-grass-flowers' }, 'FLOW grass'), true);
  assert.equal(matches({ name: '1x1-grass' }, 'water'), false);
  assert.equal(matches({ name: 'x' }, '   '), true);
  const s = sectionsOf(world, marksOf, { query: 'gra' });
  assert.deepEqual([names(s.placed), s.tray.length, s.off.length, s.shown], [['grass'], 0, 0, 2]);
  const both = sectionsOf(world, marksOf, { query: 'cliff', filter: 'have' });
  assert.equal(both.shown, 1);
});

test('a piece the program has no word for counts as printing', () => {
  const ps = [piece('p1', 'grass', null)];
  const s = sectionsOf(ps, marks(ps), { filter: 'print' });
  assert.equal(s.placed[0].toPrint, 1);
});

test('a stored choice that is not one of ours is the default', () => {
  assert.deepEqual(listFrom(undefined), DEFAULT_LIST);
  assert.deepEqual(listFrom({ sort: 'bogus', filter: 'nope' }), DEFAULT_LIST);
  assert.deepEqual(listFrom({ sort: 'count', filter: 'have', query: 'a' }), { sort: 'count', filter: 'have', query: 'a' });
});

test('the pills over the pieces are off by default: only a stored 1 turns them on', () => {
  assert.equal(pillsWanted(null), false); // nothing stored
  assert.equal(pillsWanted('0'), false); // the old default
  assert.equal(pillsWanted('1'), true);
});
