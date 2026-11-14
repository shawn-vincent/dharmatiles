// A change at pointer speed (a map handle dragged, a value scrubbed or typed) is a run of steps the program makes: each step is one
// request that edits the text and drafts what it made, one in the air at a time, the newest wanted value winning; a refusal for a
// stale step is answered by a new check; and the whole change is one step of the history.  The page never makes the text.
import test, { mock } from 'node:test';
import assert from 'node:assert/strict';
import { install, until } from './dom.js';

// the program, played: a text is `v=N`; a step sets N and answers the new text (the test lets each answer go when it chooses)
const FILE = '/home/t.dtile';
const disk = { text: 'v=0', writable: true };
const sent = [], edits = [], checks = [], gates = [];
let refuse = null; // a status to answer the next step with
const mesh = () => {
  const json = new TextEncoder().encode(JSON.stringify({ triangles: 0 }).padEnd(16)); // a payload's own head is a multiple of four long
  const b = new ArrayBuffer(8 + json.length);
  new Uint8Array(b, 0, 4).set([...'DTM1'].map((c) => c.charCodeAt(0)));
  new DataView(b).setUint32(4, json.length, true);
  new Uint8Array(b, 8).set(json);
  return b;
};
const textOf = (n) => `v=${n}`;
const host = {
  step: (id, body) => new Promise((resolve) => {
    sent.push({ id, ...body });
    const answer = () => {
      if (refuse) { const status = refuse; refuse = null; return resolve({ status, body: { error: 'The text changed since the program listed its nodes' } }); }
      const text = textOf(body.edit.value);
      resolve({ status: 200, head: { text, id: body.edit.id, at: 0, rev: `rev:${text}`, draft: 'mesh' }, buffer: mesh() });
    };
    gates.push(answer);
  }),
  edit: async (b) => { edits.push(b); return { status: 200, body: { text: textOf(b.value), id: b.id, at: 0 } }; },
  check: async (id, b) => { checks.push(b.text); return { status: 200, body: { ok: true, errors: [], nodes: [], nodesRev: `rev:${b.text}` } }; },
  names: async () => ({ status: 200, body: { ok: false } }),
  text: async () => ({ file: FILE, text: disk.text, writable: disk.writable, docRev: 1 }),
  draft: async () => ({ status: 200, buffer: mesh() }),
  save: async () => ({ status: 200, body: {} }),
};
const mined = [];
mock.module(new URL('../js/editor/app.js', import.meta.url).href, { namedExports: { $: () => null, host, viewer: {}, toast: () => {}, pickView: () => {}, onMakeMine: () => {}, makeMine: (r) => { mined.push(r); return false; } } });
// the text pane is a CodeMirror editor, which needs a browser: a stand-in that holds the text is enough here
mock.module(new URL('../js/text.js', import.meta.url).href, {
  namedExports: {
    TextPane: class {
      text = ''; file = null; selectionStart = 0;
      show(file, text) { this.file = file; this.text = text; }
      caret() { return { before: '' }; } reveal() {} set() {} clear() { this.text = ''; }
      setText(t) { this.text = t; }
    },
    byteToIndex: (t, b) => b,
  },
});
install();
const { steps } = await import('../js/editor/document.js');
const { docs } = await import('../js/editor/docs.js');
const { loadText } = await import('../js/editor/document.js');
const { doc, server, view } = await import('../js/editor/stores.js');

// each test opens its own tile, so one's unsaved text is not another's
let n = 0, id = 't0';
const reset = async (text = textOf(0), writable = true) => {
  sent.length = 0; edits.length = 0; checks.length = 0; gates.length = 0; mined.length = 0; refuse = null;
  Object.assign(disk, { text, writable });
  id = `t${++n}`;
  server.set({ tiles: [{ id, path: FILE, name: 't', status: 'ok', errors: [], root: 0, meshVersion: 1 }], ready: true });
  view.set({ tileId: id, draft: null });
  await loadText(null);
  await until(() => doc.get().checked === text, 'the first check');
  checks.length = 0;
};
const make = (value) => ({ op: 'set', id: 'tile/stack/rock', name: 'seed', value });
const tick = () => new Promise((r) => setImmediate(r));
const answer = async () => { gates.shift()(); await tick(); await tick(); };
const entries = () => docs.history.entries(`text:${id}`).length;

