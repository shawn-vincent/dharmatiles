import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';
import { browserHost } from '../js/host.js';
import { kdlString, parseValue, unquote, valueOf, wordControlNeedsSelect } from '../js/lens.js';
import { NO_ENTRY, ownerVertices, parsePayload, worstFlagged } from '../js/payload.js';
import { snap } from '../js/place.js';
import { indentFor, braceEnd } from '../js/text.js';
import { DEFAULTS, MIN, SASH, normalise, fit, grid } from '../js/layout.js';
import { rememberView, shortcutViews } from '../js/view-presets.js';
import { describeChange } from '../js/history.js';
import { RATIO_SPAN, STEPS, curveShape, sliderOf, valueOfSlider } from '../js/gallery.js';
import { asNumber, asRange, debounce, formOf, setUnits, valueItems, written } from '../js/value.js';

setUnits(['sq', 'mm', 'in', 'dmm', 'layer', 'deg', 'rad', 'pi']); // what the program's language table says

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

test('history describes the text that was added, changed, or removed', () => {
  assert.deepEqual(describeChange('rock size=8', 'rock size=12', 'type'), {
    title: 'Replaced text on line 1', line: 1, removed: '8', added: '12',
  });
  assert.deepEqual(describeChange('rock\nwater', 'rock', 'view', 'Deleted water'), {
    title: 'Deleted water on line 2', line: 2, removed: 'water', added: null,
  });
  assert.equal(describeChange('rock', 'rocks', 'type').title, 'Typed on line 1');
  const long = describeChange('', `about "${'x'.repeat(100)}"`, 'type');
  assert.equal(long.added.length, 76);
  assert.match(long.added, /…$/);
});

function caret(text, marker = '|') {
  const index = text.indexOf(marker);
  const clean = text.replace(marker, '');
  return { text: clean, caret: { index, before: clean.slice(0, index) } };
}

test('completion offers element names at a node', () => {
  const c = caret('tile { stack { ro| } }');
  const got = suggest(c.caret, elements);
  assert.deepEqual(got.items.map((x) => x.label), ['rock']);
  assert.equal(got.items[0].insert, 'rock');
});

test('completion offers controls but not ones already written', () => {
  const c = caret('tile { stack { rock size=8 sh| } }');
  const got = suggest(c.caret, elements);
  assert.deepEqual(got.items.map((x) => x.label), ['shape']);
  assert.equal(got.items[0].insert, 'shape=');
});

test('completion knows regions and line edges', () => {
  let c = caret('tile { region bank { fill "0.5,0.5" }; stack { grass at=ba| } }');
  assert.deepEqual(suggest(c.caret, elements, false, null, { regions: ['bank'] }).items.map((x) => x.label), ['bank']);
  c = caret('tile { stack { wall at=no| } }');
  assert.deepEqual(suggest(c.caret, elements).items.map((x) => x.label), ['north']);
});

test('completion stays out of strings and comments', () => {
  let c = caret('tile { about "ro|" }');
  assert.equal(suggest(c.caret, elements), null);
  c = caret('tile { // ro|\n stack {} }');
  assert.equal(suggest(c.caret, elements), null);
});

