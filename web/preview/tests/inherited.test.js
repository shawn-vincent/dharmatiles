// What the sheet shows where a control is not written (docs/preview.html, "The property sheet"): the value a build uses, faint,
// with where it comes from beside it, so there is never a "not set".  The program says what each is (`defaults` on the entry:
// from the element, from its schema's default, or in words for what leaving it out means); the page only shows it.  A control
// the program says nothing about is shown as a mistake, loudly, never as a blank.  Editing an inherited value writes it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { install, settle } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { Sheet, rowsOf, inheritedOf } = await import('../js/view/sheet.js');
const { setUnits } = await import('../js/value.js');
setUnits(['sq', 'mm', 'in', 'dmm', 'layer', 'deg', 'rad', 'pi']);

const STYLE = [{ word: 'cut-stone' }, { word: 'brick' }, { word: 'fieldstone' }];
const element = { name: 'wall', controls: [
  { name: 'style', unit: 'word', words: STYLE, default: 'cut-stone', default1: 'cut-stone' },
  { name: 'stones', unit: null, subs: [
    { name: 'height', unit: 'dmm', numeric: true, ranges: true },
    { name: 'texture', unit: 'word', words: [{ word: 'chipped' }, { word: 'worn' }, { word: 'hewn' }, { word: 'dressed' }, { word: 'smooth' }] },
  ] },
  { name: 'bond', unit: 'word', words: [{ word: 'irregular' }, { word: 'running' }] },
  { name: 'ruin', unit: 'percent', numeric: true },
  { name: 'shape', unit: 'word', words: [{ word: 'rounded' }, { word: 'angular' }, { word: 'flat' }] },
  { name: 'trunk', unit: null, subs: [{ name: 'width', unit: 'dmm', numeric: true }] },
  { name: 'odd', unit: 'dmm', numeric: true },
] };
const rows = { ...element, controls: rowsOf(element.controls) };
const DEFAULTS = [
  { control: 'style', text: 'cut-stone', from: 'default' },
  { control: 'stones/height', text: '2.5dmm .. 3dmm', from: 'style brick' },
  { control: 'stones/texture', text: "brick's own finish", from: 'style brick' },
  { control: 'bond', text: 'irregular', from: 'style brick' },
  { control: 'ruin', text: '0%', from: 'default' },
  { control: 'shape', text: 'a mix of all three', from: '' },
  { control: 'trunk/width', text: 'grows with the branches it carries', from: '' },
  // `odd` is not here: the program said nothing about it
];

function world(written = [], defaults = DEFAULTS) {
  const calls = [];
  const api = {
    writable: () => true, canMine: () => false,
    replace: (...a) => calls.push(['replace', ...a]), set: (...a) => calls.push(['set', ...a]),
    onField: () => {}, checkValue: async () => null, unitAbout: () => ({}),
  };
  const model = { title: 'wall', kind: 'wall', groups: [{ at: 10, schema: rows, written, defaults }] };
  render(html`<${Sheet} model=${model} api=${api} />`, root);
  return { calls };
}
const has = (el, c) => (el.className || el.getAttribute('class') || '').split(/\s+/).includes(c);
const valueOfSelect = (el) => el.value ?? el.getAttribute('value');
const rowOf = (name) => [...root.querySelectorAll('.row')].find((r) => r.querySelector('.nm').textContent.trim() === name);

test('a group has no row of its own: its sub-controls are rows named by their path, as the document writes them', () => {
  assert.deepEqual(rows.controls.map((c) => c.name), ['style', 'stones/height', 'stones/texture', 'bond', 'ruin', 'shape', 'trunk/width', 'odd']);
  assert.equal(rows.controls[1].label, 'stones height');
});

