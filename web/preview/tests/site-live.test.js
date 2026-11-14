// The site's live cards (docs/assets/live/): the waiting line's policy, the controls that write the source, and the card's
// messages made from the engine's diagnostics.  Pure parts only: the worker, the canvas and the page are looked at in a browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Queue, isStale } from '../../../docs/assets/live/queue.js';
import { findControl, parseControls, rangeOf, readValue, writeValue } from '../../../docs/assets/live/controls.js';
import { CardLog } from '../../../docs/assets/live/site-log.js';
import { countsText, statusOf } from '../js/messages.js';

const edit = (version) => ({ kind: 'edit', version });
const full = (version) => ({ kind: 'full', version });

test('a card holds its newest edit: a newer one replaces the older, and never another card\'s', () => {
  const q = new Queue();
  q.put('a', edit(1));
  q.put('b', edit(1));
  q.put('a', edit(2));
  assert.equal(q.pending('a', 'edit').version, 2);
  assert.equal(q.pending('b', 'edit').version, 1);
  assert.equal(q.order().length, 2);
});

test('the focused card goes first, then the others by most recent edit', () => {
  const q = new Queue();
  q.put('a', edit(1));
  q.put('b', edit(1));
  q.put('c', edit(1));
  q.focusOn('a');
  assert.deepEqual(q.order().map((x) => x.card), ['a', 'c', 'b']);
  q.focusOn('b');
  assert.deepEqual(q.order().map((x) => x.card), ['b', 'c', 'a']);
  assert.equal(q.next().card, 'b');
  assert.equal(q.ahead('c', 'edit'), 0);
});

test('an edit goes before a full build of a card that is not focused, and makes its own waiting full build stale', () => {
  const q = new Queue();
  q.put('a', full(1));
  q.put('b', edit(1));
  q.focusOn('c');
  assert.deepEqual(q.order().map((x) => `${x.card}:${x.job.kind}`), ['b:edit', 'a:full']);
  q.put('a', edit(2));
  assert.equal(q.pending('a', 'full'), null, 'a new edit drops the full build that was waiting');
  q.drop('a', 'edit');
  assert.equal(q.pending('a', 'edit'), null);
});

test('a result for an older edit is stale', () => {
  assert.equal(isStale(3, 2), true);
  assert.equal(isStale(3, 3), false);
});

test('a page picks controls by name, with ranges of its own', () => {
  assert.deepEqual(parseControls('wobble, width:4..30, seed'), [
    { name: 'wobble', min: null, max: null }, { name: 'width', min: 4, max: 30 }, { name: 'seed', min: null, max: null },
  ]);
  assert.deepEqual(rangeOf({ name: 'wobble', min: null, max: null }, 5), { min: 0, max: 12, step: 0.5 });
  assert.deepEqual(rangeOf({ name: 'width', min: 4, max: 30 }, 10), { min: 4, max: 30, step: 1 });
});

test('a control finds its value in the map and writes it where it is written, keeping unit, quotes and the rest of the source', () => {
  const source = 'tile size="1x1" seed=97 {\n  line shore wobble="5dmm" { through "(0sq, 0.6sq)" "(1sq, 0.6sq)" }  // é\n}\n';
  const at = (needle, len) => { const i = Buffer.from(source).indexOf(needle); return [i, len ?? needle.length]; };
  const map = {
    lines: [{ name: 'shore', sheet: { controls: [{ name: 'wobble', text: '"5dmm"', span: at('"5dmm"') }] } }],
    tile: { controls: [{ name: 'seed', text: '97', span: at('97') }] },
  };
  const w = findControl(map, 'wobble');
  assert.equal(w.owner, 'line shore');
  assert.deepEqual(readValue(w.text), { value: 5, unit: 'dmm', quoted: true });
  const next = writeValue(source, w, 8.5);
  assert.equal(next.text, source.replace('"5dmm"', '"8.5dmm"'));
  // a second change before the next check goes to the span the first one left
  const again = writeValue(next.text, { text: next.written, span: next.span }, 2);
  assert.equal(again.text, source.replace('"5dmm"', '"2dmm"'));
  const s = findControl(map, 'seed');
  assert.equal(writeValue(source, s, 12).text, source.replace('seed=97', 'seed=12'));
  assert.equal(findControl(map, 'nothing'), null);
  assert.equal(findControl(null, 'seed'), null);
});

const diag = (over = {}) => ({ code: 'dt::unit', message: '`5` has no unit', label: 'here', help: 'write 5dmm', file: 'tile.dtile', span: [10, 1], span16: [10, 1], line: 2, col: 5, fix: null, ...over });

test('a check\'s mistakes and notes are messages, and what the next check no longer reports is cleared', () => {
  const log = new CardLog('card-1');
  log.checked({ ok: false, errors: [diag()], notes: [diag({ code: 'dt::edge', message: 'the dirt meets the edge' })] });
  let v = log.view();
  assert.deepEqual(v.rows.map((r) => [r.m.severity, r.m.code]), [['error', 'dt::unit'], ['note', 'dt::edge']]);
  assert.equal(v.rows[0].m.span.line, 2);
  assert.equal(countsText(v.counts), '1 error · 1 note');
  const seq = v.rows[0].m.seq;
  // the same mistake again is the same message; the note is gone
  log.checked({ ok: false, errors: [diag()], notes: [] });
  v = log.view();
  assert.equal(v.rows.length, 1);
  assert.equal(v.rows[0].m.seq, seq);
  log.checked({ ok: true, errors: [], notes: [] });
  assert.equal(log.view().rows.length, 0);
});

test('a draft says it has not been through the gate; a full build reports the gate\'s verdict; the engine failing is loud and apart', () => {
  const log = new CardLog('card-2');
  const meta = { triangles: 1000, passes: true, failures: [], watertight: true, band_mm2: 0, band_budget_mm2: 1, overhang_mm2: 0, overhang_budget_mm2: 1, holes: 0 };
  log.built(meta, { draft: true, cached: false, ms: 310 });
  let v = log.view();
  assert.equal(v.record.gate, null);
  assert.match(v.record.build.text, /draft detail in 0\.31 s/);
  log.built(meta, { draft: false, cached: false, ms: 2100 });
  v = log.view();
  assert.equal(v.record.gate.text, 'Passes the printability gate');
  assert.equal(statusOf(v, { t: { status: 'ready', name: 'x' } }).verdict.startsWith('passes'), true);
  log.built({ ...meta, passes: false, failures: ['3 holes'] }, { draft: false, cached: false, ms: 2100 });
  assert.equal(log.view().record.gate.severity, 'error');
  log.failed('the worker trapped', 'stack');
  assert.equal(log.view().rows.at(-1)?.m.kind ?? log.view().rows[0].m.kind, 'server');
  assert.equal(log.view().counts.errors, 1);
});
