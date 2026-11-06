import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';
import { browserHost } from '../js/host.js';
import { parseValue, valueOf } from '../js/lens.js';
import { NO_ENTRY, ownerVertices, parsePayload, worstFlagged } from '../js/payload.js';
import { snap } from '../js/place.js';
import { indentSelection, newlineAt } from '../js/text.js';
import { rememberView, shortcutViews } from '../js/view-presets.js';

const elements = [
  {
    name: 'rock', role: 'thing', about: 'one rock',
    controls: [
      { name: 'size', about: 'its size', unit: 'mm' },
      { name: 'shape', about: 'its shape', words: [{ word: 'round', about: 'rounded' }, { word: 'sharp', about: 'angular' }] },
    ],
  },
  { name: 'grass', role: 'plant', about: 'blades of grass', controls: [] },
];

test('view shortcuts keep ISO and Map plus two recent alternatives', () => {
  let recent = ['low', 'close'];
  assert.deepEqual(shortcutViews(recent), ['painted', 'top', 'low', 'close']);
  recent = rememberView(recent, 'front');
  assert.deepEqual(shortcutViews(recent), ['painted', 'top', 'front', 'low']);
  recent = rememberView(recent, 'right');
  assert.deepEqual(shortcutViews(recent), ['painted', 'top', 'right', 'front']);
  assert.deepEqual(rememberView(recent, 'top'), recent);
});

function caret(text, marker = '|') {
  const index = text.indexOf(marker);
  const clean = text.replace(marker, '');
  return { text: clean, caret: { index, before: clean.slice(0, index) } };
}

test('completion offers element names at a node', () => {
  const c = caret('tile a { stack { ro| } }');
  const got = suggest(c.caret, c.text, elements);
  assert.deepEqual(got.items.map((x) => x.label), ['rock']);
  assert.equal(got.items[0].insert, 'rock');
});

test('completion offers controls but not ones already written', () => {
  const c = caret('tile a { stack { rock size=8 sh| } }');
  const got = suggest(c.caret, c.text, elements);
  assert.deepEqual(got.items.map((x) => x.label), ['shape']);
  assert.equal(got.items[0].insert, 'shape=');
});

test('completion knows regions and line edges', () => {
  let c = caret('tile a { region bank { fill "0.5,0.5" }; stack { grass in=ba| } }');
  assert.deepEqual(suggest(c.caret, c.text, elements).items.map((x) => x.label), ['bank']);
  c = caret('tile a { stack { rock along=no| } }');
  assert.deepEqual(suggest(c.caret, c.text, elements).items.map((x) => x.label), ['north']);
});

test('completion stays out of strings and comments', () => {
  let c = caret('tile a { about "ro|" }');
  assert.equal(suggest(c.caret, c.text, elements), null);
  c = caret('tile a { // ro|\n stack {} }');
  assert.equal(suggest(c.caret, c.text, elements), null);
});

test('a comment on an earlier line does not disable completion', () => {
  const c = caret('// a useful note\ntile a { stack { ro| } }');
  assert.deepEqual(suggest(c.caret, c.text, elements).items.map((x) => x.label), ['rock']);
});

test('points snap to named corners and edge middles', () => {
  assert.deepEqual(snap([0.03, 0.04], [2, 1]), { q: [0, 0], word: 'south-west' });
  assert.deepEqual(snap([1.01, 0.98], [2, 1]), { q: [1, 1], word: 'north' });
  assert.deepEqual(snap([1.7, 0.03], [2, 1]), { q: [1.7, 0], word: null });
});

test('lens values preserve their exact written value', () => {
  assert.deepEqual(valueOf('size="6..10"', 'size'), { at: 5, text: '"6..10"' });
  assert.deepEqual(valueOf('"6..10"', 'size'), { at: 0, text: '"6..10"' });
  assert.deepEqual(valueOf('size "6..10"', 'size'), { at: 5, text: '"6..10"' });
  assert.equal(parseValue('7.5'), 7.5);
  assert.equal(parseValue('#true'), true);
  assert.equal(parseValue('"mossy"'), 'mossy');
});

