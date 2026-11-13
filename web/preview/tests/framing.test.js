// The viewer owns its camera: it decides from a description and what it holds, with one memory of the user's
// last view per document and mode (js/viewer/framing.js).  Each behaviour the page keeps is a test here; the
// browser probe (cam.mjs) checks the same on the real camera.

import test from 'node:test';
import assert from 'node:assert/strict';
import { createFraming } from '../js/viewer/framing.js';

const box = (w, h, z = 8) => ({ lo: [0, 0, 0], hi: [w, h, z] });
const desc = (over = {}) => ({ key: 'tile:a', kind: 'tile', view: 'painted', asked: null, fresh: false, ...over });
const cam = (n) => ({ p: [n, 0, 0], t: [0, 0, 0] });

test('a document shown for the first time is framed at once on the preset', () => {
  const f = createFraming();
  assert.deepEqual(f.step({ desc: desc(), box: box(35, 35), finished: true, camera: () => cam(1) }), { do: 'frame', view: 'painted', box: box(35, 35), animate: false });
});

test('a map opens framed on its pieces, never on its printer\'s bed', () => {
  const f = createFraming();
  const pieces = { lo: [0, -70, -7], hi: [105, 0, 22] };
  const a = f.step({ desc: desc({ key: 'map:m', kind: 'map' }), box: pieces, finished: true, camera: () => cam(1) });
  assert.deepEqual(a.box, pieces);
  // the plan the viewer holds (a print list's is the bed) is not in a map's step at all: the box is the description's
  const later = f.step({ desc: desc({ key: 'map:m', kind: 'map' }), box: pieces, finished: true, camera: () => cam(1) });
  assert.equal(later.do, 'keep');
});

test('a named view frames again when pieces arrive, even mid-flight, and lands on the new frame', () => {
  const f = createFraming();
  const d = desc({ key: 'map:m', kind: 'map' });
  f.step({ desc: d, box: box(70, 70), finished: true, camera: () => cam(1) });
  const asked = { name: 'low', animate: true };
  const ask = desc({ key: 'map:m', kind: 'map', view: 'low', asked });
  assert.equal(f.step({ desc: ask, box: box(70, 70), finished: true, camera: () => cam(1) }).animate, true);
  // a piece arrives while the flight is on
  const a = f.step({ desc: ask, box: box(140, 70), finished: true, flying: true, camera: () => cam(1) });
  assert.deepEqual(a, { do: 'frame', view: 'low', box: box(140, 70), animate: true });
  // and a resize, flying or not
  assert.equal(f.step({ desc: ask, box: box(140, 70), finished: true, flying: false, resized: true, camera: () => cam(1) }).animate, false);
});

test('a view the user moved stays through rebuilds, edits and pieces; asking for a preset gives the camera back', () => {
  const f = createFraming();
  const d = desc({ key: 'map:m', kind: 'map' });
  f.step({ desc: d, box: box(70, 70), finished: true, camera: () => cam(1) });
  assert.equal(f.state().held, false);
  f.hold();
  assert.equal(f.state().held, true);
  for (const b of [box(140, 70), box(140, 140)]) assert.equal(f.step({ desc: d, box: b, finished: true, camera: () => cam(2) }).do, 'keep');
  assert.equal(f.step({ desc: d, box: box(140, 140), finished: true, resized: true, camera: () => cam(2) }).do, 'keep');
  const tile = desc({ fresh: true });
  const g = createFraming();
  g.step({ desc: tile, box: box(35, 35), finished: false, camera: () => cam(1) });
  g.hold();
  assert.equal(g.step({ desc: tile, box: box(70, 50), finished: false, camera: () => cam(2) }).do, 'keep');
  assert.equal(g.step({ desc: tile, box: box(35, 35), finished: true, camera: () => cam(2) }).do, 'keep');
  const asked = { name: 'top', animate: true };
  assert.equal(f.step({ desc: { ...d, view: 'top', asked }, box: box(140, 140), finished: true, camera: () => cam(2) }).do, 'frame');
  assert.equal(f.state().held, false);
});

