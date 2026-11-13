// The inventory's model (js/inventory.js): how a tile is named in the log, what is had per system and colour mode,
// the pills, the plates' lines and what the document's view lists.

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  canonOf, defaultFilters, exists, groups, haveLine, nameOf, numbered, passes, pill, plateTotals, relinkCandidates, seedsText, tileHave, totalsLine, wantLine,
} from '../js/inventory.js';

const P = (tile, seed, system, colour, have, extra = {}) => ({ tile, seed, system, colour, have, printed: have, failed: 0, gone: 0, hash: null, last: '2026-11-09', edited: false, missing: false, lines: [], ...extra });

const inv = {
  exists: true, home: '/Users/x/dharmatiles', path: '/Users/x/dharmatiles/inventory.dinv',
  pieces: [
    P('lib:tiles/ground/1x1-grass', 97, 'db', 'multi', 1),
    P('lib:tiles/ground/1x1-grass', 12, 'db', 'multi', 0, { printed: 1, gone: 1 }),
    P('lib:tiles/ground/1x1-grass', 7, 'ol', 'single', 2),
    P('lib:tiles/walls/1x1-wall-stone', 14, 'db', 'single', 1),
    P('lib:tiles/cliffs/2x1-cliff', 4, 'db', 'multi', 1, { failed: 1, edited: true }),
    P('home:meadows/glade', 3, 'db', 'single', 1, { missing: true }),
  ],
};

test('a tile is named in the log the way the program names it', () => {
  assert.equal(canonOf('lib:tiles/ground/1x1-grass.dtile', ''), 'lib:tiles/ground/1x1-grass');
  assert.equal(canonOf('/Users/x/dharmatiles/meadows/glade.dtile', '/Users/x/dharmatiles'), 'home:meadows/glade');
  assert.equal(canonOf('/Users/x/dharmatiles/meadows/glade.dtile', '/Users/x/dharmatiles/'), 'home:meadows/glade');
  assert.equal(canonOf('/elsewhere/glade.dtile', '/Users/x/dharmatiles'), '/elsewhere/glade');
  assert.equal(canonOf('home:a/b', ''), 'home:a/b');
  assert.equal(nameOf('home:meadows/glade'), 'glade');
});

test('what you have is per system: DungeonBlocks and OpenLOCK never count for each other', () => {
  const db = tileHave(inv, 'lib:tiles/ground/1x1-grass', 'db');
  assert.equal(db.have, 1);
  assert.deepEqual(db.seeds, [{ seed: 97, colour: 'multi', have: 1 }]);
  assert.equal(db.gone, 1);
  const ol = tileHave(inv, 'lib:tiles/ground/1x1-grass', 'ol');
  assert.equal(ol.have, 2);
  assert.equal(ol.single, 2);
  assert.equal(tileHave(inv, 'lib:tiles/walls/1x1-wall-stone', 'ol').have, 0);
});

test('the tile level says what is printed and which seeds', () => {
  assert.equal(haveLine(tileHave(inv, 'lib:tiles/ground/1x1-grass', 'db')), '1 printed in multicolour · seeds 97');
  assert.equal(haveLine(tileHave(inv, 'lib:tiles/ground/1x1-grass', 'ol')), '2 printed · seeds 7 ×2');
  assert.equal(haveLine(tileHave(inv, 'lib:tiles/nothing/x', 'db')), 'none printed yet');
  assert.match(haveLine({ have: 0, failed: 1, gone: 1, seeds: [] }), /none on the shelf \(1 failed, 1 gone\)/);
  assert.equal(seedsText([{ seed: 7, have: 2 }, { seed: 12, have: 1 }, { seed: 97, have: 1 }]), '7 ×2, 12, 97');
  const mixed = tileHave({ pieces: [P('a', 1, 'db', 'single', 2), P('a', 2, 'db', 'multi', 1)] }, 'a', 'db');
  assert.match(haveLine(mixed), /3 printed \(2 single colour, 1 multicolour\)/);
});

