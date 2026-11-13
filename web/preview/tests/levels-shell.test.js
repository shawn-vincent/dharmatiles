// The levels shell is one mechanism for every document's levels (editor/levels.js): which level shows and which way it slides is
// nav.js, and no view draws its own level switching.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { LEVELS, folderDepth, mapLevelOf, slideBetween, slideDir, tileLevelOf, trailDepth } from '../js/nav.js';

const entry = { kind: 'entry', id: 'rock' };

test('which level a tile and a map are at', () => {
  assert.equal(tileLevelOf({ kind: 'tile' }), 'tile');
  assert.equal(tileLevelOf(entry), 'entry');
  assert.equal(tileLevelOf(entry, { name: 'size', group: 0 }), 'control');
  assert.equal(mapLevelOf({ pieces: 0 }), 'map');
  assert.equal(mapLevelOf({ pieces: 3 }), 'piece');
  assert.equal(mapLevelOf({ pieces: 1, prop: 'seed' }), 'prop');
  assert.equal(mapLevelOf({ pieces: 2, prop: 'seed' }), 'piece', 'a property is of one piece');
  assert.equal(mapLevelOf({ pieces: 0, prop: 'seed' }), 'map');
  assert.equal(mapLevelOf({ pieces: 0, add: true }), 'add');
  assert.equal(mapLevelOf({ pieces: 2, add: true }), 'piece', 'a piece selected leaves Add');
});

test('the Add tiles level is one step deeper than the map and a sibling of a piece', () => {
  assert.equal(slideBetween('map', 'add'), 1);
  assert.equal(slideBetween('add', 'map'), -1);
  assert.equal(slideBetween('add', 'prop'), 1);
  assert.equal(slideBetween('prop', 'add'), -1);
  assert.equal(slideBetween('add', 'piece'), 0, 'the same depth: a swap');
});

test('a deeper level slides in from the right, a shallower one from the left, the same for every document', () => {
  for (const [a, b, c] of [['tile', 'entry', 'control'], ['map', 'piece', 'prop']]) {
    assert.equal(slideBetween(a, b), 1);
    assert.equal(slideBetween(b, c), 1);
    assert.equal(slideBetween(c, b), -1);
    assert.equal(slideBetween(b, a), -1);
    assert.equal(slideBetween(a, c), 1, 'a jump of two levels goes the way of the depth');
    assert.equal(slideBetween(c, a), -1);
    assert.equal(slideBetween(b, b), 0);
  }
});

test('no slide between documents of different kinds, or from no level', () => {
  assert.equal(slideBetween('tile', 'piece'), 0);
  assert.equal(slideBetween('prop', 'entry'), 0);
  assert.equal(slideBetween(null, 'tile'), 0);
  assert.equal(slideBetween('tile', 'nope'), 0);
});

test('every level of the table has its section on the page, and the page has no other', async () => {
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const sections = [...html.matchAll(/<section class="lvl" id="lvl-([a-z]+)"/g)].map((m) => m[1]).sort();
  assert.deepEqual(sections, Object.keys(LEVELS).sort());
});

test('no level view renders its own level switching: the shell shows, hides and slides every level', () => {
  const dir = new URL('../js/view/', import.meta.url);
  // a view showing or hiding itself, animating, or asking which level is on screen
  const own = /\.hidden\s*=|\bhidden=\$\{[^}]*level|\.animate\(|getAnimations|translateX|\bm\.level\b|\blevel\.value\b|\bmapLevel\b|slideBetween|data-depth|dataset\.depth/;
  const seen = [];
  for (const f of readdirSync(dir)) {
    readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line, i) => { if (own.test(line) && !/^\s*(\/\/|\*|\/\*)/.test(line)) seen.push(`${f}:${i + 1}: ${line.trim()}`); });
  }
  assert.deepEqual(seen, []);
});

test('folders, a document and its levels are steps of one trail, by one direction rule', () => {
  const lvl = (root, dir = '') => ({ root, dir });
  assert.equal(folderDepth(lvl(null)), 0);
  assert.equal(folderDepth(lvl(1)), 1);
  assert.equal(folderDepth(lvl(1, 'dungeon')), 2);
  assert.equal(folderDepth(lvl(1, 'dungeon/rooms')), 3);
  // a folder to a subfolder: deeper; back: shallower; across a sibling: a swap
  assert.equal(slideDir(folderDepth(lvl(1)), folderDepth(lvl(1, 'a'))), 1);
  assert.equal(slideDir(folderDepth(lvl(1, 'a')), folderDepth(lvl(1))), -1);
  assert.equal(slideDir(folderDepth(lvl(1, 'a')), folderDepth(lvl(1, 'b'))), 0);
  // a tile opened from its folder is one deeper, and Up from it one shallower, at every depth of folder and every level of the document
  for (const folder of [lvl(1), lvl(1, 'a'), lvl(2, 'a/b')]) {
    assert.equal(slideDir(folderDepth(folder), trailDepth(folder)), 1);
    assert.equal(slideDir(trailDepth(folder), folderDepth(folder)), -1);
    assert.equal(slideDir(trailDepth(folder, LEVELS.control.depth), folderDepth(folder)), -1, 'a crumb from a control to the folder');
    assert.equal(slideDir(trailDepth(folder, LEVELS.piece.depth), trailDepth(folder, LEVELS.map.depth)), -1);
  }
  // the shell's own table agrees with the rule
  assert.equal(slideBetween('tile', 'entry'), slideDir(trailDepth(lvl(1)), trailDepth(lvl(1), 1)));
});

test('one function animates a level change: nothing else animates, translates or reads the motion setting', () => {
  const seen = [];
  const walk = (base) => {
    const dir = new URL(base, import.meta.url);
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      if (f.isDirectory()) { if (!['vendor', 'viewer'].includes(f.name)) walk(`${base}${f.name}/`); continue; }
      if (!f.name.endsWith('.js') || (base === '../js/' && f.name === 'slide.js')) continue;
      readFileSync(new URL(f.name, dir), 'utf8').split('\n').forEach((line, i) => {
        if (/\.animate\(\s*\[|new Animation\b|getAnimations|translateX|prefers-reduced-motion|SLIDE_MS|cubic-bezier/.test(line) && !/^\s*(\/\/|\*|\/\*)/.test(line)) seen.push(`${base}${f.name}:${i + 1}: ${line.trim()}`);
      });
    }
  };
  walk('../js/');
  assert.deepEqual(seen, []);
  // and the stylesheet gives the pages no slide of its own
  const css = readFileSync(new URL('../style.css', import.meta.url), 'utf8');
  assert.doesNotMatch(css, /#track[^{]*\{[^}]*(transition|translateX)/);
  assert.doesNotMatch(css, /#side\.editing #track/);
});

test('only the shell and the model name the levels\' sections', () => {
  const seen = [];
  for (const base of ['../js/', '../js/editor/', '../js/view/']) {
    const dir = new URL(base, import.meta.url);
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.js') || ['levels.js', 'nav.js'].includes(f)) continue;
      readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line, i) => { if (/lvl-(tile|entry|control|map|piece|prop|add)\b/.test(line) && !/^\s*(\/\/|\*)/.test(line)) seen.push(`${base}${f}:${i + 1}: ${line.trim()}`); });
    }
  }
  assert.deepEqual(seen, []);
});
