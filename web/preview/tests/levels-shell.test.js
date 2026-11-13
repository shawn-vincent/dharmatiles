// The levels shell is one mechanism for every document's levels (editor/levels.js): which level shows and which way it slides is
// nav.js, and no view draws its own level switching.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { ADD_PANES, LEVELS, crumbsFor, folderDepth, levelDepth, mapLevelOf, slideBetween, slideDir, tileLevelOf, trailDepth } from '../js/nav.js';

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
  assert.equal(mapLevelOf({ pieces: 0, add: 0 }), 'add');
  assert.equal(mapLevelOf({ pieces: 0, add: 1 }), 'add2', "the Add step's folders take its two sections in turn");
  assert.equal(mapLevelOf({ pieces: 2, add: 0 }), 'piece', 'a piece selected leaves Add');
  assert.deepEqual(ADD_PANES, ['add', 'add2']);
});

test('the Add step is one step deeper than the map, and each folder it walks one deeper again', () => {
  const at = (root, dir = '') => ({ root, dir });
  assert.equal(levelDepth('add', at(null)), 1, 'Places of the Add step is the step itself');
  assert.equal(levelDepth('add', at(0)), 2);
  assert.equal(levelDepth('add2', at(0, 'dungeon')), 3);
  assert.equal(levelDepth('add', at(0, 'dungeon/rooms')), 4);
  assert.equal(levelDepth('map'), 0);
  assert.equal(levelDepth('piece'), 1);
  assert.equal(slideBetween('map', 'add'), 1);
  assert.equal(slideBetween('add', 'map'), -1);
  assert.equal(slideBetween('map', { id: 'add2', folder: at(0) }), 1);
  assert.equal(slideBetween('add', 'piece'), 0, 'the same depth: a swap');
  // into a folder: deeper; back up: shallower; across to a sibling folder: a swap; a jump back several folders: the way of the depth
  assert.equal(slideBetween({ id: 'add', folder: at(0) }, { id: 'add2', folder: at(0, 'dungeon') }), 1);
  assert.equal(slideBetween({ id: 'add2', folder: at(0, 'dungeon') }, { id: 'add', folder: at(0) }), -1);
  assert.equal(slideBetween({ id: 'add', folder: at(0, 'a') }, { id: 'add2', folder: at(0, 'b') }), 0);
  assert.equal(slideBetween({ id: 'add', folder: at(0, 'a/b') }, { id: 'add2', folder: at(null) }), -1);
  // the same rule as a folder of the browser: one deeper than the one above it
  assert.equal(slideDir(levelDepth('add', at(0)), levelDepth('add2', at(0, 'dungeon'))), slideDir(folderDepth(at(0)), folderDepth(at(0, 'dungeon'))));
});

test('the trail through the Add step: every folder a crumb, Add and Places first, each at its own depth', () => {
  const list = crumbsFor({ place: { root: 0, label: 'My tiles', dir: '' }, tileName: 'first-plate.dmap', add: { root: 0, label: 'My tiles', dir: 'dungeon/rooms' } });
  assert.deepEqual(list.map((c) => c.label), ['Places', 'My tiles', 'first-plate.dmap', 'Add', 'Places', 'My tiles', 'dungeon', 'rooms']);
  assert.deepEqual(list.filter((c) => c.add).map((c) => [c.label, c.add.root, c.add.dir, c.depth]), [
    ['Add', null, '', 1], ['Places', null, '', 1], ['My tiles', 0, '', 2], ['dungeon', 0, 'dungeon', 3], ['rooms', 0, 'dungeon/rooms', 4],
  ]);
  assert.deepEqual(list.filter((c) => c.current).map((c) => c.label), ['rooms']);
  // at Places of the Add step
  const top = crumbsFor({ place: null, tileName: 'a.dmap', add: null });
  assert.deepEqual(top.map((c) => c.label), ['Places', 'a.dmap', 'Add', 'Places']);
  assert.equal(top.at(-1).current, true);
  // a map without the Add step has no such crumbs
  assert.deepEqual(crumbsFor({ place: null, tileName: 'a.dmap' }).map((c) => c.label), ['Places', 'a.dmap']);
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
  const sections = [...html.matchAll(/<section class="lvl" id="lvl-([a-z0-9]+)"/g)].map((m) => m[1]).sort();
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
      readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line, i) => { if (/lvl-(tile|entry|control|map|piece|prop|add2?)\b/.test(line) && !/^\s*(\/\/|\*)/.test(line)) seen.push(`${base}${f}:${i + 1}: ${line.trim()}`); });
    }
  }
  assert.deepEqual(seen, []);
});

test('the Add step draws no navigation of its own: no crumb bar, no up button, no folder slide, no depth', () => {
  // its trail is the panel's (editor/levels.js, the crumbs over the panel), its ‹ is the panel's, its folders are levels of the shell
  const src = readFileSync(new URL('../js/view/map-panel.js', import.meta.url), 'utf8');
  const own = /crumbbar|class="up"|\bCrumbs\b|DeepCrumbs|crumbsFor|\bLevels\b|parentLevel|folderDepth|levelDepth|\bghost\b|\bscrolls\b|addUp|addScrolls|aria-label="Up a/;
  const seen = [];
  src.split('\n').forEach((line, i) => { if (own.test(line) && !/^\s*(\/\/|\*|\/\*)/.test(line)) seen.push(`map-panel.js:${i + 1}: ${line.trim()}`); });
  assert.deepEqual(seen, []);
  // `Levels` (the browser's sliding ghost) is drawn by the browser alone
  const users = [];
  for (const base of ['../js/', '../js/editor/', '../js/view/']) {
    const dir = new URL(base, import.meta.url);
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.js')) continue;
      readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line) => { if (/<\$\{Levels\}/.test(line)) users.push(`${base}${f}`); });
    }
  }
  assert.deepEqual(users, ['../js/editor/places.js']);
  // and the stylesheet has no bar for it
  const css = readFileSync(new URL('../style.css', import.meta.url), 'utf8');
  assert.doesNotMatch(css, /\.add-head/);
});