test('the text editor indents selections and carries block indentation', () => {
  let r = indentSelection('a\nb', 0, 3, false);
  assert.deepEqual(r, { text: '    a\n    b', start: 4, end: 11 });
  r = indentSelection(r.text, r.start, r.end, true);
  assert.equal(r.text, 'a\nb');
  assert.deepEqual(newlineAt('    stack {', 11, 11), { text: '    stack {\n        ', caret: 20 });
  assert.deepEqual(newlineAt('    dirt', 8, 8), { text: '    dirt\n    ', caret: 13 });
});

function payload(traced = true) {
  let json = '{"triangles":1,"lo":[0,0,0],"hi":[1,1,0]}';
  while ((8 + new TextEncoder().encode(json).length) % 4) json += ' ';
  const meta = new TextEncoder().encode(json);
  const bytes = 8 + meta.length + 36 + 36 + 9 + 9 + (traced ? 4 : 0);
  const buffer = new ArrayBuffer(bytes + ((4 - bytes % 4) % 4));
  const u8 = new Uint8Array(buffer);
  u8.set([68, 84, 77, 49]);
  new DataView(buffer).setUint32(4, meta.length, true);
  u8.set(meta, 8);
  if (traced) {
    const at = 8 + meta.length + 36 + 36 + 9 + 9;
    new DataView(buffer).setUint16(at, 3, true);
    new DataView(buffer).setUint16(at + 2, 7, true);
  }
  return buffer;
}

test('mesh payload exposes triangle owners without copying arrays', () => {
  const got = parsePayload(payload(true));
  assert.equal(got.triangles, 1);
  assert.equal(got.entry[0], 3);
  assert.equal(got.piece[0], 7);
  assert.equal(got.positions.buffer, got.entry.buffer);
});

test('an old untraced mesh stays readable', () => {
  const got = parsePayload(payload(false));
  assert.equal(got.entry[0], NO_ENTRY);
  assert.equal(got.piece[0], 0);
});

test('GPU ownership is compact, repeated per corner and reserves zero for none', () => {
  const who = ownerVertices(new Uint16Array([2, NO_ENTRY]), new Uint16Array([7, 0]));
  assert.ok(who instanceof Uint16Array);
  assert.deepEqual([...who], [3, 7, 3, 7, 3, 7, 0, 0, 0, 0, 0, 0]);
});

test('the gate points to the largest red face before any orange face', () => {
  const p = {
    triangles: 3,
    positions: new Float32Array([
      0,0,0, 4,0,0, 0,4,0,
      0,0,1, 1,0,1, 0,1,1,
      0,0,2, 2,0,2, 0,2,2,
    ]),
    clean: new Uint8Array(27).fill(100),
    painted: new Uint8Array(27).fill(100),
    entry: new Uint16Array([1, 2, 3]),
    piece: new Uint16Array([0, 4, 0]),
  };
  p.painted.set([255, 140, 26], 0); // large orange
  p.painted.set([230, 30, 30], 9);  // small red
  p.painted.set([230, 30, 30], 18); // larger red: the answer
  const got = worstFlagged(p);
  assert.equal(got.kind, 'red');
  assert.equal(got.triangle, 2);
  assert.equal(got.entry, 3);
  assert.deepEqual(got.lo, [0, 0, 2]);
  assert.deepEqual(got.hi, [2, 2, 2]);
});

test('a foreign mesh fails clearly', () => {
  assert.throws(() => parsePayload(new ArrayBuffer(8)), /not a dharmatiles mesh payload/);
});

test('a disconnected edit reports a recoverable result instead of rejecting', async () => {
  const original = globalThis.fetch;
  globalThis.fetch = async () => { throw new Error('offline'); };
  try {
    assert.deepEqual(await browserHost().post('/api/edit', {}), {
      status: 0,
      body: { error: 'the editor is disconnected' },
    });
  } finally {
    globalThis.fetch = original;
  }
});
