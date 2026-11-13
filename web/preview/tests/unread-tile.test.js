// A tile that is open and could not be read is not "No tile": the left panel says so, with its first mistake (the one record
// the view's card reads, `firstError`) and a way to its line.  `tileLevelState` is the one decision, and no source may name
// "No tile" except through it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { signal } from '@preact/signals-core';
import { install, settle } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { Unread, ErrorCard } = await import('../js/view/messages-panel.js');
const { PlatesPanel } = await import('../js/view/plates-panel.js');
const { firstError, tileLevelState } = await import('../js/messages.js');

const mistake = { code: 'dt::unknown-control', message: 'there is no control `colour` on rock', file: '/t/a.dtile', line: 4, column: 3, help: 'did you mean `color`?', unexpected: false };
const failed = { id: 'a', path: '/t/a.dtile', status: 'failed', errors: [mistake], failure: null };
const none = { rows: [] };

test('a tile that is open is never "none", whatever state it is in', () => {
  for (const status of ['idle', 'building', 'ready', 'failed']) {
    const t = { ...failed, status, errors: status === 'failed' ? [mistake] : [] };
    assert.notEqual(tileLevelState(t, null, firstError(t, none)), 'none', status);
  }
  assert.equal(tileLevelState(null, null, null), 'none');
  assert.equal(tileLevelState(failed, null, firstError(failed, none)), 'unread');
  assert.equal(tileLevelState(failed, { title: 'x' }, null), 'sheet');
  assert.equal(tileLevelState({ ...failed, status: 'building', errors: [] }, null, null), 'reading');
});

test("the first mistake is the stream's first open error, else the tile's own errors, else its failure", () => {
  const row = { m: { severity: 'error', text: 'from the stream', code: 'dt::x', span: { file: '/t/a.dtile', line: 2 } } };
  assert.equal(firstError(failed, { rows: [row] }).text, 'from the stream');
  const e = firstError(failed, none);
  assert.deepEqual([e.code, e.text, e.span.line, e.help], ['dt::unknown-control', mistake.message, 4, mistake.help]);
  const bug = firstError({ ...failed, errors: [], failure: { kind: 'bug', message: 'it panicked', help: null } }, none);
  assert.equal(bug.text, 'it panicked');
  assert.equal(firstError({ ...failed, errors: [] }, none), null);
  assert.equal(firstError({ ...failed, status: 'ready' }, none), null);
});

test("the left panel and the view's card say the same first mistake, from the one record", async () => {
  const first = firstError(failed, none);
  const lines = [];
  render(html`<${Unread} name="a" first=${first} go=${() => lines.push(first.span.line)} openPanel=${() => {}} />`, root);
  await settle();
  const text = root.textContent;
  assert.doesNotMatch(text, /No tile/);
  assert.match(text, /a could not be read/);
  assert.match(text, /dt::unknown-control/);
  assert.match(text, /there is no control/);
  assert.match(text, /line 4/);
  const go = root.querySelectorAll('button').find((b) => /Go to line/.test(b.textContent));
  assert.ok(go, 'a Go to line link');
  go.dispatchEvent({ type: 'click' });
  assert.deepEqual(lines, [4]);

  const m = { tile: signal(failed), view: signal({ rows: [], running: false }) };
  render(html`<${ErrorCard} m=${m} act=${{ showLine() {}, openPanel() {} }} />`, root);
  await settle();
  assert.match(root.textContent, /there is no control/, 'the card reads the same record');
});

test('a mistake with no line has no Go to line, and a tile with no reason says so rather than nothing', async () => {
  render(html`<${Unread} name="a" first=${{ code: 'bug', text: 'it panicked', span: null }} go=${null} openPanel=${() => {}} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('button').filter((b) => /Go to line/.test(b.textContent)).length, 0);
  render(html`<${Unread} name="a" first=${null} go=${null} openPanel=${() => {}} />`, root);
  await settle();
  assert.match(root.textContent, /gave no reason/);
});

test('the plates panel of a list that does not plan says why, with no plan back', async () => {
  const m = { tile: signal(failed), on: signal(true), plan: signal(null), readOnly: signal(false) };
  render(html`<${PlatesPanel} m=${m} act=${{}} />`, root);
  await settle();
  assert.match(root.textContent, /does not plan/);
  assert.match(root.textContent, /there is no control/);
});

test('only the tile level decides "No tile": no other source file may say it', () => {
  for (const d of ['../js', '../js/view', '../js/editor']) {
    for (const f of readdirSync(new URL(`${d}/`, import.meta.url)).filter((x) => x.endsWith('.js'))) {
      const src = readFileSync(new URL(`${d}/${f}`, import.meta.url), 'utf8');
      if (/emptyTitle: 'No tile'/.test(src)) assert.equal(`${d}/${f}`, '../js/editor/sheet.js');
    }
  }
  const sheet = readFileSync(new URL('../js/editor/sheet.js', import.meta.url), 'utf8');
  assert.match(sheet, /tileLevelState\(/, 'the tile level asks tileLevelState');
});