test('the pills: have, need, none without an inventory, edited since printing in the title', () => {
  const home = inv.home;
  const row = (name) => `${home}/${name}.dtile`;
  assert.deepEqual(pill(inv, 'lib:tiles/ground/1x1-grass.dtile', 'db').text, 'have 1');
  assert.equal(pill(inv, 'lib:tiles/ground/1x1-grass.dtile', 'ol').text, 'have 2');
  assert.equal(pill(inv, 'lib:tiles/walls/1x1-wall-stone.dtile', 'ol').kind, 'need');
  assert.match(pill(inv, 'lib:tiles/cliffs/2x1-cliff.dtile', 'db').title, /edited since printing/);
  assert.equal(pill(inv, row('meadows/glade'), 'db').text, 'have 1', 'a tile in My tiles is found by its home: name');
  assert.equal(pill({ exists: false, pieces: [] }, 'lib:tiles/x.dtile', 'db'), null, 'no inventory yet: nothing to have or need');
  assert.equal(pill(null, 'lib:tiles/x.dtile', 'db'), null);
});

test('a multicolour piece wanted where only single colour is owned reads have, single colour', () => {
  const w = wantLine({ want: 2, have: 0, count: 2, otherColour: 1 }, 'multicolour');
  assert.equal(w.caution, true);
  assert.match(w.text, /^have, single colour · 2 to print$/);
  const plain = wantLine({ want: 3, have: 2, count: 1, otherColour: 0 }, 'multicolour');
  assert.equal(plain.caution, false);
  assert.equal(plain.text, 'have 2 · 1 to print');
  assert.equal(wantLine({ want: 1, have: 0, count: 1, otherColour: 1 }, 'single').text, 'have, multicolour · 1 to print');
  // owning the exact piece is not a caution
  assert.equal(wantLine({ want: 1, have: 1, count: 0, otherColour: 1 }, 'multicolour').caution, false);
});

test('the plates say how many are in the inventory and how many print', () => {
  const p = { kinds: [{ count: 2, have: 1 }, { count: 3, have: 0 }], held: [{ name: 'a', have: 4 }] };
  assert.deepEqual(plateTotals(p), { held: 5, toPrint: 5 });
  assert.deepEqual(plateTotals({}), { held: 0, toPrint: 0 });
});

test('the view groups by tile under the filters, and the system defaults to the target', () => {
  const f = defaultFilters('db');
  const gs = groups(inv, f);
  assert.deepEqual(gs.map((g) => g.name), ['1x1-cliff'.replace('1x1-', '2x1-'), 'glade', '1x1-grass', '1x1-wall-stone'].sort((a, b) => a.localeCompare(b, undefined, { numeric: true })));
  const grass = gs.find((g) => g.name === '1x1-grass');
  assert.deepEqual(grass.pieces.map((p) => p.seed), [12, 97], 'its pieces by seed');
  assert.equal(grass.have, 1);
  assert.equal(gs.find((g) => g.name === '2x1-cliff').edited, true);
  assert.equal(gs.find((g) => g.name === 'glade').missing, true);
  assert.deepEqual(groups(inv, { ...f, system: 'ol' }).map((g) => g.name), ['1x1-grass']);
  assert.deepEqual(groups(inv, { ...f, colour: 'single' }).map((g) => g.name).sort(), ['1x1-wall-stone', 'glade']);
  assert.equal(groups(inv, { ...f, show: 'off' }).length, 1, 'only the grass seed 12 that is gone');
  assert.equal(passes(inv.pieces[1], { ...f, show: 'have' }), false);
  assert.match(totalsLine(gs), /4 pieces on the shelf · 1 failed · 1 gone/);
});

test('a tile that moved offers the same name first', () => {
  const tiles = [{ id: 'a', path: '/h/x/other.dtile' }, { id: 'b', path: '/h/y/glade.dtile' }, { id: 'c', path: '/h/m.dmap' }];
  assert.deepEqual(relinkCandidates(tiles, 'home:meadows/glade').map((t) => t.id), ['b', 'a']);
});

test('the source is shown with line numbers', () => {
  assert.deepEqual(numbered('a\nb\n'), [[1, 'a'], [2, 'b']]);
  assert.equal(exists({ exists: true }), true);
  assert.equal(exists(null), false);
});
