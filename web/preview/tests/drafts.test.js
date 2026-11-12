import test from 'node:test';
import assert from 'node:assert/strict';

import { createDrafter } from '../js/drafts.js';

// a request that answers only when the test lets it
function rig() {
  let text = 'v1';
  const sent = [], shown = [], waiting = [];
  const drafter = createDrafter({
    text: () => text,
    request: (t) => { sent.push(t); return new Promise((resolve) => waiting.push(() => resolve(`draft of ${t}`))); },
    accept: (reply) => shown.push(reply),
    fail: (e) => { throw e; },
  });
  return { drafter, sent, shown, waiting, setText: (t) => { text = t; } };
}
const tick = () => new Promise((r) => setImmediate(r));

test('a draft is shown when the text is still the text it was built from', async () => {
  const r = rig();
  r.drafter.ask();
  r.waiting.shift()();
  await tick();
  assert.deepEqual(r.shown, ['draft of v1']);
});

test('a draft for text that has changed since is dropped, and the newer text is asked for', async () => {
  const r = rig();
  r.drafter.ask();
  r.setText('v2');
  r.drafter.ask(); // asked while the first is in flight: once it returns, again
  r.waiting.shift()(); // the draft of v1 comes back
  await tick();
  assert.deepEqual(r.shown, []);
  assert.deepEqual(r.sent, ['v1', 'v2']);
  r.waiting.shift()();
  await tick();
  assert.deepEqual(r.shown, ['draft of v2']);
});

test('a draft in flight when a full build of the text lands is dropped', async () => {
  const r = rig();
  r.drafter.ask();
  r.drafter.supersede();
  r.waiting.shift()();
  await tick();
  assert.deepEqual(r.shown, []);
  // the next draft is shown again
  r.drafter.ask();
  r.waiting.shift()();
  await tick();
  assert.deepEqual(r.shown, ['draft of v1']);
});

test('nothing to draft asks for nothing', async () => {
  const sent = [];
  const d = createDrafter({ text: () => null, request: (t) => sent.push(t), accept: () => {}, fail: () => {} });
  await d.ask();
  assert.deepEqual(sent, []);
});
