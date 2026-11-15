// The keeper of kept builds (js/kept-idb.js) stores opaque records and gives them back: it makes no key, encodes nothing and
// decides no eviction (the engine's `KeptBuilds` does).  Driven with a small in-memory IndexedDB and a stand-in engine store.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openKept } from '../js/kept-idb.js';

/** The little of IndexedDB the keeper uses: one object store by `id`, requests that answer in a microtask, transactions that finish after their writes. */
function fakeIdb(initial = []) {
  const rows = new Map(initial.map((r) => [r.id, r]));
  const deleted = [];
  const request = (value) => { const req = {}; queueMicrotask(() => { req.result = value; req.onsuccess?.(); }); return req; };
  const store = () => ({
    getAll: () => request([...rows.values()]),
    put: (r) => { rows.set(r.id, r); },
    delete: (id) => { rows.delete(id); },
  });
  const db = { transaction: () => { const tx = {}; queueMicrotask(() => queueMicrotask(() => tx.oncomplete?.())); tx.objectStore = store; return tx; }, createObjectStore() {} };
  const idb = {
    open: () => { const req = /** @type {any} */ ({}); queueMicrotask(() => { req.result = db; req.onupgradeneeded?.(); req.onsuccess?.(); }); return req; },
    deleteDatabase: (name) => { deleted.push(name); },
  };
  return { idb: /** @type {any} */ (idb), rows, deleted };
}

/** An engine store that records what it is told, and says a record is damaged when its first byte is 0. */
function fakeKept() {
  const calls = [];
  return {
    calls,
    persist: () => calls.push('persist'),
    load: (bytes, used) => { calls.push(`load:${bytes[0]}:${used}`); return bytes[0] === 0 ? 'a kept build is damaged (test); it is dropped' : ''; },
  };
}

test('without IndexedDB the builds stay in memory, and nothing throws', async () => {
  const kept = fakeKept();
  const opened = await openKept(kept, { idb: undefined, say: () => assert.fail('nothing to say') });
  assert.equal(opened.persistent, false);
  assert.deepEqual(kept.calls, []);
});

test('an IndexedDB that will not open is said, and the builds stay in memory', async () => {
  const said = [];
  const idb = /** @type {any} */ ({ open: () => { const req = /** @type {any} */ ({}); queueMicrotask(() => { req.error = new Error('blocked'); req.onerror?.(); }); return req; } });
  const opened = await openKept(fakeKept(), { idb, say: (w) => said.push(w) });
  assert.equal(opened.persistent, false);
  assert.match(said[0], /not kept between visits.*blocked/);
});

test('the engine is told to persist before it loads, each record goes to it as it is, and a damaged one is deleted', async () => {
  const { idb, rows, deleted } = fakeIdb([
    { id: 'a', bytes: new Uint8Array([7]), used: 3 },
    { id: 'bad', bytes: new Uint8Array([0]), used: 4 },
  ]);
  const kept = fakeKept();
  const said = [];
  const opened = await openKept(kept, { idb, say: (w) => said.push(w) });
  await opened.flush();
  assert.equal(opened.persistent, true);
  assert.deepEqual(kept.calls, ['persist', 'load:7:3', 'load:0:4']);
  assert.deepEqual([...rows.keys()], ['a'], 'the damaged record is gone');
  assert.match(said[0], /damaged/);
  assert.deepEqual(deleted, ['dt-drafts'], 'the old draft store is removed');
});

test('what the engine keeps is written as an opaque copy, and what it drops is deleted', async () => {
  const { idb, rows } = fakeIdb();
  const opened = await openKept(fakeKept(), { idb, say: (w) => assert.fail(w) });
  const view = new Uint8Array([1, 2, 3]);
  /** @type {any} */ (globalThis).dtKeptPut('k1', view, 9);
  view[0] = 99; // the engine's memory is reused after the call: the record is a copy
  /** @type {any} */ (globalThis).dtKeptPut('k2', new Uint8Array([4]), 10);
  await opened.flush();
  assert.deepEqual([...rows.get('k1').bytes], [1, 2, 3]);
  assert.equal(rows.get('k1').used, 9);
  /** @type {any} */ (globalThis).dtKeptDrop('k1');
  await opened.flush();
  assert.deepEqual([...rows.keys()], ['k2']);
});
