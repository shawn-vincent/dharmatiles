// The folder levels are a function of the model: Places lists the places, a place lists its folders and tiles, a row's ⋯ and keys
// say what was asked, and a level that changes is drawn by itself.
import test from 'node:test';
import assert from 'node:assert/strict';
import { signal } from '@preact/signals-core';
import { install, settle } from './dom.js';

globalThis.matchMedia = () => ({ matches: true }); // reduced motion: no slide, no ghost
const root = install();
const { render, html } = await import('../js/view/html.js');
const { Levels } = await import('../js/view/places-panel.js');
const { levelOf, placesOf } = await import('../js/folders.js');

const roots = [{ label: 'My tiles', role: 'home', kind: 'dir', path: '/h', writable: true, exists: true }, { label: 'Library', role: 'library', kind: 'lib', path: 'lib:tiles/', writable: false, exists: true }];
const tiles = [
  { id: 'a', root: 0, path: '/h/glade.dtile', status: 'ready', size: '1x1' },
  { id: 'b', root: 0, path: '/h/dungeon/hall.dtile', status: 'ready', size: '2x1' },
];
const dataFor = (level) => ({
  roots, tiles, folders: [], level, query: '', places: placesOf(roots), openTile: null, home: '/h', pill: () => null, system: 'db', inventory: null,
  view: level.root == null ? null : levelOf({ roots, tiles, folders: [], root: level.root, dir: level.dir, query: '' }), writable: level.root === 0,
});

function world() {
  const level = signal({ root: null, dir: '' });
  const calls = [];
  const on = new Proxy({ canDrop: () => ({ ok: false }) }, { get: (t, k) => t[k] ?? ((...a) => calls.push([k, ...a.filter((x) => typeof x !== 'object')])) });
  const scrolls = new Map();
  const App = () => html`<${Levels} d=${dataFor(level.value)} on=${on} scrolls=${scrolls} />`;
  render(html`<${App} />`, root);
  return { level, calls };
}

test('Places lists the places, a place lists its folders then its tiles, and the level follows the model', async () => {
  const { level, calls } = world();
  await settle();
  assert.equal(root.querySelectorAll('.orow').length, 2);
  assert.match(root.textContent, /Your places/);
  assert.match(root.textContent, /Included/);
  root.querySelectorAll('.orow')[0].dispatchEvent({ type: 'click' });
  assert.deepEqual(calls.at(-1), ['place', 0]);
  level.value = { root: 0, dir: '' };
  await settle();
  assert.match(root.textContent, /Folders/);
  assert.match(root.textContent, /dungeon/);
  assert.match(root.textContent, /Tiles here/);
  assert.equal(root.querySelectorAll('.orow').length, 2, 'the folder and the tile');
  assert.equal(root.querySelectorAll('.omore').length, 2, 'each has a menu');
  const folder = root.querySelectorAll('.orwrap')[0];
  assert.equal(folder.getAttribute('data-drop-dir'), 'dungeon', 'a folder row is a place a tile can be let go');
});

test('a row\'s ⋯ and its keys say what was asked of which row', async () => {
  const { level, calls } = world();
  level.value = { root: 0, dir: '' };
  await settle();
  const tileRow = root.querySelectorAll('.orow').find((b) => b.dataset.kind === 'tile');
  tileRow.parentNode.querySelector('.omore').dispatchEvent({ type: 'click', stopPropagation() {} });
  assert.deepEqual(calls.at(-1), ['menu', 'tile', 'a']);
  tileRow.dispatchEvent({ type: 'keydown', key: 'F2', preventDefault() {}, stopPropagation() {}, target: tileRow });
  assert.deepEqual(calls.at(-1), ['act', 'rename', 'tile', 'a']);
});