test('a step is one request that carries the edit, the text it is made to and the revision its ids were read at', async () => {
  await reset();
  const s = steps('Changed seed', make);
  s.to(1);
  await tick();
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].edit, { op: 'set', id: 'tile/stack/rock', name: 'seed', value: 1, text: 'v=0', read: 'rev:v=0' });
  await answer();
  assert.equal(doc.get().text, 'v=1', 'the text is the program\'s answer');
  assert.ok(view.get().draft, 'and so is the draft, in the same reply');
  assert.equal(edits.length, 0, 'no second request for the draft');
  await s.end(1);
  assert.equal(edits.length, 0, 'the last answer made the value: nothing more to ask');
});

test('one request is in the air at a time and the newest value wins: a fast drag skips steps', async () => {
  await reset();
  const s = steps('Changed seed', make);
  s.to(1);
  await tick();
  s.to(2); s.to(3); s.to(4); // while the first is in the air
  await tick();
  assert.equal(sent.length, 1, 'nothing queues behind the one in the air');
  await answer(); // the answer for 1 lands, the newest (4) is sent at once, to the text 1 made
  await tick();
  assert.equal(sent.length, 2);
  assert.deepEqual(sent.map((x) => x.edit.value), [1, 4]);
  assert.equal(sent[1].edit.text, 'v=1');
  assert.equal(sent[1].edit.read, 'rev:v=1', 'at the revision the answer named, not the last check\'s');
  await answer();
  assert.equal(doc.get().text, 'v=4');
  await s.end(4);
});

test('a step the program refuses as stale is answered by a new check, and the next step names the nodes afresh', async () => {
  await reset();
  const s = steps('Changed seed', make);
  refuse = 409;
  s.to(1);
  await tick();
  await answer();
  await until(() => checks.length === 1, 'the check that follows the refusal');
  assert.equal(doc.get().text, 'v=0', 'a refused step changed nothing');
  s.to(2);
  await tick();
  assert.equal(sent.at(-1).edit.read, 'rev:v=0', 'the next step is at the revision the new check brought');
  await answer();
  assert.equal(doc.get().text, 'v=2');
  await s.end(2);
});

test('the whole change is one step of the history, whatever the number of steps', async () => {
  await reset();
  const before = entries();
  const s = steps('Changed seed', make);
  for (const v of [1, 2, 3]) { s.to(v); await tick(); await answer(); }
  assert.equal(entries(), before, 'the steps on the way are not steps of history');
  await s.end(3);
  assert.equal(entries(), before + 1);
  const last = docs.history.entries(`text:${id}`).at(-1);
  assert.equal(last.before, 'v=0');
  assert.equal(last.after, 'v=3');
});

test('a value that is not the last answer is made by an ordinary edit, at the last answer\'s revision', async () => {
  await reset();
  const s = steps('Changed seed', make);
  s.to(1); await tick(); await answer();
  await s.end(5); // let go somewhere else than the last step
  assert.equal(edits.length, 1);
  assert.equal(edits[0].value, 5);
  assert.equal(edits[0].read, 'rev:v=1');
  assert.equal(doc.get().text, 'v=5');
});

test('a change given up leaves the text as it was, and nothing to undo', async () => {
  await reset();
  const before = entries();
  const s = steps('Changed seed', make);
  s.to(1); await tick(); await answer();
  await s.cancel();
  assert.equal(doc.get().text, 'v=0');
  assert.equal(entries(), before);
});

test('a change that ends where it began is no step of history', async () => {
  await reset();
  const before = entries();
  const s = steps('Changed seed', make);
  s.to(1); await tick(); await answer();
  s.to(0); await tick(); await answer();
  await s.end(0);
  assert.equal(doc.get().text, 'v=0');
  assert.equal(entries(), before);
});

test('a text that has not been checked has no ids to name: no change starts, and nothing is sent', async () => {
  await reset();
  doc.set({ checked: 'something else' });
  assert.equal(steps('Changed seed', make), null);
  assert.equal(sent.length, 0);
});

test('a library tile is copied by the change, which the copy then gets once, as an ordinary edit', async () => {
  await reset(textOf(0), false);
  const s = steps('Changed seed', make);
  s.to(1); s.to(2);
  assert.equal(mined.length, 1, 'asked once');
  assert.equal(sent.length, 0, 'nothing is stepped in the library\'s text');
  await s.end(3);
  doc.set({ writable: true });
  await mined[0].run();
  assert.equal(edits.at(-1).value, 3, 'the copy gets the value the change ended at');
});