test('an unwritten control shows what a build uses, faint, with where it comes from; a written one shows neither', async () => {
  world([{ name: 'bond', text: 'bond running', span: [30, 12] }]);
  await settle();
  const h = rowOf('stones height');
  assert.ok(has(h, 'faint'));
  assert.equal(h.querySelector('.val').textContent.replace(/\s/g, ''), '2.5–3dmm');
  assert.equal(h.querySelector('.from').textContent, 'from style brick');
  assert.equal(rowOf('ruin').querySelector('.from').textContent, 'default');
  const bond = rowOf('bond');
  assert.ok(!has(bond, 'faint'));
  assert.equal(bond.querySelector('.from'), null, 'a written value is not marked');
});

test('a word control shows the inherited word chosen, never "not set"', async () => {
  world();
  await settle();
  const style = rowOf('style').querySelector('select');
  assert.equal(valueOfSelect(style), 'cut-stone');
  assert.ok(![...style.querySelectorAll('option')].some((o) => /not set/.test(o.textContent)));
  const bond = rowOf('bond');
  assert.equal([...bond.querySelectorAll('button')].filter((b) => has(b, 'inh')).map((b) => b.textContent).join(), 'irregular');
  // a word with no name for what it is: the words for what leaving it out means, in the select
  const tex = rowOf('stones texture').querySelector('select');
  assert.equal(valueOfSelect(tex), '', 'brick has a finish of its own, not one of the words');
  assert.match([...tex.querySelectorAll('option')].find((o) => o.disabled).textContent, /brick's own finish/);
});

test('where leaving a control out means something, the words say it: no value, no mark', async () => {
  world();
  await settle();
  const r = rowOf('trunk width');
  assert.equal(r.querySelector('.absent').textContent, 'grows with the branches it carries');
  assert.equal(r.querySelector('.from'), null);
  const shape = rowOf('shape').querySelector('select');
  assert.match([...shape.querySelectorAll('option')].find((o) => o.disabled).textContent, /a mix of all three/);
});

test('a control the program says nothing about is a mistake, shown and logged, never a blank or "not set"', async () => {
  const errors = [];
  const was = console.error;
  console.error = (...a) => errors.push(a.join(' '));
  try {
    world();
    await settle();
  } finally { console.error = was; }
  const r = rowOf('odd');
  assert.match(r.textContent, /no default for odd/);
  assert.ok(r.querySelector('.lost'));
  assert.ok(errors.some((e) => /no default for wall odd/.test(e)), errors.join('\n'));
  assert.ok(!/not set/.test(root.textContent));
});

test('writing an inherited value writes it: a word chosen, a number dragged', async () => {
  const { calls } = world();
  await settle();
  const b = [...rowOf('bond').querySelectorAll('button')].find((x) => x.textContent === 'running');
  b.dispatchEvent({ type: 'click' });
  assert.deepEqual(calls.at(-1), ['set', 10, 'bond', 'running']);
  const num = rowOf('stones height').querySelectorAll('.num')[0];
  num.dispatchEvent({ type: 'pointerdown', button: 0, clientX: 100, pointerId: 1, preventDefault() {} });
  assert.deepEqual(calls.at(-1), ['set', 10, 'stones/height', '2.5dmm .. 3dmm'], 'it is written first, as it is, by the path the program edits');
});

test('the tile\'s own nodes, which carry no program defaults, show their schema\'s default or its words', () => {
  assert.deepEqual(inheritedOf({}, { name: 'size', default1: '1x1' }), { text: '1x1', from: 'default' });
  assert.deepEqual(inheritedOf({}, { name: 'about', absent: 'no description' }), { text: 'no description', from: '' });
  assert.equal(inheritedOf({}, { name: 'x' }), null);
  assert.equal(inheritedOf({ defaults: [] }, { name: 'bond', default1: 'irregular' }), null, 'an entry takes only what the program says');
});

test('the sheet has no "not set" in it, and works out no default itself', () => {
  const src = readFileSync(new URL('../js/view/sheet.js', import.meta.url), 'utf8');
  assert.ok(!/['"`>]not set\b/.test(src), 'no "not set" text');
  const prop = readFileSync(new URL('../js/view/property.js', import.meta.url), 'utf8');
  assert.ok(!/not set\b/.test(prop.replace(/\/\/.*$/gm, '')));
});