test('a comment on an earlier line does not disable completion', () => {
  const c = caret('// a useful note\ntile { stack { ro| } }');
  assert.deepEqual(suggest(c.caret, elements).items.map((x) => x.label), ['rock']);
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

test('word controls become dropdowns before their buttons would wrap', () => {
  const words = (...names) => names.map((word) => ({ word }));
  assert.equal(wordControlNeedsSelect(words('bare', 'dirt')), false);
  assert.equal(wordControlNeedsSelect(words('rubble', 'broken', 'massive')), true);
  assert.equal(wordControlNeedsSelect(words('one very long choice', 'another')), true);
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
  globalThis.fetch = async () => { throw new TypeError('Failed to fetch'); };
  try {
    assert.deepEqual(await browserHost().edit({}), {
      status: 0,
      body: { error: 'the editor is disconnected' },
    });
  } finally {
    globalThis.fetch = original;
  }
});

test('history names an indent at the start of a line as spacing, not a removal', () => {
  const d = describeChange('rock', '  rock', 'type');
  assert.equal(d.removed, null);
  assert.equal(d.title, 'Changed spacing on line 1');
});

test('a stored layout falls back to the defaults field by field', () => {
  assert.deepEqual(normalise(null), { ...DEFAULTS });
  assert.deepEqual(normalise({ mode: 'sideways', side: 'right', sidew: -5, texth: 3, hideSource: 'yes' }),
    { ...DEFAULTS, side: 'right' });
  assert.equal(normalise({ mode: 'inspect', sourcew: 410 }).sourcew, 410);
});

test('panes give way so the view keeps its minimum', () => {
  const w = 1280;
  const e = fit({ ...DEFAULTS, sidew: 5000 }, w);
  assert.equal(e.sidew, w - MIN.view - SASH);
  const i = fit({ ...DEFAULTS, mode: 'inspect', sourcew: 900, propw: 300 }, w);
  assert.ok(i.sourcew + i.propw + 2 * SASH + MIN.view <= w);
  assert.ok(i.sourcew >= MIN.source && i.propw >= MIN.props);
  assert.equal(fit({ ...DEFAULTS, sidew: 10 }, w).sidew, MIN.source);
});

test('the grid puts every shown pane and sash in place', () => {
  const edit = grid({ ...DEFAULTS });
  assert.match(edit.areas, /"side sash-side view"/);
  assert.equal(edit.editor.areas, '"source" "sash-split" "properties"');
  assert.equal(edit.props, 'editor');
  assert.match(grid({ ...DEFAULTS, side: 'right' }).areas, /"view sash-side side"/);
  assert.equal(grid({ ...DEFAULTS, hideProps: true }).editor.areas, '"source"');
  assert.equal(grid({ ...DEFAULTS, hideSource: true }).editor.areas, '"properties"');
  const ins = grid({ ...DEFAULTS, mode: 'inspect' });
  assert.match(ins.areas, /"side sash-side view sash-props properties"/);
  assert.equal(ins.props, 'app');
  assert.match(grid({ ...DEFAULTS, mode: 'inspect', side: 'right', hideSource: true }).areas, /"properties sash-side view"/);
  assert.equal(grid({ ...DEFAULTS, hideSource: true, hideProps: true }).areas, '"title" "view"');
});

test('a new line is indented by the line above, one more after {, one less before }', () => {
  const t = 'tile {\n    stack {\n        rock\n    }\n}';
  assert.equal(indentFor(t, t.indexOf('\n')), 4); // after "tile {"
  assert.equal(indentFor(t, t.indexOf('rock') + 4), 8);
  const close = t.indexOf('    }');
  assert.equal(indentFor(t, close + 4), 4); // before "}" under stack
  assert.equal(indentFor('a // { not a brace\n', 18), 0);
});

test('a brace folds to its own closing brace, skipping strings and comments', () => {
  const t = 'stack {\n    about "a { b"\n    // }\n    rock\n}\nwater';
  assert.equal(braceEnd(t, t.indexOf('{')), t.lastIndexOf('}'));
  assert.equal(braceEnd('a {', 2), null);
});

test('a description is written as a KDL v2 string and read back the same', () => {
  const s = 'A "pool" \\ by the bank\n\u0001';
  const k = kdlString(s);
  assert.equal(k, '"A \\"pool\\" \\\\ by the bank\\n\\u{1}"');
  assert.equal(unquote(k), s);
  assert.equal(unquote('#"raw "quotes""#'), 'raw "quotes"');
});

test('a typed value is written bare when KDL takes it bare, else quoted', () => {
  assert.equal(written('6'), '6');
  assert.equal(written(' (h)1.2 '), '(h)1.2');
  assert.equal(written('edge'), 'edge');
  assert.equal(written('6..15'), '"6..15"');
  assert.equal(written('ground - 1'), '"ground - 1"');
  assert.equal(written('30%'), '"30%"');
  assert.equal(written('inf'), '"inf"');
});

test('a value switches between one number and a range', () => {
  assert.equal(formOf('6'), 'number');
  assert.equal(formOf('6..15'), 'range');
  assert.equal(formOf('20%..50%'), 'range');
  assert.equal(formOf('ground - 1'), 'expression');
  assert.equal(formOf('full'), 'word');
  assert.equal(asRange('6'), '6..6');
  assert.equal(asNumber('6..15'), '6');
  assert.equal(asRange('ground'), 'ground');
});

test("a control's words are offered while only a word is typed", () => {
  const height = { unit: 'mm', words: [{ word: 'full', about: 'up to the wall top' }, { word: 'low', about: 'half' }] };
  assert.deepEqual(valueItems('', 0, height).items.map((i) => i.label), ['full', 'low']);
  assert.deepEqual(valueItems('l', 1, height).items.map((i) => i.label), ['low']);
  // the whole word selected, the caret before it: every word
  assert.deepEqual(valueItems('full', 0, height).items.map((i) => i.label), ['full', 'low']);
  assert.deepEqual(valueItems('12 l', 4, height).items, []);
});

test('the sheet asks the program to check a typed value', async () => {
  const sent = [];
  const realFetch = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    if (url === '/api/session') return new Response('{"token":"t"}');
    sent.push([url, JSON.parse(init.body)]);
    return new Response('{"message":"takes 20 to 120"}');
  };
  try {
    const r = await browserHost().checkValue('tree', 'size', '150');
    assert.deepEqual(sent, [['/api/check-value', { element: 'tree', control: 'size', typed: '150' }]]);
    assert.equal(r.body.message, 'takes 20 to 120');
  } finally {
    globalThis.fetch = realFetch;
  }
});

// ---- the editor's stores ----------------------------------------------------

test('a store tells only those who named a key that changed', async () => {
  const { createStore } = await import('../js/editor/stores.js');
  const s = createStore({ a: 1, b: 2 });
  const seen = [];
  const stop = s.subscribe(['a'], (st, keys) => seen.push(['a', st.a, keys]));
  s.subscribe(null, (st, keys) => seen.push(['all', keys]));
  s.set({ b: 3 });
  s.set({ a: 1 }); // no change: nobody is told
  s.set({ a: 5, b: 3 });
  stop();
  s.set({ a: 6 });
  assert.deepEqual(seen, [['all', ['b']], ['a', 5, ['a']], ['all', ['a']], ['all', ['a']]]);
  assert.equal(s.get().a, 6);
});

test('the selection is one shape, and the same thing selected twice is the same', async () => {
  const { entrySel, markSel, same, isEntry, isMark } = await import('../js/editor/stores.js');
  assert.ok(same(entrySel(2), entrySel(2, null)));
  assert.ok(!same(entrySel(2), entrySel(2, 0)));
  assert.ok(!same(markSel('line', 'a'), markSel('region', 'a')));
  assert.ok(same({ kind: 'tile', id: 'x' }, { kind: 'tile', id: 'y' }));
  assert.ok(same(null, null) && !same(null, entrySel(0)));
  assert.ok(isEntry(entrySel(0)) && isMark(markSel('line', 'a')) && !isMark(entrySel(0)));
});

test('a tile just made is waited for until the server lists it', async () => {
  const { server, whenListed, tileById } = await import('../js/editor/stores.js');
  server.set({ tiles: [{ id: 'a' }] });
  assert.equal(await whenListed('a'), true);
  const later = whenListed('b');
  server.set({ tiles: [{ id: 'a' }, { id: 'b' }] });
  assert.equal(await later, true);
  assert.equal(tileById('b').id, 'b');
  assert.equal(tileById('nope'), null);
});

test('a dropped stream is opened again with its epoch and last revision', () => {
  const urls = [];
  const sources = [];
  const original = globalThis.EventSource;
  const realTimeout = globalThis.setTimeout;
  globalThis.EventSource = class {
    constructor(url) { urls.push(url); this.close = () => {}; sources.push(this); }
  };
  globalThis.setTimeout = (fn) => { fn(); return 0; };
  try {
    const got = [];
    browserHost().connect({
      onSnapshot: (m) => got.push(['snapshot', m.rev]),
      onTile: (t) => got.push(['tile', t.id]),
      onProgress: (id) => got.push(['progress', id]),
      onState: (s) => got.push(s),
    });
    const send = (m) => sources[sources.length - 1].onmessage({ data: JSON.stringify(m) });
    send({ type: 'snapshot', rev: 4, epoch: 'e1', tiles: [] });
    send({ type: 'tile', rev: 7, tile: { id: 'a' } });
    send({ type: 'progress', rev: 7, id: 'a', progress: {} });
    sources[0].onerror();
    assert.deepEqual(urls, ['/api/events', '/api/events?epoch=e1&since=7']);
    send({ type: 'resumed', rev: 9, epoch: 'e1' });
    assert.deepEqual(got, ['live', ['snapshot', 4], ['tile', 'a'], ['progress', 'a'], 'offline', 'live']);
  } finally {
    globalThis.EventSource = original;
    globalThis.setTimeout = realTimeout;
  }
});

test('a burst of typing asks the program once, with the last text', () => {
  const pending = new Map();
  let next = 1;
  const timers = {
    set: (fn) => { pending.set(next, fn); return next++; },
    clear: (id) => pending.delete(id),
  };
  const asked = [];
  const ask = debounce((typed) => asked.push(typed), 200, timers);
  ask('4');
  ask('40');
  ask('40.');
  assert.deepEqual(asked, []);
  assert.equal(pending.size, 1);
  const [[due, run]] = [...pending];
  pending.delete(due); // a timer is gone once it has fired
  run();
  assert.deepEqual(asked, ['40.']);
  ask('5');
  assert.equal(pending.size, 1);
  ask.cancel();
  assert.equal(pending.size, 0);
  assert.deepEqual(asked, ['40.']);
});

test('only a lost connection is status 0; any other failure of a call is thrown', async () => {
  const original = globalThis.fetch;
  globalThis.fetch = async () => { throw new RangeError('a bug'); };
  try {
    await assert.rejects(browserHost().edit({}), /a bug/);
  } finally {
    globalThis.fetch = original;
  }
});

test('a reply that is not JSON has its words as the error', async () => {
  const original = globalThis.fetch;
  globalThis.fetch = async (url) => (String(url).endsWith('session')
    ? new Response('{"token":"t"}')
    : new Response('not this page', { status: 403 }));
  try {
    const r = await browserHost().edit({});
    assert.deepEqual({ status: r.status, body: r.body }, { status: 403, body: { error: 'not this page' } });
    assert.equal(r.error.kind, 'refused');
  } finally {
    globalThis.fetch = original;
  }
});

test('unexpected failures are kept with what was being done, once each, and shown to listeners', async () => {
  const { showError, failed, problems, dismiss, onProblems, installGlobalHandlers } = await import('../js/problems.js');
  const log = console.error; console.error = () => {};
  try {
    let told = 0;
    onProblems(() => { told += 1; });
    showError('Could not save a', new Error('disk full', { cause: new Error('no space') }));
    showError('Could not save a', new Error('disk full'));
    assert.equal(problems().length, 1);
    assert.equal(problems()[0].count, 2);
    assert.match(problems()[0].details, /\[2\][^\n]*\n\s+no space/);
    assert.equal(failed('x', { status: 0, body: { error: 'disconnected' } }), false, 'a lost connection is the chip\'s');
    assert.equal(failed('x', { status: 422, body: { error: 'a mistake' } }), false, 'a refusal with its reason is shown where asked');
    assert.equal(failed('Could not y', { status: 500, body: { error: 'boom' } }), true);
    assert.equal(problems().at(-1).message, 'boom');
    assert.ok(told >= 3);
    const handlers = {};
    installGlobalHandlers({ addEventListener: (k, f) => { handlers[k] = f; } });
    handlers.unhandledrejection({ reason: new Error('lost promise') });
    handlers.error({ message: 'ResizeObserver loop completed with undelivered notifications.' });
    handlers.error({ message: 'x', error: new Error('script broke') });
    assert.deepEqual(problems().slice(-2).map((p) => p.message), ['lost promise', 'script broke']);
    dismiss();
    assert.equal(problems().length, 0);
  } finally {
    console.error = log;
  }
});

// ---- the weighting gallery (js/gallery.js): geometry and sliders; the curves are the program's ----

const RATIO = { name: 'ratio', default: 4, least: 0, above: true, most: null };
const SPREAD = { name: 'spread', default: 0.2, least: 0.05, above: false, most: 1 };

test('a ratio slides on a log scale from 0.1 to 100, and the default sits where it should', () => {
  assert.equal(valueOfSlider(RATIO, 0), RATIO_SPAN[0]);
  assert.equal(valueOfSlider(RATIO, STEPS), RATIO_SPAN[1]);
  // a third of the way is a tenth of the decades: 0.1 * 10^1 = 1
  assert.equal(valueOfSlider(RATIO, STEPS / 3), 1);
  // the number written is two figures, so the source stays short
  for (const at of [123, 457, 777]) assert.match(String(valueOfSlider(RATIO, at)), /^\d*\.?\d{1,2}$/);
  // the default round-trips to within a step, and a value past the ends stops at them
  assert.ok(Math.abs(valueOfSlider(RATIO, sliderOf(RATIO, 4)) - 4) < 0.1);
  assert.equal(sliderOf(RATIO, 5000), STEPS);
  assert.equal(sliderOf(RATIO, 0.001), 0);
});

test('a bounded number slides straight between its limits, in hundredths', () => {
  assert.equal(valueOfSlider(SPREAD, 0), 0.05);
  assert.equal(valueOfSlider(SPREAD, STEPS), 1);
  assert.equal(valueOfSlider(SPREAD, 500), 0.53); // 0.05 + 0.95 / 2 = 0.525, to hundredths
  assert.equal(sliderOf(SPREAD, 0.525), 500);
  // past its limits the slider stops; it never writes a number the program would refuse
  assert.equal(valueOfSlider(SPREAD, -50), 0.05);
  assert.equal(valueOfSlider(SPREAD, STEPS + 50), 1);
});

test('a density is drawn as an outline over a flat line at the height of a plain range', () => {
  const flat = curveShape({ kind: 'density', values: Array(8).fill(1) }, 64, 32);
  assert.match(flat.line, /^M/);
  assert.ok(flat.area.endsWith('Z'));
  // the curve of a plain range lies on the flat line
  const y = flat.line.match(/M[\d.]+ ([\d.]+)/)[1];
  assert.equal(Number(y), flat.flat);
  // a sharp curve is scaled to its own top: its peak is nearer the top than the flat line is
  const sharp = curveShape({ kind: 'density', values: [0, 0, 4, 0] }, 64, 32);
  const ys = [...sharp.line.matchAll(/ ([\d.]+)/g)].map((m) => Number(m[1]));
  assert.ok(Math.min(...ys) < sharp.flat);
  assert.ok(Math.min(...ys) >= 2); // inside the padding
});

test('a list is drawn as one bar per value, the likeliest tallest', () => {
  const { bars } = curveShape({ kind: 'masses', values: [[0, 0.5], [0.5, 0.25], [1, 0.25]] }, 64, 32);
  assert.equal(bars.length, 3);
  assert.ok(bars[0].h > bars[1].h && bars[1].h === bars[2].h);
  assert.ok(bars[0].x < bars[1].x && bars[1].x < bars[2].x);
  // every bar stands on the same floor
  assert.ok(bars.every((b) => Math.abs(b.y + b.h - 30) < 1e-9));
});

test('the host reads the program\'s weighting curves with a query and gives an answer or a failure', async () => {
  const seen = [];
  const real = globalThis.fetch;
  globalThis.fetch = async (url) => { seen.push(String(url)); return { ok: true, status: 200, text: async () => JSON.stringify({ ok: true, text: 'low(1..6, 4)' }) }; };
  try {
    const r = await browserHost().weightings({ fn: 'low', range: '1..6', args: '4' });
    assert.equal(seen[0], '/api/weightings?fn=low&range=1..6&args=4');
    assert.deepEqual(r, { status: 200, body: { ok: true, text: 'low(1..6, 4)' } });
    // a reply that is not JSON is a failure, not a silent null
    globalThis.fetch = async () => ({ ok: true, status: 200, text: async () => 'oops' });
    await assert.rejects(browserHost().weightings({ range: '1..6' }), /not JSON/);
    // a refusal with its reason is the body's, for the caller to show
    globalThis.fetch = async () => ({ ok: false, status: 400, text: async () => '{"error":"give a range"}' });
    const refusal = await browserHost().weightings({});
    assert.deepEqual({ status: refusal.status, body: refusal.body }, { status: 400, body: { error: 'give a range' } });
    assert.equal(refusal.error.kind, 'refused');
    // lost connection: status 0, the chip's business
    globalThis.fetch = async () => { throw new TypeError('failed to fetch'); };
    assert.equal((await browserHost().weightings({ range: '1..6' })).status, 0);
  } finally {
    globalThis.fetch = real;
  }
});
