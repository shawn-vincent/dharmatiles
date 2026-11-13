import test from 'node:test';
import assert from 'node:assert/strict';
import {
  BUDGET, DOWN, GUESS, STICKY, UP, atWord, placeWords, bounds, chooseTiers, footprint, bottomWords, matrixOf, overlaps, marks, snapSquares, staleKeys, planTiers, tierCounts, toWorld, totalsText, turned, wantedTier,
} from '../js/map-model.js';

const piece = (over = {}) => ({
  id: 'p1', name: 'grass', canon: 'lib:tiles/ground/1x1-grass', seedUsed: 97, system: 'db', colour: 'multi', print: 'missing',
  size: [35, 35], sq: 35, at: [0, 0], rot: 0, bottomMm: 0, ...over,
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
  const p = piece({ bottomMm: 33.1, rot: 270 });
  near(toWorld(p, 5, 5, 4)[2], 37.1);
  near(matrixOf(p)[14], 33.1);
});

test('a place snaps to the whole squares of the piece’s own system', () => {
  assert.deepEqual(snapSquares(piece(), 52, 17), { x: 1, y: 0 });
  const ol = piece({ system: 'ol', sq: 25.4 });
  assert.deepEqual(snapSquares(ol, 52, 17), { x: 2, y: 1 });
  assert.equal(atWord(2, 1), '(2sq, 1sq)');
  assert.equal(atWord(2.5, 0), '(2.5sq, 0sq)');
  assert.equal(placeWords(piece({ at: [0, 0] })), 'column 1, row 1');
  assert.equal(placeWords(piece({ at: [70, 35] })), 'column 3, row 2');
  assert.equal(placeWords(piece({ at: null })), 'in the tray');
});

test('bounds covers the placed pieces only', () => {
  assert.equal(bounds([piece({ at: null })]), null);
  const b = bounds([piece({ at: [0, 0] }), piece({ index: 1, at: [70, 35], size: [70, 35] })]);
  assert.deepEqual(b, { lo: [0, 0], hi: [140, 70] });
});

test('the shelf\'s word on each piece is the program\'s, and the totals say its numbers', () => {
  const ps = [{ id: 'a', shelf: { state: 'have', text: 'have' } }, { id: 'b', shelf: { state: 'need', text: 'to print' } }, { id: 'c' }];
  assert.deepEqual([...marks(ps).keys()], ['a', 'b']);
  assert.equal(totalsText({ shelfExists: true, have: 2, toPrint: 4 }), '2 in inventory · 4 to print');
  assert.equal(totalsText({ shelfExists: false, have: 0, toPrint: 6 }), '6 to print');
  assert.equal(totalsText(null), '0 to print');
});

const ALL = new Set(['super-low', 'low', 'full']);
const cost = (n) => () => (t) => ({ 'super-low': 10_000, low: 50_000, full: n }[t]);
const costs = (n, k) => Array.from({ length: k }, cost(n));

test('a piece wants the tier its size says, from the tier it wanted before: up at one size, down at a lower', () => {
  assert.equal(wantedTier(30), 'super-low');
  assert.equal(wantedTier(60), 'low');
  assert.equal(wantedTier(400), 'full');
  assert.equal(wantedTier(30, null, true), 'low');
  // between the two sizes it stays where it was
  const mid = (UP.full + DOWN.full) / 2;
  assert.equal(wantedTier(mid, 'low'), 'low');
  assert.equal(wantedTier(mid, 'full'), 'full');
  assert.equal(wantedTier(UP.full, 'low'), 'full');
  assert.equal(wantedTier(DOWN.full - 0.1, 'full'), 'low');
  assert.equal(wantedTier((UP.low + DOWN.low) / 2, 'super-low'), 'super-low');
  assert.equal(wantedTier((UP.low + DOWN.low) / 2, 'low'), 'low');
  assert.equal(wantedTier(DOWN.low - 0.1, 'low'), 'super-low');
  // a piece that was full and is far away goes down through low in one look
  assert.equal(wantedTier(10, 'full'), 'super-low');
});

test('a camera that jitters by 3% changes no piece\'s tier unless it crosses a margin', () => {
  // the margins are wider than a 3% wobble each way (both ratios are over 1.06)
  assert.ok(UP.full / DOWN.full > 1.06 && UP.low / DOWN.low > 1.06);
  let seed = 7;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) * 2 - 1;
  for (const base of [20, 31, 35, 41, 60, 92, 100, 110, 119, 121, 300, 800]) {
    for (const was of ['super-low', 'low', 'full']) {
      // settle on the base size first: this is the tier the piece holds there
      const settled = wantedTier(base, was);
      let t = settled;
      for (let k = 0; k < 400; k++) {
        t = wantedTier(base * (1 + 0.03 * rnd()), t);
        // the tier can only differ from the settled one if a jittered size reached a threshold the other side
        const up = base * 1.03, down = base * 0.97;
        const crossed = (settled === 'super-low' && up >= UP.low) || (settled === 'low' && (up >= UP.full || down < DOWN.low)) || (settled === 'full' && down < DOWN.full);
        if (!crossed) assert.equal(t, settled, `base ${base} was ${was}`);
      }
    }
  }
});

