// The site's kept drafts are looked up by the key the engine makes (dt-store's, asked of the worker: `keyOf`), driven here with
// a stand-in worker that answers `keyed`, and a stand-in store: the same key finds the draft, a document that does not read
// has no key (the store is not asked and the job runs, so its check says why), and another engine's key finds nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Engine } from '../../../docs/assets/live/engine.js';

/**
 * An engine whose worker is a stand-in: `keyed` answers with `engine|text` (null for text "broken"), a job answers a failed check
 * when the text is "broken" and otherwise a check and a build.
 */
async function rig(hash) {
  const posted = [];
  class FakeWorker {
    constructor() { this.onmessage = null; }
    terminate() {}
    postMessage(m) {
      posted.push(m);
      const say = (data) => queueMicrotask(() => this.onmessage?.({ data }));
      if (m.type === 'init') say({ type: 'ready', version: 'test' });
      else if (m.type === 'key') say({ type: 'keyed', id: m.id, key: m.text === 'broken' ? null : `${m.engine}|${m.system}|${m.cells}|${m.tier}|${m.text}` });
      else if (m.type === 'job') {
        if (m.text === 'broken') {
          say({ type: 'checked', id: m.id, result: { ok: false, errors: [{ message: 'does not read' }] }, ms: 1 });
          say({ type: 'idle', id: m.id });
        } else {
          say({ type: 'checked', id: m.id, result: { ok: true, errors: [] }, ms: 1 });
          say({ type: 'built', id: m.id, ms: 1, meta: { built: true }, positions: new Float32Array(3), colours: new Float32Array(3), stl: new Uint8Array(0) });
        }
      }
    }
  }
  // the page's globals the engine reads while it starts
  const saved = { Worker: globalThis.Worker, fetch: globalThis.fetch, compile: WebAssembly.compileStreaming };
  globalThis.Worker = /** @type {any} */ (FakeWorker);
  globalThis.fetch = /** @type {any} */ (async (url) => (String(url).endsWith('engine.json')
    ? new Response(JSON.stringify({ hash, js: 'dt_web.js', wasm: 'dt_web_bg.wasm', library: 'library.json' }))
    : new Response('{}')));
  WebAssembly.compileStreaming = /** @type {any} */ (async () => ({}));
  const engine = new Engine(new URL('http://site.test/'));
  const kept = new Map();
  const asked = [];
  engine.store = Promise.resolve(/** @type {any} */ ({
    get: async (key) => { asked.push(key); return kept.get(key) ?? null; },
    put: async (key, value) => { kept.set(key, value); },
  }));
  const heard = [];
  const card = engine.register('card', (ev) => heard.push(ev));
  const restore = () => { globalThis.Worker = saved.Worker; globalThis.fetch = saved.fetch; WebAssembly.compileStreaming = saved.compile; };
  return { engine, card, kept, asked, heard, posted, restore };
}

const ask = (card, text, version) => card.edit({ text, system: 'db', seed: null, version, cells: 24, tier: 'low', persist: true });
const settle = () => new Promise((r) => setTimeout(r, 30));

test('a draft kept under the key the engine makes is found: shown from the store, no job sent', async () => {
  const r = await rig('hashA');
  try {
    const draft = { checked: { ok: true, errors: [] }, built: { meta: {}, positions: new Float32Array(3), colours: new Float32Array(3), stl: new Uint8Array(0), ms: 1 } };
    r.kept.set('hashA|db|24|low|tile one', draft);
    ask(r.card, 'tile one', 1);
    await settle();
    assert.deepEqual(r.asked, ['hashA|db|24|low|tile one']);
    assert.equal(r.posted.some((m) => m.type === 'job'), false, 'nothing was built');
    assert.ok(r.heard.some((e) => e.type === 'built' && e.cached === true));
  } finally { r.restore(); }
});

test('a build is kept under that same key, and a second ask finds it', async () => {
  const r = await rig('hashA');
  try {
    ask(r.card, 'tile two', 1);
    await settle();
    assert.deepEqual([...r.kept.keys()], ['hashA|db|24|low|tile two']);
    const jobs = r.posted.filter((m) => m.type === 'job').length;
    r.engine.cache.items.clear(); // a new visit: the visit's own cache is empty, the store is what is left
    r.engine.cache.bytes = 0;
    ask(r.card, 'tile two', 2);
    await settle();
    assert.equal(r.posted.filter((m) => m.type === 'job').length, jobs, 'found, not built again');
  } finally { r.restore(); }
});

test('a document that does not read has no key: the store is not asked, and the job runs so its check says why', async () => {
  const r = await rig('hashA');
  try {
    ask(r.card, 'broken', 1);
    await settle();
    assert.deepEqual(r.asked, []);
    assert.equal(r.posted.filter((m) => m.type === 'job').length, 1);
    const checked = r.heard.find((e) => e.type === 'checked');
    assert.equal(checked?.result.ok, false);
    assert.equal(r.kept.size, 0, 'nothing is kept for it');
  } finally { r.restore(); }
});

test('a draft kept by another engine is not found: its key differs, so the job builds', async () => {
  const r = await rig('hashB');
  try {
    r.kept.set('hashA|db|24|low|tile one', { checked: { ok: true, errors: [] }, built: { meta: {}, positions: new Float32Array(3), colours: new Float32Array(3), stl: new Uint8Array(0), ms: 1 } });
    ask(r.card, 'tile one', 1);
    await settle();
    assert.deepEqual(r.asked, ['hashB|db|24|low|tile one']);
    assert.equal(r.posted.filter((m) => m.type === 'job').length, 1, 'built, not shown from the other engine');
  } finally { r.restore(); }
});
