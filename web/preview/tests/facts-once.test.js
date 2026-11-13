// Each fact is printed once, where its owner says it (the title bar says which document and whether it is saved, the panel
// says what is in it, the status line says what the build is doing); the interface says "draft" for a low-detail mesh and
// "Close-up" for the close camera; one segmented rule fills a wide track.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { contextFor } from '../js/nav.js';
import { VIEWS } from '../js/viewer/consts.js';

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8');

test('the title bar and the status line carry no second copy of the size, the totals or the target', () => {
  assert.doesNotMatch(read('../index.html'), /id="tileplan"|id="sb-target"/);
  const lines = [...contextFor({ depth: 0, tile: { entries: 2 } }), ...contextFor({ depth: 1, tile: {}, entry: { note: 'line a' } })];
  for (const line of lines) assert.doesNotMatch(line, /DungeonBlocks|OpenLOCK|seed|×/);
});

test('no floating "more below" pill and no Rename strip: the scroll shadow says it, the crumb renames', () => {
  assert.doesNotMatch(read('../style.css'), /more below/);
  assert.doesNotMatch(read('../js/editor/sheet.js'), /label: 'Rename'/);
  assert.match(read('../js/editor/tiles.js'), /tilebtn'\)\.ondblclick/);
});

test('a low-detail mesh is a draft and the close camera is Close-up, in what the user reads', () => {
  for (const f of ['../js/messages.js', '../js/editor/scene.js', '../js/view/plates-panel.js']) {
    assert.doesNotMatch(read(f), /low · not yet|low meshes|\$\{counts\.low\} low/, f);
  }
  assert.equal(VIEWS.find((v) => v[0] === 'close')[1], 'Close-up');
});

test('one segmented rule: the buttons of a track share its room, and no id re-states it', () => {
  assert.match(read('../../../docs/assets/theme.css'), /\.seg > button \{[^}]*flex: 1 1 auto/);
  assert.doesNotMatch(read('../style.css'), /\.seg > button \{[^}]*\bflex:/);
});
