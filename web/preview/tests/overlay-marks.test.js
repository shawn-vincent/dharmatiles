// The overlay's marks and their states (js/overlay-marks.js), and the lookups the caret and the pointer make: pure, from a map and
// a text offset, down to a single point.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { byteToIndex } from '../js/spans.js';
import { TOGGLES, byteOffset, drawn, markAtOffset, marksOf, parsePins, sourceRange, stateOf } from '../js/overlay-marks.js';
import { heightAtGrid, heightGrid } from '../js/overlay-geometry.js';

// tile size="1x1" { line shore { through "0,0.4" "1,0.4" } region pool { fill "0.5,0.2" } }
const TEXT = 'tile size="1x1" {\n  line shore { through "0,0.4" "1,0.4" }\n  region pool { boundedBy shore; fill "0.5,0.2" }\n}\n';
const span = (needle, nth = 0) => {
  let at = -1;
  for (let k = 0; k <= nth; k++) at = TEXT.indexOf(needle, at + 1);
  return [at, needle.length];
};
const MAP = {
  cols: 1, rows: 1, cells: 4,
  lines: [{
    name: 'shore', pts: [[0, 0.4], [1, 0.4]], span: span('line shore { through "0,0.4" "1,0.4" }'),
    points: [{ text: '"0,0.4"', span: span('"0,0.4"'), at: [0, 0.4] }, { text: '"1,0.4"', span: span('"1,0.4"'), at: [1, 0.4] }],
  }],
  regions: [{
    name: 'pool', nx: 5, ny: 5, bits: '0'.repeat(25), span: span('region pool { boundedBy shore; fill "0.5,0.2" }'),
    fills: [{ text: '"0.5,0.2"', span: span('"0.5,0.2"'), at: [0.5, 0.2] }],
  }],
  points: [],
};

test('every line, point, region and fill point is a mark with its source span', () => {
  const marks = marksOf(MAP);
  assert.deepEqual(marks.map((m) => m.id), ['line:shore', 'point:shore#0', 'point:shore#1', 'region:pool', 'fill:pool#0']);
  assert.equal(marksOf(null).length, 0);
});

test('the caret on a point lights that point; between points it lights the line; outside any mark, nothing', () => {
  const marks = marksOf(MAP);
  const at = (needle, plus = 1) => TEXT.indexOf(needle) + plus;
  assert.equal(markAtOffset(marks, TEXT, at('"1,0.4"')), 'point:shore#1');
  assert.equal(markAtOffset(marks, TEXT, at('"0,0.4"')), 'point:shore#0');
  assert.equal(markAtOffset(marks, TEXT, at('through')), 'line:shore');
  assert.equal(markAtOffset(marks, TEXT, at('boundedBy')), 'region:pool');
  assert.equal(markAtOffset(marks, TEXT, at('fill "0.5')), 'region:pool');
  assert.equal(markAtOffset(marks, TEXT, at('"0.5,0.2"')), 'fill:pool#0');
  assert.equal(markAtOffset(marks, TEXT, 3), null, 'the tile header is no mark');
});

test('offsets count bytes in the map and code units in a text area', () => {
  const text = 'é"0,0"';
  assert.equal(byteOffset(text, 1), 2);
  assert.equal(byteToIndex(text, 2), 1);
  assert.equal(byteToIndex(text, 0), 0);
  const marks = marksOf({ lines: [{ name: 'x', span: [0, 7], points: [{ span: [2, 5], at: [0, 0] }] }], regions: [] });
  // the quote after the accent is at code unit 1, byte 2: the point
  assert.equal(markAtOffset(marks, text, 2), 'point:x#0');
  assert.deepEqual(sourceRange(marks[1], text), [1, 6]);
  // a node's span ends with its newline: the range lit does not
  assert.deepEqual(sourceRange({ span: [0, 7] }, 'ab { }\nrest'), [0, 6]);
});

test('a mark is off, shown by its toggle or its lit parent, and lit when pinned, hovered or under the caret', () => {
  const marks = marksOf(MAP);
  const m = Object.fromEntries(marks.map((k) => [k.id, k]));
  assert.equal(stateOf(m['line:shore'], {}), 'off');
  assert.equal(stateOf(m['line:shore'], { toggles: new Set(['lines']) }), 'shown');
  assert.equal(stateOf(m['point:shore#0'], { toggles: new Set(['lines']) }), 'off', 'points are their own toggle');
  assert.equal(stateOf(m['point:shore#0'], { toggles: new Set(['points']) }), 'shown');
  assert.equal(stateOf(m['fill:pool#0'], { toggles: new Set(['points']) }), 'shown');
  assert.equal(stateOf(m['region:pool'], { toggles: new Set(['regions']) }), 'shown');
  assert.equal(stateOf(m['line:shore'], { hover: 'line:shore' }), 'lit');
  assert.equal(stateOf(m['point:shore#1'], { hover: 'line:shore' }), 'shown', 'a lit line shows its points');
  assert.equal(stateOf(m['point:shore#1'], { caret: 'point:shore#1' }), 'lit');
  assert.equal(stateOf(m['region:pool'], { pinned: new Set(['region:pool']), toggles: new Set() }), 'lit');
  // nothing the reader does hides a pin
  assert.equal(stateOf(m['region:pool'], { pinned: new Set(['region:pool']), hover: 'line:shore', toggles: new Set() }), 'lit');
  assert.deepEqual(drawn(marks, { toggles: new Set(['lines']) }).map((d) => d.mark.id), ['line:shore']);
  assert.deepEqual(TOGGLES.map((t) => t[0]), ['lines', 'points', 'regions', 'squares']);
});

test('a page pins marks in words, and a pin that names nothing is an error in words', () => {
  const marks = marksOf(MAP);
  const { ids, errors } = parsePins('line shore, point 2, region pool, fill 1', marks);
  assert.deepEqual([...ids], ['line:shore', 'point:shore#1', 'region:pool', 'fill:pool#0']);
  assert.deepEqual(errors, []);
  const bad = parsePins('line nowhere, point 9', marks);
  assert.equal(bad.ids.size, 0);
  assert.equal(bad.errors.length, 2);
  assert.match(bad.errors[0], /names nothing/);
});

test('the height grid lies on the highest corner of each cell and fills the empty ones', () => {
  // one triangle at z 3 over the lower left, none elsewhere
  const P = [0, 0, 3, 1, 0, 3, 0, 1, 3];
  const H = heightGrid(P, 1, [4, 4]);
  assert.equal(heightAtGrid(H, 0.5, 0.5, -1), 3);
  assert.equal(heightAtGrid(H, 3.5, 3.5, -1), 3, 'holes are filled from their neighbours');
  assert.equal(heightGrid([], 0, [4, 4]), null);
  assert.equal(heightAtGrid(null, 1, 1, 7), 7);
});
