// A message with a `dt::` code has an Explain link to that code's error page, served by the program from the docs it was installed
// with (never the online site, so the page matches the installed program).  A message with no code, or a code the program did not
// make, has none.
import test from 'node:test';
import assert from 'node:assert/strict';
import { computed, signal } from '@preact/signals-core';
import { install, settle } from './dom.js';

const root = install();
const { render, html } = await import('../js/view/html.js');
const { ErrorCard, MessagesBody } = await import('../js/view/messages-panel.js');
const { createLog, explainHref } = await import('../js/messages.js');

const error = (over = {}) => ({
  seq: 1, t: '2026-11-14T10:00:00.000Z', kind: 'error', severity: 'error', scope: 'check', code: 'dt::unknown-element', tile: 'a', tile_name: 'bad',
  node: null, span: { file: 'bad.dtile', line: 3, col: 5, offset: 0, len: 5 }, text: 'no element is called `dirtt`', help: null,
  fix: null, data: {}, state: 'open', ...over,
});

function world(msg) {
  const log = createLog();
  log.apply(msg);
  const m = {
    open: signal(true), view: computed(() => log.view('a')), filter: signal('all'), lit: signal(null), clearLit() {},
    tile: signal({ id: 'a', path: 'bad.dtile', name: 'bad', status: 'failed', errors: [] }), progress: signal(null), angles: signal({ overhangDeg: 55, steepestDeg: 65 }),
    shown: signal(0), file: signal('bad.dtile'), text: signal('x'),
  };
  return { m, act: new Proxy({}, { get: () => () => {} }) };
}

test('the address of a code is the local copy of its error page', () => {
  assert.equal(explainHref('dt::type'), '/docs/e/type.html');
  assert.equal(explainHref('dt::unknown-control'), '/docs/e/unknown-control.html');
  assert.equal(explainHref('failure'), null, 'a code the page made itself has no page');
  assert.equal(explainHref(null), null);
  assert.equal(explainHref('dt::../../x'), null, 'only a word and hyphens');
});

test('a row with a code links to its error page, and a row without one does not', async () => {
  const { m, act } = world(error());
  render(html`<${MessagesBody} m=${m} act=${act} />`, root);
  await settle();
  const a = root.querySelectorAll('a').find((x) => x.textContent === 'Explain');
  assert.ok(a, 'the Explain link');
  assert.equal(a.getAttribute('href'), '/docs/e/unknown-element.html');

  const none = world(error({ code: null }));
  render(html`<${MessagesBody} m=${none.m} act=${none.act} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('a').filter((x) => x.textContent === 'Explain').length, 0);
});

test('the error card of a tile that did not build links to the first error\'s page', async () => {
  const { m, act } = world(error());
  render(html`<${ErrorCard} m=${m} act=${act} />`, root);
  await settle();
  const a = root.querySelectorAll('a').find((x) => x.textContent === 'Explain');
  assert.ok(a, 'the card has the link');
  assert.equal(a.getAttribute('href'), '/docs/e/unknown-element.html');
});
