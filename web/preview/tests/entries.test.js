import test from 'node:test';
import assert from 'node:assert/strict';

import { NO_ENTRY, entryKey, meshEntry, meshIndex, meshStack, piecesByKey, worstFlagged } from '../js/payload.js';

// a mesh built from a stack of three entries: dirt, a scatter, the base
const stack = [
  { index: 0, key: 'tile/dirt/0', label: 'dirt', element: 'dirt', what: 'dirt' },
  { index: 1, key: 'tile/scatter/0', label: 'rocks', element: 'rock', what: 'scatter' },
  { index: 2, key: 'tile/connector', label: 'DungeonBlocks base', element: 'connector', what: 'base' },
];
const mesh = (entry, piece) => ({
  meta: { stack }, triangles: entry.length, entry: new Uint16Array(entry), piece: new Uint16Array(piece),
});

test('an entry number is decoded against the stack the mesh was built from', () => {
  const p = mesh([0, 1, 1, 2, NO_ENTRY], [0, 0, 1, 0, 0]);
  assert.equal(entryKey(p, 0), 'tile/dirt/0');
  assert.equal(entryKey(p, 1), 'tile/scatter/0');
  assert.equal(entryKey(p, 2), 'tile/connector');
  assert.equal(entryKey(p, NO_ENTRY), null);
  assert.equal(entryKey(p, 9), null, 'a number the mesh has no entry for is none, not another entry');
  assert.equal(meshEntry(p, 1).label, 'rocks');
  assert.deepEqual(meshStack(p).map((e) => e.index), [0, 1, 2]);
  // an untraced mesh (a sketch, an STL) has no stack: every entry is none
  assert.equal(entryKey({ meta: {} }, 0), null);
  assert.deepEqual(meshStack(null), []);
});

test('a key is found again in the mesh it is in, and not in one that has not got it', () => {
  const p = mesh([0], [0]);
  assert.equal(meshIndex(p, 'tile/scatter/0'), 1);
  assert.equal(meshIndex(p, 'tile/tree/0'), -1);
  assert.equal(meshIndex(p, null), -1);
});

test('pieces are counted by key', () => {
  const p = mesh([0, 1, 1, 1, 2], [0, 0, 1, 2, 0]);
  assert.deepEqual([...piecesByKey(p)], [['tile/dirt/0', 1], ['tile/scatter/0', 3], ['tile/connector', 1]]);
});

test('the worst face names its entry by key', () => {
  const p = {
    meta: { stack }, triangles: 1,
    positions: new Float32Array([0, 0, 0, 4, 0, 0, 0, 4, 0]),
    clean: new Uint8Array(9).fill(100), painted: new Uint8Array(9).fill(100),
    entry: new Uint16Array([1]), piece: new Uint16Array([2]),
  };
  p.painted.set([230, 30, 30], 0);
  const w = worstFlagged(p);
  assert.equal(w.entry, 1);
  assert.equal(w.key, 'tile/scatter/0');
  assert.equal(w.piece, 2);
});

test('a selection kept by key survives a reorder of the stack, and a mesh older than the text still decodes', async () => {
  const { entrySel, server, view, doc, stackIndex, itemOf, same } = await import('../js/editor/stores.js');
  const item = (key, label) => ({ key, label, element: key.split('/')[1], what: key.split('/')[1] });
  const before = [item('tile/dirt/0', 'dirt'), item('tile/scatter/0', 'rocks'), item('tile/connector', 'base')];
  server.set({ tiles: [{ id: 't1', path: 'a.dtile', plan: { stack: before } }] });
  view.set({ tileId: 't1', sel: entrySel('tile/scatter/0') });
  doc.set({ tileId: 't1', stack: before });
  assert.equal(stackIndex('tile/scatter/0'), 1);
  assert.equal(itemOf(view.get().sel).label, 'rocks');
  // the text is edited: a water is written above the scatter, so every index after it moves
  const after = [item('tile/dirt/0', 'dirt'), item('tile/water/0', 'water'), item('tile/scatter/0', 'rocks'), item('tile/connector', 'base')];
  doc.set({ stack: after });
  assert.equal(stackIndex('tile/scatter/0'), 2, 'the text now has it one place later');
  assert.equal(itemOf(view.get().sel).label, 'rocks', 'the selection still names the scatter');
  assert.ok(same(view.get().sel, entrySel('tile/scatter/0')));
  // the mesh on screen is the old build's: its number 1 is still the scatter, whatever the text says now
  const old = mesh([1], [0]);
  assert.equal(entryKey(old, old.entry[0]), view.get().sel.id);
  assert.equal(meshIndex(old, view.get().sel.id), 1, 'tinted at the old mesh\'s place');
  // an entry the text no longer has is not found (the selection is dropped by the page when no mesh has it either)
  doc.set({ stack: [item('tile/dirt/0', 'dirt')] });
  assert.equal(stackIndex('tile/scatter/0'), -1);
  assert.equal(itemOf(view.get().sel), undefined);
  assert.equal(meshIndex(old, 'tile/scatter/0'), 1);
});
