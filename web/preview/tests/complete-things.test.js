// Completion of a thing or layer parameter's value: the names that fit the slot, by the program's one fit rule.
// A thing slot offers the things and never a layer; a layer slot offers the layers and the things (placing a thing
// makes it a layer); a slot that holds a number offers none.

import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';

const elements = [
  { name: 'rock', about: 'one rock', role: 'thing', generator: 'thing', family: 'rock', controls: [] },
  { name: 'pebble', about: 'a small rock', role: 'thing', generator: 'thing', family: 'rock', controls: [] },
  { name: 'dirt', about: 'earth', role: 'surface', generator: 'layer', family: 'surface', controls: [{ name: 'top' }] },
];
const names = {
  defs: ['stand', 'bank', 'boulder-ish', 'lawn'],
  kinds: [
    { name: 'stand', kind: 'layer' },
    { name: 'bank', kind: 'layer' },
    { name: 'boulder-ish', kind: 'thing' },
    { name: 'lawn', kind: 'layer' },
  ],
  params: [
    { owner: 'stand', param: 'piece', kind: 'thing' },
    { owner: 'bank', param: 'ground', kind: 'layer' },
    { owner: 'bank', param: 'reach', kind: 'length' },
  ],
};
function caret(text) {
  const index = text.indexOf('|');
  const clean = text.replace('|', '');
  return { index, before: clean.slice(0, index) };
}

test('a thing parameter takes the things, and no layer', () => {
  const s = suggest(caret('tile { stack { stand piece=|'), elements, true, null, names);
  assert.deepEqual(s.items.map((i) => i.label).sort(), ['boulder-ish', 'pebble', 'rock']);
  assert.ok(s.items.every((i) => i.about.startsWith('a thing')));
  const p = suggest(caret('tile { stack { stand piece=pe|'), elements, true, null, names);
  assert.deepEqual(p.items.map((i) => i.label), ['pebble']);
});

test('a layer parameter takes the layers and the things', () => {
  const s = suggest(caret('tile { stack { bank ground=|'), elements, true, null, names);
  assert.deepEqual(s.items.map((i) => i.label).sort(), ['bank', 'boulder-ish', 'dirt', 'lawn', 'pebble', 'rock', 'stand']);
  assert.ok(s.items.some((i) => i.label === 'dirt' && i.about.startsWith('a layer')));
  assert.ok(s.items.some((i) => i.label === 'rock' && i.about.startsWith('a thing')));
});

test('a parameter that holds a number, or none, is offered no names', () => {
  assert.equal(suggest(caret('tile { stack { stand other=|'), elements, true, null, names), null);
  assert.equal(suggest(caret('tile { stack { bank reach=|'), elements, true, null, names), null);
});
