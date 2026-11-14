// The property sheet is a function of its model: rows from the program's description, a number scrubbed by a drag (the source
// follows the pointer, and it ends as one step), a value typed in place that keeps its words through a new state of the
// document, words and switches, and the outline's rows.
import test from 'node:test';
import assert from 'node:assert/strict';
import { signal } from '@preact/signals-core';
import { install, press, settle, type } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { Sheet } = await import('../js/view/sheet.js');
const { Outline } = await import('../js/view/outline.js');
const { setUnits } = await import('../js/value.js');
setUnits(['mm', 'dmm', 'sq']);

const schema = { name: 'scatter', controls: [
  { name: 'perSquare', unit: 'count', numeric: true, default: '4', about: 'how many' },
  { name: 'mode', words: [{ word: 'a', about: 'A' }, { word: 'b', about: 'B' }], unit: 'word' },
  { name: 'flat', unit: 'switch' },
] };
const model = (text) => ({
  title: 'flowers', kind: 'scatter', groups: [{ at: 10, schema, written: text.map(([name, t, span]) => ({ name, text: t, span })) }], actions: [{ label: 'Delete', title: 'Delete it', run() {} }],
});

function world(written) {
  const m = signal(model(written));
  const calls = [];
  const api = {
    writable: () => true, canMine: () => false, scrub: (...a) => { calls.push(['scrub', ...a]); return { to: (v) => calls.push(['to', v]), end: (v) => calls.push(['end', v]), cancel: () => calls.push(['cancel']) }; }, set: (...a) => calls.push(['set', ...a]),
    onField: (n, on) => calls.push(['field', n, on]), checkValue: async () => null,
  };
  const App = () => html`<${Sheet} model=${m.value} api=${api} />`;
  render(html`<${App} />`, root);
  return { m, calls };
}
const W = [['perSquare', 'perSquare=4', [20, 11]], ['mode', 'mode=a', [32, 6]]];

test('the sheet draws a row for each control of the description, written or not, and a switch and words as their own widgets', async () => {
  world(W);
  await settle();
  assert.deepEqual(root.querySelectorAll('.row').map((r) => r.querySelector('.nm').textContent), ['perSquare', 'mode', 'flat']);
  assert.equal(root.querySelectorAll('.faint').length, 1, 'flat is not written: its row is faint');
  assert.equal(root.querySelectorAll('.flip').length, 1);
  assert.match(root.querySelector('.entry-actions').textContent, /Delete/);
});

test('a number is scrubbed by a drag: the source follows the pointer, the step ends as one write that says it is the end', async () => {
  const { calls } = world(W);
  await settle();
  const num = root.querySelector('.num');
  num.dispatchEvent({ type: 'pointerdown', button: 0, clientX: 100, pointerId: 1, preventDefault() {} });
  num.dispatchEvent({ type: 'pointermove', clientX: 100 + 40, shiftKey: false });
  num.dispatchEvent({ type: 'pointermove', clientX: 100 + 80, shiftKey: false });
  const live = calls.filter((c) => c[0] === 'to');
  assert.equal(live.length, 2, 'a step for each move');
  assert.deepEqual(live.map((c) => c[1]), [8, 12], 'the number as it stood at each step: a value, which the program writes');
  assert.deepEqual(calls.find((c) => c[0] === 'scrub').slice(1), [10, 'perSquare', 'Changed perSquare'], 'one change, of the control on its node');
  assert.equal(calls.filter((c) => c[0] === 'scrub').length, 1);
  assert.ok(!calls.some((c) => c[0] === 'end'), 'not ended yet');
  num.dispatchEvent({ type: 'pointerup' });
  const last = calls.filter((c) => c[0] === 'end');
  assert.equal(last.length, 1, 'the end of the drag is the one step of history');
  assert.equal(last[0][1], 12);
});

test('a value being typed keeps its words and focus when the document tells the sheet a new state, and Enter writes it', async () => {
  const { m, calls } = world(W);
  await settle();
  root.querySelector('.val').dispatchEvent({ type: 'click' });
  await settle();
  const input = root.querySelector('.edit');
  assert.ok(input, 'a field in place of the value');
  type(input, '12');
  m.value = model([['perSquare', 'perSquare=4', [20, 11]], ['mode', 'mode=b', [32, 6]]]); // another control changed outside
  await settle();
  assert.equal(root.querySelector('.edit'), input, 'the same field');
  assert.equal(input.value, '12');
  press(input, 'Enter');
  await settle();
  assert.equal(root.querySelector('.edit'), null, 'the field is gone');
  const w = calls.filter((c) => c[0] === 'set').at(-1);
  assert.deepEqual(w, ['set', 10, 'perSquare', 12], 'the value, which the program writes');
});

test('the outline lists the stack top first with its numbers, and a row says what it opens', async () => {
  const calls = [];
  const on = new Proxy({}, { get: (_t, k) => (...a) => { calls.push([k, ...a.filter((x) => typeof x !== 'object')]); } });
  const data = { stack: [{ key: 'base', label: 'base', what: 'base' }, { key: 'rock#1', label: 'rock', what: 'rock' }], lines: [], regions: [], badges: new Map(), cols: 1, rows: 1, editable: true };
  render(html`<${Outline} data=${data} on=${on} />`, root);
  await settle();
  const rows = root.querySelectorAll('.orow').filter((r) => r.getAttribute('data-kind') === 'entry');
  assert.deepEqual(rows.map((r) => r.querySelector('.no').textContent), ['2', '1'], 'the top of the tile first');
  rows[0].dispatchEvent({ type: 'click' });
  assert.deepEqual(calls.at(-1), ['open', 'entry', 'rock#1']);
  assert.equal(root.querySelectorAll('.omore').length, 2, 'each row has a menu when it can change');
});
