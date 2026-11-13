// The plates panel is a function of its model: drawn from a plan, drawn again by itself when the plan changes, and a field
// being typed in keeps its words and focus through the program telling it a new plan.
import test from 'node:test';
import assert from 'node:assert/strict';
import { computed, signal } from '@preact/signals-core';
import { install, press, settle, type } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { PlatesPanel } = await import('../js/view/plates-panel.js');

const plan = (gap, pieces = 2) => ({
  planned: true, writable: true, printer: 'p', label: 'Printer', mode: 'single', group: 'filament', gap, margin: 5, turn: true, bed: [180, 180], slots: 1, view: 0, slotCheck: { ok: true, need: 1, have: 1 },
  layout: [{ plate: 1, filaments: [], pieces: Array.from({ length: pieces }, (_, i) => ({ id: `p${i}`, name: 'rock', kind: 0, pinned: false })) }],
  kinds: [{ name: 'rock', exact: true, count: pieces, want: pieces, have: 0, otherColour: 0, seedUsed: 7, system: 'db' }],
  pieces: [{ id: 'p0', tile: 'lib:tiles/rock', count: pieces, print: 'missing' }], held: [], checkInventory: true,
});

function world() {
  const p = signal(plan(10));
  const ui = signal({ exporting: { busy: false, files: null, error: null, failures: [] }, after: { open: false, plate: 1, bad: [], busy: false } });
  const m = {
    on: signal(true), tile: signal({ id: 'l1', path: '/x/l.dmap', status: 'ok', errors: [] }), plan: p, ui: computed(() => ui.value), inventory: signal({ exists: false }),
    printers: signal([]), currentPrinter: signal('p'), readOnly: signal(false), name: signal('l'),
  };
  const calls = [];
  const act = new Proxy({ chooser: () => null, today: () => '2026-01-01' }, { get: (t, k) => t[k] ?? ((...a) => calls.push([k, ...a])) });
  render(html`<${PlatesPanel} m=${m} act=${act} />`, root);
  return { p, ui, m, calls };
}

test('the plates panel draws its sections from the plan and again when the plan changes', async () => {
  const { p } = world();
  await settle();
  assert.match(root.textContent, /Plate 1/);
  assert.equal(root.querySelectorAll('.pl-plate').length, 1);
  assert.match(root.textContent, /Spacing/);
  p.value = plan(10, 3);
  await settle();
  assert.match(root.textContent, /3 pieces/);
});

test('a gap being typed keeps its words and focus when the program tells a new plan, and leaving it commits through change', async () => {
  const { p, calls } = world();
  await settle();
  const input = root.querySelectorAll('input').find((i) => i.getAttribute('aria-label') === 'Between pieces, millimetres');
  assert.ok(input, 'the gap field');
  assert.equal(input.value, '10');
  type(input, '7');
  p.value = plan(12); // the file changed under the typing
  await settle();
  assert.equal(root.querySelectorAll('input').find((i) => i.getAttribute('aria-label') === 'Between pieces, millimetres'), input, 'the same field');
  assert.equal(input.value, '7', 'what was typed is still there');
  input.dispatchEvent({ type: 'change', target: input });
  assert.deepEqual(calls.at(-1), ['set', 'gap', '7mm']);
  await settle();
  assert.equal(input.value, '12', 'committed: the field shows the plan again');
});

test('the sheet after a print is the model\'s: opened by an action, drawn from its state', async () => {
  const { ui, calls } = world();
  await settle();
  root.querySelectorAll('button').find((b) => /Record a plate/.test(b.textContent)).dispatchEvent({ type: 'click' });
  assert.equal(calls.at(-1)[0], 'openSheet');
  ui.value = { ...ui.value, after: { open: true, plate: 1, bad: [0], busy: false } };
  await settle();
  assert.equal(root.querySelectorAll('.pl-piece').length, 2);
  assert.match(root.querySelector('.bad').textContent, /failed/);
  assert.match(root.textContent, /Add 1 to the inventory · 1 failed/);
  void press;
});
