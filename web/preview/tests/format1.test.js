// The editor in format 1: points and values are written with units, from the
// north-west corner; completion knows the `$` names; the sheet's value forms
// read a unit after a number.

import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';
import { pointLabel } from '../js/points.js';
import { asNumber, asRange, formOf, setUnits } from '../js/value.js';

setUnits(['sq', 'mm', 'in', 'dmm', 'layer', 'deg', 'rad', 'pi']); // what the program's language table says

test('a point is labelled as the document counts it', () => {
  // "(x, y)" with units, y down from the north-west corner
  assert.equal(pointLabel([0.5, 0.25], 2), '(0.5sq, 1.75sq)');
  assert.equal(pointLabel([1 / 3, 1], 1), '(0.33sq, 0sq)');
});

test('typed values with units are numbers and ranges', () => {
  assert.equal(formOf('8dmm'), 'number');
  assert.equal(formOf('0.5sq'), 'number');
  assert.equal(formOf('40deg'), 'number');
  assert.equal(formOf('6dmm .. 10dmm'), 'range');
  assert.equal(formOf('$bottom + 40dmm'), 'expression');
  assert.equal(asRange('8dmm'), '8dmm..8dmm');
  assert.equal(asNumber('6dmm..10dmm'), '6dmm');
});

const elements = [{ name: 'dirt', about: 'earth', role: 'surface', controls: [{ name: 'top' }] }];
const language = { names: ['$x', '$bottom', '$region'], units: ['sq', 'dmm', 'deg'], functions: ['sin', 'point'] };
function caret(text, marker = '|') {
  const index = text.indexOf(marker);
  const clean = text.replace(marker, '');
  return { text: clean, caret: { index, before: clean.slice(0, index) } };
}

test('completion in a format 1 value offers the $ names, functions and units', () => {
  let c = caret('tile { stack { dirt top="$bo|" } }');
  assert.deepEqual(suggest(c.caret, elements, false, language).items.map((x) => x.label), ['$bottom']);
  c = caret('tile { stack { dirt top="si|" } }');
  assert.deepEqual(suggest(c.caret, elements, false, language).items.map((x) => x.label), ['sin']);
  c = caret('tile { stack { dirt top="8d|" } }');
  assert.deepEqual(suggest(c.caret, elements, false, language).items.map((x) => x.label), ['deg', 'dmm']);
  // other strings are the author's, and format 0 offers nothing in a string
  c = caret('tile { about "$bo|" }');
  assert.equal(suggest(c.caret, elements, false, language), null);
  c = caret('tile { stack { dirt top="$bo|" } }');
  assert.equal(suggest(c.caret, elements, false, null), null);
});