test('the plates show the bed: the box is the viewer\'s, and a change of it frames again', () => {
  const f = createFraming();
  const d = desc({ key: 'plates:m', kind: 'plates' });
  f.step({ desc: d, box: box(35, 35, 1), finished: false, camera: () => cam(1) });
  const bed = { lo: [0, 0, 0], hi: [180, 180, 12] };
  assert.deepEqual(f.step({ desc: d, box: bed, finished: true, camera: () => cam(1) }), { do: 'frame', view: 'painted', box: bed, animate: false });
  assert.equal(f.step({ desc: d, box: bed, finished: true, camera: () => cam(1) }).do, 'keep');
});

test('a tile being built widens to take in each part and settles on the finished tile', () => {
  const f = createFraming();
  const d = desc({ fresh: true });
  f.step({ desc: d, box: box(35, 35), finished: false, camera: () => cam(1) });
  // a part far beyond the footprint
  assert.deepEqual(f.step({ desc: d, box: { lo: [0, 0, 0], hi: [35, 35, 30] }, finished: false, camera: () => cam(1) }), { do: 'frame', view: 'painted', box: { lo: [0, 0, 0], hi: [35, 35, 30] }, animate: true });
  // a part that changes nothing worth a move
  assert.equal(f.step({ desc: d, box: { lo: [0, 0, 0], hi: [35, 35, 30.5] }, finished: false, camera: () => cam(1) }).do, 'keep');
  // the finished tile
  const fin = { lo: [0, 0, 0], hi: [35, 35, 12] };
  assert.deepEqual(f.step({ desc: d, box: fin, finished: true, camera: () => cam(1) }), { do: 'frame', view: 'painted', box: fin, animate: true });
  // a rebuild's new mesh leaves the camera alone
  assert.equal(f.step({ desc: d, box: { lo: [0, 0, 0], hi: [70, 35, 12] }, finished: true, camera: () => cam(1) }).do, 'keep');
});

test('a tile that already has its mesh is framed at once, with no flight', () => {
  const f = createFraming();
  const d = desc({ fresh: false });
  f.step({ desc: d, box: box(35, 35), finished: false, camera: () => cam(1) });
  assert.equal(f.step({ desc: d, box: box(35, 35, 11), finished: true, camera: () => cam(1) }).animate, false);
});

test('going back to a document restores the view the user left it on, and only that', () => {
  const f = createFraming();
  const a = desc({ key: 'tile:a' }), b = desc({ key: 'tile:b' });
  f.step({ desc: a, box: box(35, 35), finished: true, camera: () => cam(1) });
  f.hold();
  f.step({ desc: b, box: box(35, 35), finished: true, camera: () => cam(7) }); // leaving a: its camera is saved
  assert.deepEqual(f.savedKeys(), ['tile:a']);
  assert.equal(f.state().held, false, 'b was never moved');
  const back = f.step({ desc: a, box: box(35, 35), finished: true, camera: () => cam(9) }); // b was not moved: nothing saved for it
  assert.deepEqual(back, { do: 'restore', camera: cam(7) });
  assert.equal(f.state().held, true);
  assert.deepEqual(f.savedKeys(), ['tile:a']);
});

test('nothing outside the viewer moves its camera: the page describes what it shows and the viewer decides', async () => {
  const { readdirSync, readFileSync } = await import('node:fs');
  const dir = new URL('../js/editor/', import.meta.url);
  const moving = /viewer\.(setView|setCamera|restoreCamera|getCamera|place|placeEquivalent|flyToPose|reframe|setSceneBox)\b|viewer\.controls\.target\.(set|copy)|viewer\.camera\.position\.(set|copy)/;
  const seen = [];
  for (const f of readdirSync(dir)) {
    readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line, i) => { if (moving.test(line) && !/^\s*\/\//.test(line)) seen.push(`${f}:${i + 1}: ${line.trim()}`); });
  }
  assert.deepEqual(seen, []);
});

test('the same document in another mode has its own saved view', () => {
  const f = createFraming();
  f.step({ desc: desc({ key: 'map:m', kind: 'map' }), box: box(70, 70), finished: true, camera: () => cam(1) });
  f.hold();
  f.step({ desc: desc({ key: 'plates:m', kind: 'plates' }), box: box(180, 180), finished: true, camera: () => cam(3) });
  assert.equal(f.step({ desc: desc({ key: 'map:m', kind: 'map' }), box: box(70, 70), finished: true, camera: () => cam(4) }).do, 'restore');
});
