// The site's live cards (docs/assets/live/): the waiting line's policy, the controls that write the source, and the card's
// messages made from the engine's diagnostics.  Pure parts only: the worker, the canvas and the page are looked at in a browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Queue, isStale } from '../../../docs/assets/live/queue.js';
import { findControl, parseControls, rangeOf, readValue, setOp } from '../../../docs/assets/live/controls.js';
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

test('a control finds its value and its node in the map, and the edit it asks is typed: an op by node id, with the fingerprint, no text', () => {
  const source = 'tile size="1x1" seed=97 {\n  line shore wobble="5dmm" { through "(0sq, 0.6sq)" "(1sq, 0.6sq)" }  // é\n}\n';
  const nodes = [{ id: 'tile', at: 0, len: source.length }, { id: 'tile/line shore', at: 28, len: 60 }];
  const map = {
    lines: [{ name: 'shore', sheet: { at: 28, controls: [{ name: 'wobble', text: '"5dmm"' }] } }],
    tile: { at: 0, controls: [{ name: 'seed', text: '97' }] },
  };
  const w = findControl(map, nodes, 'wobble');
  assert.equal(w.owner, 'line shore');
  assert.equal(w.id, 'tile/line shore');
  assert.deepEqual(readValue(w.text), { value: 5, unit: 'dmm', quoted: true });
  // the op carries the value and its unit as a value, never as KDL: the program writes the quotes
  assert.deepEqual(setOp(source, 'abc', { id: w.id, name: 'wobble', unit: 'dmm' }, 8.5), {
    read: 'abc', op: 'set', text: source, id: 'tile/line shore', name: 'wobble', value: '8.5dmm',
  });
  const s = findControl(map, nodes, 'seed');
  assert.equal(s.id, 'tile');
  assert.equal(setOp(source, 'abc', { id: s.id, name: 'seed', unit: '' }, 12).value, 12);
  assert.equal(findControl(map, nodes, 'nothing'), null);
  assert.equal(findControl(null, nodes, 'seed'), null);
  // a control whose node the check did not list cannot be edited by id: it is not offered
  assert.equal(findControl(map, [], 'wobble'), null);
});

const diag = (over = {}) => ({ code: 'dt::unit', message: '`5` has no unit', label: 'here', help: 'write 5dmm', file: 'tile.dtile', span: [10, 1], span16: [10, 1], line: 2, col: 5, fix: null, ...over });

test('a check\'s mistakes and notes are messages, and what the next check no longer reports is cleared', () => {
  const log = new CardLog('card-1', () => '1.0.0');
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
  const log = new CardLog('card-2', () => '1.0.0');
  // a whole build record, held to the program's TileRecord by messages_wire.rs
  const record = JSON.parse(readFileSync(new URL('./fixtures/tile-record.json', import.meta.url), 'utf8'));
  const meta = { built: true, record };
  log.built(meta, { draft: true, cached: false, ms: 310 });
  let v = log.view();
  assert.equal(v.record.gate, null);
  assert.match(v.record.build.text, /draft detail in 0\.31 s/);
  log.built(meta, { draft: false, cached: false, ms: 2100 });
  v = log.view();
  assert.equal(v.record.gate.text, 'Passes the printability gate');
  assert.equal(statusOf(v, { t: { status: 'ready', name: 'x' } }).verdict.startsWith('passes'), true);
  log.built({ ...meta, record: { ...meta.record, audit: { ...meta.record.audit, failures: ['3 holes'] } } }, { draft: false, cached: false, ms: 2100 });
  assert.equal(log.view().record.gate.severity, 'error');
  log.failed('the worker trapped', 'stack');
  assert.equal(log.view().rows.at(-1)?.m.kind ?? log.view().rows[0].m.kind, 'server');
  assert.equal(log.view().counts.errors, 1);
});
