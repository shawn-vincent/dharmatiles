// Lengths as written and as printed (docs/preview.html, "The property sheet"): the sheet shows what is written, with its unit
// and the program's words for it; beside it, quieter, what it prints on the build target, which the program works out; a
// printed size is typed in and the program writes it back in the value's own unit.  And the page does no unit arithmetic.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { signal } from '@preact/signals-core';
import { install, press, rendered, type, until } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { Sheet } = await import('../js/view/sheet.js');
const { setUnits } = await import('../js/value.js');
setUnits(['sq', 'mm', 'in', 'dmm', 'layer', 'deg', 'rad', 'pi']);

const ABOUT = { dmm: 'design millimetre: 1/35 of a square; 1 mm on DungeonBlocks, 0.73 mm on OpenLOCK', mm: 'a printed millimetre' };
const schema = { name: 'rock', controls: [
  { name: 'size', unit: 'dmm', numeric: true, ranges: true, default: '8' , default1: '8dmm', about: 'how big' },
  { name: 'count', unit: 'count', numeric: true, about: 'how many' },
  { name: 'top', unit: 'height', numeric: true },
] };
const model = (rows) => ({ title: 'rock', kind: 'rock', groups: [{ at: 10, schema, written: rows.map(([name, text, span]) => ({ name, text, span })) }] });

function world(rows, printedAnswer) {
  const m = signal(model(rows));
  const calls = [];
  const api = {
    writable: () => true, canMine: () => false,
    replace: (...a) => calls.push(['replace', ...a]), set: (...a) => calls.push(['set', ...a]),
    onField: () => {}, checkValue: async () => null,
    unitAbout: () => ABOUT,
    printed: async (items) => { calls.push(['printed', items]); return items.map((i) => printedAnswer[i.control] ?? null); },
    printedKey: () => 'ol|default',
    setPrinted: async (body) => { calls.push(['setPrinted', body]); return '6.89dmm'; },
  };
  render(html`<${Sheet} model=${m.value} api=${api} />`, root);
  return { calls };
}
const ROWS = [['size', 'size="7dmm"', [20, 12]], ['count', 'count=0', [40, 7]], ['top', 'top="$groundHeight + 4dmm"', [50, 30]]];

