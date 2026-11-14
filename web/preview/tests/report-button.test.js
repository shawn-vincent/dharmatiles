// Report this defect: the button is on a message that means the program went wrong (the program says so: `reportable`) and
// never on a mistake in the maker's document; its preview shows the program's report, a checkbox leaves the source out, and
// Open GitHub issue opens the address the program made.  When the address is shortened the whole report is copied first, and a
// refusal of the clipboard is said.
import test from 'node:test';
import assert from 'node:assert/strict';
import { computed, signal } from '@preact/signals-core';
import { install, rendered, until } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { MessagesBody } = await import('../js/view/messages-panel.js');
const { createLog } = await import('../js/messages.js');
const { canReport } = await import('../js/view/report-button.js');

const message = (over = {}) => ({
  seq: 4, t: '2026-11-14T10:00:00.000Z', kind: 'server', severity: 'error', scope: 'server', code: 'bug', tile: 'a', tile_name: 'bad',
  node: null, span: null, text: 'the kernel gave up', help: null, fix: null, data: { origin: 'server', kind: 'bug' }, state: 'open', reportable: true, ...over,
});
const mistake = () => message({ seq: 5, kind: 'error', code: 'dt::unknown-element', reportable: false, data: {} });

function world(...msgs) {
  const log = createLog();
  for (const x of msgs) log.apply(x);
  const m = {
    open: signal(true), view: computed(() => log.view('a')), filter: signal('all'), lit: signal(null), clearLit() {},
    tile: signal({ id: 'a', path: 'bad.dtile', name: 'bad', status: 'failed', errors: [] }), progress: signal(null), angles: signal({ overhangDeg: 55, steepestDeg: 65 }),
    shown: signal(0), file: signal('bad.dtile'), text: signal('x'),
  };
  const asked = [], opened = [];
  const rep = { url: 'https://github.com/x/issues/new?title=t', title: 'bug: the kernel gave up', labels: ['defect'], body: 'the kernel gave up\n\nEnvironment', truncated: false };
  const act = new Proxy({
    report: async (seq, leaveOut) => { asked.push([seq, leaveOut]); return { ...rep, ...(globalThis.__truncated ? { truncated: true } : {}) }; },
    openIssue: (url) => opened.push(url),
  }, { get: (t, k) => t[k] ?? (() => {}) });
  return { m, act, asked, opened };
}

const buttons = () => root.querySelectorAll('button').filter((b) => b.textContent === 'Report this defect');

test('only a message that means the program went wrong gets the button', async () => {
  assert.equal(canReport(message()), true);
  assert.equal(canReport(mistake()), false, 'a mistake in the document is the maker\'s own');
  assert.equal(canReport(message({ seq: -1 })), false, 'a message of the page\'s own has no number to ask the program about');
  const { m, act } = world(message(), mistake());
  render(html`<${MessagesBody} m=${m} act=${act} />`, root);
  await until(() => buttons().length > 0, 'the Report this defect button');
  await rendered(); // the mistake's row is drawn with it: both are one render
  assert.equal(buttons().length, 1, 'one button, on the bug and not on the mistake');
});

test('the preview shows the report, the checkbox leaves the source out, and the button opens the issue', async () => {
  const { m, act, asked, opened } = world(message());
  render(html`<${MessagesBody} m=${m} act=${act} />`, root);
  await until(() => buttons().length === 1, 'the Report this defect button');
  buttons()[0].dispatchEvent({ type: 'click' });
  await until(() => root.querySelector('.report-body'), 'the report in the preview');
  assert.deepEqual(asked, [[4, false]]);
  assert.ok(root.querySelector('.report-body').textContent.includes('the kernel gave up'));
  const box = root.querySelectorAll('input')[0];
  box.checked = true;
  box.dispatchEvent({ type: 'change', currentTarget: box, target: box });
  await until(() => asked.length === 2, 'the report asked for again without the source');
  assert.deepEqual(asked.at(-1), [4, true], 'asked again without the source');
  root.querySelectorAll('button').find((b) => b.textContent === 'Open GitHub issue').dispatchEvent({ type: 'click' });
  await until(() => opened.length === 1, 'the issue opened');
  assert.deepEqual(opened, ['https://github.com/x/issues/new?title=t']);
});

test('a shortened address copies the whole report first, and a refused clipboard is said', async () => {
  globalThis.__truncated = true;
  const wrote = [];
  Object.defineProperty(globalThis, 'navigator', { value: { clipboard: { writeText: async (t) => { wrote.push(t); } } }, configurable: true });
  const w = world(message());
  render(null, root); // the last test's row, and its open preview, are gone
  render(html`<${MessagesBody} m=${w.m} act=${w.act} />`, root);
  await until(() => buttons().length === 1, 'the Report this defect button');
  buttons()[0].dispatchEvent({ type: 'click' });
  await until(() => root.querySelector('.report-body'), 'the report in the preview');
  root.querySelectorAll('button').find((b) => b.textContent === 'Open GitHub issue').dispatchEvent({ type: 'click' });
  await until(() => w.opened.length === 1, 'the issue opened');
  assert.deepEqual(wrote, ['the kernel gave up\n\nEnvironment']);
  assert.ok(root.textContent.includes('on your clipboard'));

  Object.defineProperty(globalThis, 'navigator', { value: { clipboard: { writeText: async () => { throw new Error('denied'); } } }, configurable: true });
  root.querySelectorAll('button').find((b) => b.textContent === 'Open GitHub issue').dispatchEvent({ type: 'click' });
  await until(() => root.textContent.includes('Could not copy the report (denied)'), 'the refusal said');
  globalThis.__truncated = false;
});
