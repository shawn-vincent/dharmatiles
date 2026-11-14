// A message that carries the program's fix has a button that makes it (D26): the row, the view's error card and the text of
// the fix are one path.  The program writes the replacement and its words once (dt-doc's `suggest`); the page lays it in.
import test from 'node:test';
import assert from 'node:assert/strict';
import { computed, signal } from '@preact/signals-core';
import { install, settle } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { ErrorCard, MessagesBody } = await import('../js/view/messages-panel.js');
const { createLog } = await import('../js/messages.js');
const { fixedText } = await import('../js/spans.js');

const TEXT = 'tile size="1x1" {\n  stack {\n    dirtt\n  }\n}\n';
const at = TEXT.indexOf('dirtt');
const fix = { offset: at, len: 5, was: 'dirtt', replacement: 'dirt', label: 'Change to dirt' };
const error = (over = {}) => ({
  seq: 1, t: '2026-11-13T10:00:00.000Z', kind: 'error', severity: 'error', scope: 'check', code: 'dt::unknown-element', tile: 'a', tile_name: 'bad',
  node: null, span: { file: 'bad.dtile', line: 3, col: 5, offset: at, len: 5 }, text: 'no element or definition is called `dirtt`', help: 'did you mean `dirt`?',
  fix, data: {}, state: 'open', ...over,
});

test('the fix lays its replacement over the text it was made for, and only that text', () => {
  assert.equal(fixedText(TEXT, fix), TEXT.replace('dirtt', 'dirt'));
  assert.equal(fixedText(TEXT.replace('dirtt', 'rocks'), fix), null, 'the place no longer reads what the program saw');
  // offsets are bytes: a multi-byte character before the word moves the index, not the byte
  const wide = `// é\n${TEXT}`;
  assert.equal(fixedText(wide, { ...fix, offset: at + 6 }), wide.replace('dirtt', 'dirt'));
});

function world(msg) {
  const log = createLog();
  log.apply(msg);
  const calls = [];
  const m = {
    open: signal(true), view: computed(() => log.view('a')), filter: signal('all'), lit: signal(null), clearLit() {},
    tile: signal({ id: 'a', path: 'bad.dtile', name: 'bad', status: 'failed', errors: [] }), progress: signal(null), angles: signal({ overhangDeg: 55, steepestDeg: 65 }),
    shown: signal(0), file: signal('bad.dtile'), text: signal(TEXT),
  };
  const act = new Proxy({}, { get: (t, k) => (...a) => calls.push([k, ...a]) });
  return { m, act, calls };
}

test('a row whose message carries a fix shows its button, which applies that fix', async () => {
  const { m, act, calls } = world(error());
  render(html`<${MessagesBody} m=${m} act=${act} />`, root);
  await settle();
  const b = root.querySelectorAll('button').find((x) => x.textContent === 'Change to dirt');
  assert.ok(b, 'the button says what it does');
  b.dispatchEvent({ type: 'click' });
  assert.equal(calls.length, 1);
  assert.equal(calls[0][0], 'applyFix');
  assert.deepEqual(calls[0][1].fix, fix);
});

test('a message with no fix has no button, and the view\'s card offers the first error\'s fix', async () => {
  const none = world(error({ fix: null, help: 'a definition from another file needs `use`' }));
  render(html`<${MessagesBody} m=${none.m} act=${none.act} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('button').filter((x) => (x.className || '').split(/\s+/).includes('fix')).length, 0);

  const card = world(error());
  render(html`<${ErrorCard} m=${card.m} act=${card.act} />`, root);
  await settle();
  const b = root.querySelectorAll('button').find((x) => x.textContent === 'Change to dirt');
  assert.ok(b, 'the card has the button');
  b.dispatchEvent({ type: 'click' });
  assert.equal(card.calls[0][0], 'applyFix');
});
