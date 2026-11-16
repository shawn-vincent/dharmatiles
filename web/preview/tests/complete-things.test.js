// Completion of a thing parameter's value: the thing elements and the document's things, never a layer.

import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';

const elements = [
  { name: 'rock', about: 'one rock', role: 'thing', generator: 'thing', family: 'rock', controls: [] },
  { name: 'pebble', about: 'a small rock', role: 'thing', generator: 'thing', family: 'rock', controls: [] },
  { name: 'dirt', about: 'earth', role: 'surface', generator: 'layer', family: 'surface', controls: [{ name: 'top' }] },
];
const names = {
  defs: ['stand', 'boulder-ish'],
  things: ['boulder-ish'],
  thingParams: [{ define: 'stand', param: 'piece' }],
};
function caret(text) {
  const index = text.indexOf('|');
  const clean = text.replace('|', '');
  return { index, before: clean.slice(0, index) };
}

test('a thing parameter takes the things, and no layer', () => {
  const s = suggest(caret('tile { stack { stand piece=|'), elements, true, null, names);
  assert.deepEqual(s.items.map((i) => i.label).sort(), ['boulder-ish', 'pebble', 'rock']);
  assert.ok(s.items.every((i) => i.kind === 'thing' && i.about.startsWith('a thing')));
  const p = suggest(caret('tile { stack { stand piece=pe|'), elements, true, null, names);
  assert.deepEqual(p.items.map((i) => i.label), ['pebble']);
});

test('a parameter that holds no thing is offered none', () => {
  assert.equal(suggest(caret('tile { stack { stand other=|'), elements, true, null, names), null);
});
