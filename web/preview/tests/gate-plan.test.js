// The gate's verdict as a picture (D30): the underside plan is of the program's own flags (the painted faces), seen from below,
// and the record shows it beside the two bars.
import test from 'node:test';
import assert from 'node:assert/strict';
import { computed, signal } from '@preact/signals-core';
import { install, settle } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { MessagesBody } = await import('../js/view/messages-panel.js');
const { createLog } = await import('../js/messages.js');
const { undersidePlan, flaggedTriangles } = await import('../js/payload.js');

test('the underside plan is the flagged faces seen from below, red and orange apart, and nothing when none are flagged', () => {
  const p = {
    triangles: 3, meta: { lo: [0, 0, 0], hi: [10, 6, 4] },
    positions: new Float32Array([0, 0, 1, 4, 0, 1, 0, 4, 1, 6, 2, 1, 8, 2, 1, 6, 4, 1, 1, 1, 2, 2, 1, 2, 1, 2, 2]),
    clean: new Uint8Array(27).fill(100), painted: new Uint8Array(27).fill(100),
  };
  assert.equal(undersidePlan(p), null, 'nothing flagged, nothing drawn');
  p.painted.set([255, 140, 26], 0); // orange
  p.painted.set([230, 30, 30], 9); // red
  assert.deepEqual([...flaggedTriangles(p)], [{ t: 0, red: false }, { t: 1, red: true }]);
  const plan = undersidePlan(p);
  assert.equal(plan.width, 10);
  assert.equal(plan.height, 6);
  assert.deepEqual([plan.oranges, plan.reds], [1, 1]);
  // x is mirrored (the tile is seen from below) and y runs down the page: the corner at x 0, y 0 is at (10, 6)
  assert.equal(plan.orange, 'M10 6L6 6L10 2Z');
  assert.equal(plan.red, 'M4 4L2 4L4 2Z');
  assert.equal(undersidePlan({ ...p, meta: {} }), null, 'no outline, no plan');
});

const base = { t: '2026-11-13T10:00:00.000Z', tile: 'a', tile_name: 'bad', node: null, span: null, help: null, fix: null, state: 'open', scope: 'build' };

test('a record with a gate verdict shows the underside plan beside the bars when there are flagged faces, and none otherwise', async () => {
  const log = createLog();
  log.apply({ ...base, seq: 5, kind: 'build', severity: 'info', code: null, text: 'Built in 2 s', data: { build_s: 2, target: 'db' } });
  log.apply({
    ...base, seq: 6, kind: 'gate', severity: 'error', code: 'gate', text: 'Fails the printability gate: overhang',
    data: { triangles: 10, watertight: true, band_mm2: 12, band_budget_mm2: 2, overhang_mm2: 8, overhang_budget_mm2: 2, dropped: 0, failures: ['too much'] },
  });
  const m = {
    open: signal(true), view: computed(() => log.view('a')), filter: signal('all'), lit: signal(null), clearLit() {},
    tile: signal({ id: 'a', path: 'bad.dtile', name: 'bad', status: 'ok', errors: [], record: { record: { seed: 1 } } }), progress: signal(null),
    angles: signal({ overhangDeg: 55, steepestDeg: 65 }), shown: signal(0), file: signal('bad.dtile'), text: signal(''),
  };
  const plan = { width: 10, height: 6, red: 'M1 1L2 1L1 2Z', orange: 'M3 3L4 3L3 4Z', reds: 1, oranges: 1 };
  const acts = (underside) => new Proxy({ underside, canShowWorst: () => false }, { get: (t, k) => t[k] ?? (() => {}) });
  render(html`<${MessagesBody} m=${m} act=${acts(() => plan)} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('.underside').length, 1);
  assert.equal(root.querySelectorAll('.u-over').length, 1);
  assert.equal(root.querySelectorAll('.u-band').length, 1);
  assert.match(root.querySelector('.underside').textContent, /past 65°.*55 to 65°/);
  render(html`<${MessagesBody} m=${m} act=${acts(() => null)} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('.underside').length, 0, 'no flagged face, no picture');
});
