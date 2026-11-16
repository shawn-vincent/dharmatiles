// The thing view's facts (js/thing-model.js): which thing the cursor or the selection is on, the strip's seeds, and the words for a size.
import test from 'node:test';
import assert from 'node:assert/strict';
import { STRIP, boxText, bytesOf, extentText, frameOf, num, stripSeeds, targetOf } from '../js/thing-model.js';

const nodes = [
  { id: 'define bluff', what: 'define', name: 'bluff', at: 29, len: 68 },
  { id: 'define wet', what: 'define', name: 'wet', at: 100, len: 40 },
  { id: 'tile', what: 'tile', name: null, at: 150, len: 200 },
];
const rock = { key: 'tile/rock/0', label: 'rock', element: 'rock', things: [] };
const scatter = { key: 'tile/scatter/0', label: 'pebbles and rocks', element: 'pebble', things: [{ what: 'pebble' }, { what: 'rock' }] };
/** The page's state where nothing is selected and the cursor is at the start. */
const base = { thingFile: true, face: /** @type {'tile' | 'source'} */ ('tile'), item: null, generator: null, child: 0, nodes, things: ['bluff'], caret: 0 };

test('the cursor in a define that is one thing is that thing; in a layer define, or outside every define, it is none', () => {
  const at = (/** @type {number} */ caret) => targetOf({ ...base, face: 'source', caret });
  assert.deepEqual(at(40)?.which, { kind: 'define', name: 'bluff' });
  assert.equal(at(40)?.name, 'bluff');
  assert.equal(at(110), null, '`wet` is a layer: not in the things');
  assert.equal(at(160), null, 'the tile is no define');
  assert.equal(at(5), null);
  // the ends of the node are in it
  assert.ok(at(29) && at(97));
});

test('a selected entry that is a thing is that thing, by its key; a scatter offers its things, one at a time', () => {
  const one = targetOf({ ...base, item: rock, generator: 'thing' });
  assert.deepEqual(one?.which, { kind: 'entry', key: 'tile/rock/0', child: null });
  assert.equal(one?.element, 'rock');
  const kid = (/** @type {number} */ child) => targetOf({ ...base, item: scatter, generator: 'layer', child });
  assert.deepEqual(kid(0)?.which, { kind: 'entry', key: 'tile/scatter/0', child: 0 });
  assert.deepEqual(kid(1)?.which, { kind: 'entry', key: 'tile/scatter/0', child: 1 });
  assert.deepEqual(kid(1)?.things, ['pebble', 'rock']);
  assert.equal(kid(9)?.child, 1, 'a place past the last is the last');
  assert.equal(targetOf({ ...base, item: { ...rock, things: [] }, generator: 'layer' }), null, 'a layer with no things is no thing');
});

test('in the source the caret speaks first, in the levels the selection does', () => {
  const both = { ...base, item: rock, generator: 'thing', caret: 40 };
  assert.equal(targetOf({ ...both, face: 'source' })?.which.kind, 'define');
  assert.equal(targetOf({ ...both, face: 'tile' })?.which.kind, 'entry');
  // with nothing under the caret the source falls back to the selection, and the levels to the caret
  assert.equal(targetOf({ ...both, face: 'source', caret: 5 })?.which.kind, 'entry');
  assert.equal(targetOf({ ...base, face: 'tile', caret: 40 })?.which.kind, 'define');
});

test('a print list, a map or the inventory has no thing', () => {
  assert.equal(targetOf({ ...base, thingFile: false, item: rock, generator: 'thing', face: 'source', caret: 40 }), null);
});

test('the strip is twelve seeds a page, the first page seeds 1 to 12', () => {
  assert.equal(STRIP, 12);
  assert.deepEqual(stripSeeds(0), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  assert.deepEqual(stripSeeds(2).slice(0, 2), [25, 26]);
});

test('a size is printed mm with the dmm the document writes beside it, never the one as the other', () => {
  assert.equal(num(6.04), '6');
  assert.equal(num(20.449), '20.4');
  assert.equal(boxText({ x: 6, y: 6, z: 2.5 }), '6 × 6 × 2.5');
  const t = extentText({ x: 7, y: 7, z: 3 }, { x: 6, y: 6, z: 2.6 });
  assert.equal(t.printed, '7 × 7 × 3 mm');
  assert.equal(t.written, '6 × 6 × 2.6 dmm');
});

test('the frame is the largest box round the origin, standing on zero, so a new seed does not move the camera', () => {
  assert.deepEqual(frameOf({ x: 20, y: 10, z: 9 }), { lo: [-10, -5, 0], hi: [10, 5, 9] });
});

test('a mesh comes over as base64 and is read back as the bytes it was', () => {
  const buf = bytesOf(btoa('DTM2'));
  assert.equal(new TextDecoder().decode(new Uint8Array(buf)), 'DTM2');
});