test('equal neighbours give the same answer whatever order the list is in, and a tie goes to the nearer, then the id', () => {
  const ids = ['a', 'b', 'c', 'd'];
  const run = (order) => {
    const o = order.map((k) => ids[k]);
    const r = chooseTiers({
      want: o.map(() => 'full'), px: o.map(() => 200), have: o.map(() => ALL), tris: costs(400_000, 4),
      current: o.map(() => null), dist: o.map((id) => (id === 'd' ? 100 : 500)), ids: o, budget: 900_000,
    });
    return Object.fromEntries(o.map((id, i) => [id, r.tiers[i]]));
  };
  const first = run([0, 1, 2, 3]);
  // 900k holds two fulls of 400k with the rest low: the nearer (d), then the lowest id (a)
  assert.deepEqual(first, { a: 'full', b: 'low', c: 'low', d: 'full' });
  for (const order of [[3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1], [0, 3, 1, 2]]) assert.deepEqual(run(order), first);
});

test('a piece that holds full keeps it against a piece that is not clearly bigger; a clearly bigger one takes it', () => {
  const input = (pxB) => ({
    want: ['full', 'full'], px: [200, pxB], have: [ALL, ALL], tris: costs(600_000, 2),
    current: ['full', 'low'], dist: [500, 500], ids: ['a', 'b'], budget: 700_000,
  });
  assert.deepEqual(chooseTiers(input(205)).tiers, ['full', 'low']);
  assert.deepEqual(chooseTiers(input(200 * STICKY - 1)).tiers, ['full', 'low']);
  assert.deepEqual(chooseTiers(input(200 * STICKY + 1)).tiers, ['low', 'full']);
});

test('a mesh arriving does not demote a full piece of equal size', () => {
  // b's full has just landed; a has been full a while: the room is a's
  const r = chooseTiers({
    want: ['full', 'full'], px: [200, 200], have: [ALL, ALL], tris: costs(600_000, 2),
    current: ['full', 'full'], dist: [900, 100], ids: ['a', 'b'], budget: 700_000,
  });
  assert.deepEqual(r.tiers.slice().sort(), ['full', 'low']);
  // and the one that holds it by being first does not change with the next look at the same sizes
  const again = chooseTiers({
    want: ['full', 'full'], px: [200, 200], have: [ALL, ALL], tris: costs(600_000, 2),
    current: r.tiers, dist: [900, 100], ids: ['a', 'b'], budget: 700_000,
  });
  assert.deepEqual(again.tiers, r.tiers);
});

test('the near pieces are all full at the budget, fetched together, nearest first', () => {
  const k = 12;
  const px = Array.from({ length: k }, (_, i) => 400 - i * 25); // 400 ... 125
  const want = px.map((v) => wantedTier(v));
  const ids = px.map((_, i) => `p${i}`);
  const tris = Array.from({ length: k }, () => (t) => GUESS[t]);
  const nearFull = want.filter((w) => w === 'full').length;
  assert.ok(nearFull >= 5, 'the sizes make several pieces want full');
  const plan = planTiers({ want, px, have: px.map(() => new Set(['super-low'])), tris, current: px.map(() => null), dist: px.map((_, i) => 100 + i), ids });
  // everything the budget holds is asked for at once: not one at a time
  const asked = plan.fetch.filter(([, t]) => t === 'full').map(([i]) => i);
  assert.equal(asked.length, plan.intended.filter((t) => t === 'full').length, 'all that the budget holds in full');
  assert.ok(plan.intended.reduce((n, t) => n + GUESS[t], 0) <= BUDGET);
  assert.ok(asked.length >= 2, 'more than one is asked for at once');
  assert.deepEqual(asked, asked.slice().sort((a, b) => a - b), 'nearest first');
  assert.deepEqual(asked, Array.from({ length: asked.length }, (_, i) => i), 'the biggest on screen');
  // with them loaded they are drawn full, inside the budget
  const done = planTiers({ want, px, have: px.map((_, i) => (asked.includes(i) ? ALL : new Set(['super-low', 'low']))), tris, current: px.map((_, i) => (asked.includes(i) ? 'full' : 'low')), dist: px.map((_, i) => 100 + i), ids });
  assert.deepEqual(done.tiers.filter((t) => t === 'full').length, asked.length);
  assert.ok(done.total <= BUDGET);
});

test('only what is loaded can be drawn, and a budget the loaded tiers cannot meet is the best that can be done', () => {
  const some = new Set(['super-low']);
  const q = chooseTiers({ want: ['full', 'low', 'super-low'], px: [900, 300, 50], have: [some, ALL, new Set()], tris: costs(1, 3) });
  assert.deepEqual(q.tiers, ['super-low', 'low', null]);
  assert.deepEqual(tierCounts(q.tiers), { full: 0, low: 1, 'super-low': 1, none: 1 });
  const z = chooseTiers({ want: ['full'], px: [900], have: [new Set(['full'])], tris: costs(2_000_000, 1) });
  assert.deepEqual(z.tiers, ['full']);
  // the smallest on screen is lowered first
  const r = chooseTiers({ want: ['full', 'full', 'full'], px: [900, 800, 600], have: [ALL, ALL, ALL], tris: costs(400_000, 3), budget: 900_000 });
  assert.deepEqual(r.tiers, ['full', 'full', 'low']);
  assert.ok(r.total <= BUDGET);
  // a full is lowered before a low is: lowering a low saves little
  const f = chooseTiers({ want: ['full', 'low', 'low'], px: [900, 100, 90], have: [ALL, ALL, ALL], tris: costs(400_000, 3), budget: 300_000 });
  assert.deepEqual(f.tiers, ['low', 'low', 'low']);
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
