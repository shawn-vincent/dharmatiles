// An edit names a node by an id made from the text (`rock#2`), and the id means what it meant only in the text it was read from:
// the page sends the revision of the check the ids come from (`nodesRev`), and when the program refuses it (409: the text moved
// on) the page checks again, so the next try names the right node.
import test, { mock } from 'node:test';
import assert from 'node:assert/strict';
import { install, until } from './dom.js';

const sent = [], checks = [];
let answer = { status: 200, body: { text: 'edited' } };
const host = {
  edit: async (b) => { sent.push(b); return answer; },
  check: async (id, b) => { checks.push([id, b]); return { status: 200, body: { ok: true, errors: [], nodes: [], nodesRev: 'rev-2' } }; },
  names: async () => ({ status: 200, body: { ok: false } }),
};
mock.module(new URL('../js/editor/app.js', import.meta.url).href, { namedExports: { $: () => null, host, viewer: {}, toast: () => {}, pickView: () => {}, onMakeMine: () => {}, makeMine: () => false } });
// the text pane is a CodeMirror editor, which needs a browser: a stand-in that holds the text is enough here
mock.module(new URL('../js/text.js', import.meta.url).href, {
  namedExports: { TextPane: class { caret() { return { before: '' }; } reveal() {} set() {} }, byteToIndex: (t, b) => b },
});
install();
const { editCall } = await import('../js/editor/document.js');
const { doc, server, view } = await import('../js/editor/stores.js');

server.set({ tiles: [{ id: 't1', path: '/home/t.dtile', name: 't', status: 'ok', errors: [], root: 0 }], ready: true });
view.set({ tileId: 't1' });
doc.set({ tileId: 't1', file: '/home/t.dtile', text: 'tile {}', writable: true, nodes: [], nodesRev: 'rev-1' });

test('an edit carries the revision its ids were read at, with the text it is made to', async () => {
  const r = await editCall({ op: 'hide', id: 'tile/stack/rock#2' });
  assert.equal(r.status, 200);
  assert.deepEqual(sent.at(-1), { op: 'hide', id: 'tile/stack/rock#2', text: 'tile {}', read: 'rev-1' });
  assert.equal(checks.length, 0, 'nothing to check again when the edit was made');
});

test('an edit the program refuses because the text moved on is answered by a new check, and the revision it brings', async () => {
  answer = { status: 409, body: { error: 'The text changed since the program listed its nodes' } };
  const r = await editCall({ op: 'hide', id: 'tile/stack/rock#2' });
  assert.equal(r.status, 409, 'the refusal is the caller\'s to show');
  await until(() => doc.get().nodesRev === 'rev-2', 'the check that follows the refusal');
  assert.equal(checks.length, 1, 'the text is checked again, once');
});
