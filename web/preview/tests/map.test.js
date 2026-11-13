import test from 'node:test';
import assert from 'node:assert/strict';
import {
  BUDGET, atWord, bounds, chooseTiers, footprint, freshSeed, bottomWords, matrixOf, nearestFree, overlaps, ownedSeeds, pills, snapSquares, staleKeys, tierCounts, toWorld, totals, totalsText, turned, wantedTier,
} from '../js/map-model.js';

const piece = (over = {}) => ({
  id: 'p1', name: 'grass', canon: 'lib:tiles/ground/1x1-grass', seed_used: 97, system: 'db', colour: 'multi', print: 'missing',
  size: [35, 35], sq: 35, at: [0, 0], rot: 0, bottom_mm: 0, ...over,
});
const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} != ${b}`);

test('a piece stands with its north-west corner at its place, y down in the document and up in the world', () => {
  const p = piece({ at: [70, 35], size: [70, 35], sq: 35 });
  assert.deepEqual(footprint(p), [70, 35, 140, 70]);
  // the tile's south-west corner is at the footprint's south-west corner
  const [x, y, z] = toWorld(p, 0, 0);
  near(x, 70); near(y, -70); near(z, 0);
  // its north-east corner is the footprint's
  const [x2, y2] = toWorld(p, 70, 35);
  near(x2, 140); near(y2, -35);
});

test('turning goes clockwise about the footprint, and a quarter turn swaps its sides', () => {
  const p = piece({ at: [0, 0], size: [70, 35], rot: 90 });
  assert.deepEqual(footprint(p), [0, 0, 35, 70]);
  // the tile's north edge faces east after a clockwise quarter turn
  const sw = toWorld(p, 0, 0), nw = toWorld(p, 0, 35), ne = toWorld(p, 70, 35), se = toWorld(p, 70, 0);
  // north-west corner of the tile -> north-east of the footprint
  near(nw[0], 35); near(nw[1], 0);
  // south-west -> north-west
  near(sw[0], 0); near(sw[1], 0);
  // the east end goes south
  near(se[0], 0); near(se[1], -70);
  near(ne[0], 35); near(ne[1], -70);
  // half a turn: the tile's north-west corner is the footprint's south-east
  const q = piece({ at: [0, 0], rot: 180 });
  const c = toWorld(q, 0, 35);
  near(c[0], 35); near(c[1], -35);
  assert.equal(matrixOf(q).length, 16);
});

test('a raised piece is lifted by its bottom, and a turn leaves the bottom alone', () => {
  const p = piece({ bottom_mm: 33.1, rot: 270 });
  near(toWorld(p, 5, 5, 4)[2], 37.1);
  near(matrixOf(p)[14], 33.1);
});

test('a place snaps to the whole squares of the piece’s own system', () => {
  assert.deepEqual(snapSquares(piece(), 52, 17), { x: 1, y: 0 });
  const ol = piece({ system: 'ol', sq: 25.4 });
  assert.deepEqual(snapSquares(ol, 52, 17), { x: 2, y: 1 });
  assert.equal(atWord(2, 1), '(2sq, 1sq)');
  assert.equal(atWord(2.5, 0), '(2.5sq, 0sq)');
});

test('the nearest free place is the wish when it is free, else the nearest square that is', () => {
  const a = piece({ index: 0, at: [0, 0] });
  const b = piece({ index: 1, at: [0, 0] });
  assert.deepEqual(nearestFree([a], b, { x: 3, y: 0 }), { x: 3, y: 0, moved: 0 });
  const r = nearestFree([a], b, { x: 0, y: 0 });
  assert.ok(r.moved === 1 && !overlaps(footprint({ ...b, at: [r.x * 35, r.y * 35] }), footprint(a)));
  // a piece at another height is not in the way: it floats over
  const high = piece({ index: 2, at: [0, 0], bottom_mm: 33 });
  assert.equal(nearestFree([a], high, { x: 0, y: 0 }).moved, 0);
  // turned, a 2x1 stands 1x2: the free place is searched for the turned footprint
  const long = piece({ index: 3, size: [70, 35], at: [0, 0] });
  const wall = piece({ index: 4, at: [35, 0] });
  const turnedFree = nearestFree([wall], { ...long, rot: 90 }, { x: 0, y: 0 }, 90);
  assert.equal(turnedFree.moved, 0);
});

test('bounds covers the placed pieces only', () => {
  assert.equal(bounds([piece({ at: null })]), null);
  const b = bounds([piece({ at: [0, 0] }), piece({ index: 1, at: [70, 35], size: [70, 35] })]);
  assert.deepEqual(b, { lo: [0, 0], hi: [140, 70] });
});

test('a fresh seed is one you do not own, no piece uses and the tile does not have', () => {
  const owned = new Set([1, 2, 3]), used = new Set([4]);
  let n = 0;
  const seq = [0.00001, 0.00002, 0.00003, 0.00004, 0.00005, 0.5];
  const s = freshSeed(owned, used, 5, () => seq[n++]);
  assert.ok(![1, 2, 3, 4, 5].includes(s));
  assert.ok(n >= 6);
  assert.throws(() => freshSeed(new Set([1]), new Set(), 0, () => 0), /no free seed/);
});

const inv = {
  exists: true,
  pieces: [
    { tile: 'lib:tiles/ground/1x1-grass', seed: 97, system: 'db', colour: 'multi', have: 2 },
    { tile: 'lib:tiles/ground/1x1-grass', seed: 12, system: 'db', colour: 'multi', have: 1 },
    { tile: 'lib:tiles/ground/1x1-grass', seed: 5, system: 'ol', colour: 'multi', have: 4 },
    { tile: 'lib:tiles/ground/1x1-grass', seed: 8, system: 'db', colour: 'single', have: 1 },
    { tile: 'lib:tiles/ground/1x1-grass', seed: 31, system: 'db', colour: 'multi', have: 0 },
  ],
};

test('the seeds you own are of that tile, system and colour mode', () => {
  assert.deepEqual(ownedSeeds(inv, piece()), [{ seed: 12, have: 1 }, { seed: 97, have: 2 }]);
  assert.deepEqual(ownedSeeds(inv, piece({ system: 'ol' })), [{ seed: 5, have: 4 }]);
  assert.deepEqual(ownedSeeds(inv, piece({ colour: 'single' })), [{ seed: 8, have: 1 }]);
});

test('copies of one piece share what you own, and each state is said', () => {
  const ps = [
    piece({ id: 'p0' }), piece({ id: 'p1' }), piece({ id: 'p2' }),
    piece({ id: 'p3', seed_used: 8 }), piece({ id: 'p4', seed_used: 44 }), piece({ id: 'p5', print: 'skip' }), piece({ id: 'p6', print: 'always' }),
    piece({ id: 'p7', error: 'no such tile' }),
  ];
  const m = pills(ps, inv);
  assert.deepEqual(['p0', 'p1', 'p2'].map((i) => m.get(i).state), ['have', 'have', 'need']);
  // single colour owned, multicolour wanted: it prints, and says so
  assert.equal(m.get('p3').state, 'other');
  assert.equal(m.get('p4').state, 'need');
  assert.equal(m.get('p5').state, 'skip');
  // print=always prints whatever you have
  assert.equal(m.get('p6').state, 'need');
  assert.equal(m.has('p7'), false);
  assert.deepEqual(totals(ps, inv), { have: 2, toPrint: 4 });
  assert.equal(totalsText(ps, inv), '2 in inventory · 4 to print');
  assert.equal(totalsText(ps, { exists: false }), '6 to print');
  assert.equal(totalsText([], null), '0 to print');
});

test('a piece is drawn at the tier its size wants, held under the budget by lowering the smallest first', () => {
  assert.equal(wantedTier(30), 'super-low');
  assert.equal(wantedTier(120), 'low');
  assert.equal(wantedTier(400), 'full');
  assert.equal(wantedTier(30, true), 'low');
  const tris = (n) => (t) => ({ 'super-low': 10_000, low: 50_000, full: n }[t]);
  const all = new Set(['super-low', 'low', 'full']);
  // two full pieces of 400k fit, a third does not: the smallest drops to low
  const r = chooseTiers(['full', 'full', 'full'], [900, 800, 600], [all, all, all], [tris(400_000), tris(400_000), tris(400_000)]);
  assert.deepEqual(r.tiers, ['full', 'full', 'low']);
  assert.ok(r.total <= BUDGET);
  // only what is loaded can be drawn
  const some = new Set(['super-low']);
  const q = chooseTiers(['full', 'low', 'super-low'], [900, 300, 50], [some, all, new Set()], [tris(1), tris(1), tris(1)]);
  assert.deepEqual(q.tiers, ['super-low', 'low', null]);
  assert.deepEqual(tierCounts(q.tiers), { full: 0, low: 1, 'super-low': 1, none: 1 });
  // a budget that cannot be met by the loaded tiers is the best that can be done
  const z = chooseTiers(['full'], [900], [new Set(['full'])], [tris(2_000_000)]);
  assert.deepEqual(z.tiers, ['full']);
});

test('words for a piece’s level and its turn', () => {
  const names = [{ name: 'wallHeight', dmm: 33.1 }];
  assert.equal(bottomWords(piece(), names), 'ground level');
  assert.equal(bottomWords(piece({ bottom: '$wallHeight' }), names), '$wallHeight · 33.1 dmm');
  assert.equal(bottomWords(piece({ bottom: '20dmm' }), names), '20dmm');
  assert.equal(turned(270, 1), 0);
  assert.equal(turned(0, -1), 270);
});

test('a scene lets go of the geometry none of its pieces uses', () => {
  const held = new Map([['a|1', 1], ['b|2', 2], ['c|3', 3]]);
  const ps = [{ gkey: 'a|1' }, { gkey: null }, { gkey: 'a|1' }, { gkey: 'c|3' }];
  assert.deepEqual(staleKeys(held.keys(), ps), ['b|2']);
  assert.deepEqual(staleKeys(held.keys(), []), ['a|1', 'b|2', 'c|3'], 'an empty map keeps nothing');
  assert.deepEqual(staleKeys([], ps), []);
});
