import test from 'node:test';
import assert from 'node:assert/strict';

import { suggest } from '../js/complete.js';
import { browserHost } from '../js/host.js';
import { kdlString, parseValue, unquote, valueOf, wordControlNeedsSelect } from '../js/lens.js';
import { NO_ENTRY, ownerVertices, parsePayload, worstFlagged } from '../js/payload.js';
import { snap } from '../js/place.js';
import { indentFor, braceEnd } from '../js/text.js';
import { DEFAULTS, MIN, SASH, normalise, fit, grid } from '../js/layout.js';
import { PINNED_VIEWS, presetAt } from '../js/view-presets.js';
import { describeChange } from '../js/history.js';
import { asNumber, asRange, debounce, formOf, setUnits, valueItems, written } from '../js/value.js';

setUnits(['sq', 'mm', 'in', 'dmm', 'layer', 'deg', 'rad', 'pi']); // what the program's language table says

const elements = [
  {
    name: 'rock', role: 'thing', about: 'one rock',
    controls: [
      { name: 'size', about: 'its size', unit: 'dmm' },
      { name: 'shape', about: 'its shape', words: [{ word: 'round', about: 'rounded' }, { word: 'sharp', about: 'angular' }] },
    ],
  },
  { name: 'grass', role: 'plant', about: 'blades of grass', controls: [] },
];

test('the view bar pins four cameras and lights the preset the camera is at, within 2 degrees', () => {
  assert.deepEqual(PINNED_VIEWS, ['painted', 'top', 'front', 'close']);
  const views = [['painted', 'Iso', 40, -120, false], ['top', 'Top', 89, -90, false], ['front', 'Front', 0, -90, false], ['close', 'Close-up', 38, -125, true], ['under', 'Under', -60, -120, false]];
  assert.equal(presetAt(views, 40, -120), 'painted');
  assert.equal(presetAt(views, 41.5, -119), 'painted');
  assert.equal(presetAt(views, 89, -90), 'top');
  assert.equal(presetAt(views, 0, -90), 'front');
  assert.equal(presetAt(views, 38, -125), 'close');
  assert.equal(presetAt(views, 38, -125, 'close'), 'close');
  assert.equal(presetAt(views, 30, -120), null, 'an orbit away from every preset lights none');
  assert.equal(presetAt(views, 0, -85), null, '5 degrees from Front is not at Front');
  assert.equal(presetAt(views, -60, -120), 'under');
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
  assert.deepEqual(normalise({ mode: 'sideways', side: 'right', sidew: -5, texth: 3, hidePanel: 'yes' }),
    { ...DEFAULTS, side: 'right' });
  assert.equal(normalise({ sidew: 410 }).sidew, 410);
  // layouts from before the one panel: both panes hidden was Look; the sidebar's width is kept
  assert.equal(normalise({ hideSource: true, hideProps: true }).hidePanel, true);
  assert.equal(normalise({ mode: 'inspect', hideSource: true }).hidePanel, false);
});

test('the panel gives way so the view keeps its minimum', () => {
  const w = 1280;
  assert.equal(fit({ ...DEFAULTS, sidew: 5000 }, w).sidew, w - MIN.view - SASH);
  assert.equal(fit({ ...DEFAULTS, sidew: 10 }, w).sidew, MIN.panel);
});

test('the grid puts the panel and its sash in place, or the view alone', () => {
  assert.match(grid({ ...DEFAULTS }).areas, /"side sash-side view"/);
  assert.match(grid({ ...DEFAULTS, side: 'right' }).areas, /"view sash-side side"/);
  assert.equal(grid({ ...DEFAULTS, hidePanel: true }).areas, '"title" "view"');
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
  const height = { unit: 'dmm', words: [{ word: 'full', about: 'up to the wall top' }, { word: 'low', about: 'half' }] };
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
