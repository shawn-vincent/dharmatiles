// Ink and Order, phase 4: a fact is said the way a person says it, once, in one function (D24, D25, D30).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { dateWords, pieceLine } from '../js/inventory.js';
import { gaugeOf } from '../js/messages.js';
import { fitWords, squaresOf } from '../js/printers.js';

const read = (p) => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');

test('a date is today, day and month this year, with the year before', () => {
  const now = new Date(2026, 10, 13);
  assert.equal(dateWords('2026-11-13', now), 'today');
  assert.equal(dateWords('2026-03-02', now), '2 Mar');
  assert.equal(dateWords('2025-11-13', now), '13 Nov 2025');
  assert.equal(dateWords('not a date', now), 'not a date');
});

test('a piece is a sentence: the count, the date, the colour, the seed', () => {
  const today = new Date().toISOString().slice(0, 10);
  assert.equal(pieceLine({ have: 1, last: today, colour: 'single', seed: 9486 }), 'On the shelf: 1 · printed today · single colour · seed 9486');
  assert.doesNotMatch(pieceLine({ have: 2, last: null, colour: 'multi', seed: 7 }), /×|printed/);
});

test('a printer is the squares that fit its bed, for the system chosen', () => {
  const p = { bed: [220, 220], slots: 1 };
  assert.deepEqual(squaresOf(p, 35), [6, 6]);
  assert.deepEqual(squaresOf(p, 25.4), [8, 8]);
  assert.equal(fitWords(p, 35), '6 × 6 squares · 1 colour · 220 × 220 mm');
  assert.equal(squaresOf({ bed: null }, 35), null);
});

test('the gate gauge is full at the budget and says the multiple past it', () => {
  assert.deepEqual(gaugeOf(0, 2.07), { used: 0, budget: 2.07, fill: 0, over: null, tone: 'ok' });
  assert.equal(gaugeOf(1, 2).fill, 0.5);
  assert.equal(gaugeOf(1, 2).tone, 'warn');
  const g = gaugeOf(99.51, 6.09);
  assert.equal(g.over, '16×');
  assert.equal(g.fill, 1);
  assert.equal(g.tone, 'bad');
  assert.equal(gaugeOf(null, 1), null);
});

test('no second path: the inventory has no count of its own, and a message has no Go to line button', () => {
  assert.doesNotMatch(read('js/view/inventory-panel.js'), /inv-count|`×\$\{/);
  assert.doesNotMatch(read('js/editor/inventory.js'), /nothing is edited away/);
  const m = read('js/view/messages-panel.js');
  assert.equal((m.match(/>Go to line</g) ?? []).length, 1, 'only the error card on the view has the button; a row is pressed');
});