test('a length shows as written, its unit says in a tooltip what it is (the program\'s words), and a range reads with an en dash', async () => {
  world([['size', 'size="3.5dmm .. 6.5dmm"', [20, 24]]], {});
  await until(() => root.querySelector('.val'), 'the value drawn');
  const val = root.querySelector('.val');
  assert.equal(val.textContent.replace(/"/g, ''), '3.5–6.5dmm', 'the quotes are in the source and hidden');
  const u = root.querySelectorAll('.u');
  assert.equal(u.length, 1, 'the unit is said once');
  assert.equal(u[0].title || u[0].getAttribute('title'), ABOUT.dmm);
});

test('a scrubbed range still writes the source form, with `..`', async () => {
  const { calls } = world([['size', 'size="3.5dmm .. 6.5dmm"', [20, 24]]], {});
  await until(() => root.querySelectorAll('.num')[0], 'the number drawn');
  const num = root.querySelectorAll('.num')[0];
  num.dispatchEvent({ type: 'pointerdown', button: 0, clientX: 100, pointerId: 1, preventDefault() {} });
  num.dispatchEvent({ type: 'pointermove', clientX: 100 + 40, shiftKey: false });
  num.dispatchEvent({ type: 'pointerup' });
  const w = calls.filter((c) => c[0] === 'replace').at(-1);
  assert.match(w[3], /^"?[\d.]+dmm \.\. 6\.5dmm"?$/);
});

test('a count of nothing reads "none"', async () => {
  world([['count', 'count=0', [20, 7]]], {}); // the sheet's second row
  await until(() => root.querySelectorAll('.row')[1]?.querySelector('.val'), 'the count row drawn');
  assert.equal(root.querySelectorAll('.row')[1].querySelector('.val').textContent, 'none');
});

test('the printed size sits beside the value, asked of the program for each length; the page works nothing out', async () => {
  const { calls } = world(ROWS, { size: { ends: ['5.08'], editable: true }, top: { ends: ['6.53'], editable: false } });
  // the ask is made from an effect (after the paint) and its answer comes back later: wait for the printed sizes drawn
  await until(() => root.querySelectorAll('.printed').length === 2, 'both printed sizes drawn');
  const asked = calls.find((c) => c[0] === 'printed')[1];
  assert.deepEqual(asked.map((a) => [a.element, a.control, a.typed]), [['rock', 'size', '7dmm'], ['rock', 'top', '$groundHeight + 4dmm']]);
  const rows = root.querySelectorAll('.row');
  assert.match(rows[0].querySelector('.printed').textContent, /prints 5\.08 mm/);
  assert.equal(rows[1].querySelector('.printed'), null, 'a count has none');
  assert.match(rows[2].querySelector('.printed').textContent, /prints 6\.53 mm/);
  assert.equal(rows[2].querySelector('.pnum'), null, 'a formula only shows it');
  assert.ok(rows[0].querySelector('.pnum'), 'a number can be typed');
});

test('a printed size typed in is converted by the program and written as the new value, one step of history', async () => {
  const { calls } = world(ROWS, { size: { ends: ['5.08'], editable: true } });
  await until(() => root.querySelector('.pnum'), 'the printed size drawn');
  root.querySelector('.pnum').dispatchEvent({ type: 'click' });
  await until(() => root.querySelector('.pedit'), 'the field for the printed number drawn');
  const input = root.querySelector('.pedit');
  assert.ok(input, 'a field for the printed number');
  type(input, '5');
  await rendered(); // the typed text is state: drawn before Enter reads it
  press(input, 'Enter');
  await until(() => calls.some((c) => c[0] === 'replace'), 'the program\'s conversion written as the new value');
  assert.deepEqual(calls.find((c) => c[0] === 'setPrinted')[1], { typed: '7dmm', end: 0, mm: 5 });
  const w = calls.filter((c) => c[0] === 'replace').at(-1);
  assert.equal(w[3], '"6.89dmm"', 'written in the value\'s own unit, as the program said');
  assert.equal(w[4], true);
});

test('what is not a number is not sent, and the field says so', async () => {
  const { calls } = world(ROWS, { size: { ends: ['5.08'], editable: true } });
  await until(() => root.querySelector('.pnum'), 'the printed size drawn');
  root.querySelector('.pnum').dispatchEvent({ type: 'click' });
  await until(() => root.querySelector('.pedit'), 'the field for the printed number drawn');
  const input = root.querySelector('.pedit');
  type(input, '5 dmm');
  await rendered();
  press(input, 'Enter');
  // nothing is sent, so there is no call to wait for: wait for the field to be marked
  await until(() => { const f = root.querySelector('.pedit'); return f && (f.className || f.getAttribute('class') || '').includes('bad'); }, 'the field marked bad');
  assert.equal(calls.find((c) => c[0] === 'setPrinted'), undefined);
  const f = root.querySelector('.pedit');
  assert.ok((f.className || f.getAttribute('class')).includes('bad'));
});

// ---- the page does no unit arithmetic --------------------------------------------------------------------------

const files = (dir) => readdirSync(dir).flatMap((f) => {
  const p = join(dir, f);
  if (f === 'vendor') return [];
  return statSync(p).isDirectory() ? files(p) : p.endsWith('.js') ? [p] : [];
});
const code = (text) => text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:'"`])\/\/.*$/gm, '$1');

test('no part of the page multiplies or divides by 35, 25.4 or a dmm scale, or names a system\'s square: the square is the program\'s (plan.squareMm, the scene\'s squareMm), and a printed size is the program\'s', () => {
  const here = new URL('../js', import.meta.url).pathname;
  const bad = [];
  for (const f of files(here)) {
    const src = code(readFileSync(f, 'utf8'));
    for (const [n, line] of src.split('\n').entries()) {
      const arithmetic = /[*/]\s*\(?\s*(?<![\d.])(35|25\.4)(?![\d.])/.test(line) || /(?<![\d.])(35|25\.4)(?![\d.])\s*\)?\s*[*/](?!\/)/.test(line);
      const square = /(?<![\d.])25\.4(?![\d.])/.test(line) || /system\s*===?\s*['"]ol['"]\s*\?/.test(line);
      const scale = /\b(dmmScale|dmmToMm|mmToDmm|dmm\s*[*/]|[*/]\s*dmm)\b/.test(line);
      if (arithmetic || square || scale) bad.push(`${f.slice(here.length + 1)}:${n + 1}: ${line.trim()}`);
    }
  }
  assert.deepEqual(bad, []);
});
