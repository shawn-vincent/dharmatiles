// The site's engine files through the Cache API (docs/assets/live/engine-files.js), driven with a stand-in `caches` and `fetch`:
// a copy kept for another manifest hash is never used, and a copy that cannot be kept is said, not swallowed.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cacheName, engineFile } from '../../../docs/assets/live/engine-files.js';

const URL_WASM = new URL('http://site.test/engine/dt_web_bg.aaaa.wasm');

/** A Cache API in memory; `putFails` makes `put` throw the way a full quota does. */
function fakeCaches({ putFails = false } = {}) {
  /** @type {Map<string, Map<string, Response>>} */
  const stores = new Map();
  return {
    stores,
    async open(name) {
      if (!stores.has(name)) stores.set(name, new Map());
      const s = stores.get(name);
      return {
        async match(url) { return s.get(String(url))?.clone(); },
        async put(url, r) {
          if (putFails) throw new DOMException('the quota has been exceeded', 'QuotaExceededError');
          s.set(String(url), r);
        },
      };
    },
    async keys() { return [...stores.keys()]; },
    async delete(name) { return stores.delete(name); },
  };
}

const host = (caches, said = [], served = 'fresh') => {
  const fetched = [];
  return {
    fetched, said,
    caches,
    fetch: async (url) => { fetched.push(String(url)); return new Response(served); },
    say: (m) => said.push(m),
  };
};

test('a copy kept for this manifest hash is used, with no fetch', async () => {
  const caches = fakeCaches();
  await (await caches.open(cacheName('new'))).put(URL_WASM, new Response('kept'));
  const h = host(caches);
  assert.equal(await (await engineFile(h, 'new', URL_WASM)).text(), 'kept');
  assert.deepEqual(h.fetched, []);
});

test('a cached engine whose hash no longer matches the manifest is not used: the file is fetched, kept under the new hash, and the old cache is removed', async () => {
  const caches = fakeCaches();
  // the visit before kept an engine under its own hash, at the same URL
  await (await caches.open(cacheName('old'))).put(URL_WASM, new Response('stale engine'));
  const h = host(caches, [], 'fresh engine');
  const r = await engineFile(h, 'new', URL_WASM);
  assert.equal(await r.text(), 'fresh engine', 'the stale copy was not served');
  assert.deepEqual(h.fetched, [String(URL_WASM)]);
  assert.deepEqual([...caches.stores.keys()], [cacheName('new')], 'the other engine\'s cache is gone');
  assert.equal(await caches.stores.get(cacheName('new')).get(String(URL_WASM)).text(), 'fresh engine');
  assert.deepEqual(h.said, []);
});

test('a copy that cannot be kept is said, and the file is still served', async () => {
  const caches = fakeCaches({ putFails: true });
  const h = host(caches, [], 'fresh engine');
  const r = await engineFile(h, 'new', URL_WASM);
  assert.equal(await r.text(), 'fresh engine');
  assert.equal(h.said.length, 1);
  assert.match(h.said[0], /dt_web_bg\.aaaa\.wasm could not be kept/);
  assert.match(h.said[0], /quota has been exceeded/);
});

test('a failed fetch is not kept and not said as a cache failure: the caller reads the response', async () => {
  const caches = fakeCaches();
  const h = { ...host(caches), fetch: async () => new Response('nope', { status: 404 }) };
  const r = await engineFile(h, 'new', URL_WASM);
  assert.equal(r.status, 404);
  assert.deepEqual(h.said, []);
  assert.equal(caches.stores.get(cacheName('new')).size, 0);
});

test('with no Cache API (an insecure page) the file is fetched as it always was, and nothing is said', async () => {
  const h = host(undefined);
  assert.equal(await (await engineFile(h, 'new', URL_WASM)).text(), 'fresh');
  assert.deepEqual(h.said, []);
});
