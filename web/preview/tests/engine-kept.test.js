// The site's kept drafts are the engine's: the page keeps no store, no key and no copy of a build.  The worker is handed the
// engine's hash at its start (the kept builds are of that engine alone) and builds a page's own example through the engine's kept
// builds when its job says `persist`.  Driven here with a stand-in worker.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Engine } from '../../../docs/assets/live/engine.js';

/** An engine whose worker is a stand-in that answers a check and a build. */
async function rig(hash) {
  const posted = [];
  class FakeWorker {
    constructor() { this.onmessage = null; }
    terminate() {}
    postMessage(m) {
      posted.push(m);
      const say = (data) => queueMicrotask(() => this.onmessage?.({ data }));
      if (m.type === 'init') say({ type: 'ready', version: 'test' });
      else if (m.type === 'job') {
        say({ type: 'checked', id: m.id, result: { ok: true, errors: [] }, ms: 1 });
        say({ type: 'built', id: m.id, ms: 1, meta: { built: true }, positions: new Float32Array(3), colours: new Float32Array(3), stl: new Uint8Array(0) });
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
  const heard = [];
  const card = engine.register('card', (ev) => heard.push(ev));
  const restore = () => { globalThis.Worker = saved.Worker; globalThis.fetch = saved.fetch; WebAssembly.compileStreaming = saved.compile; };
  return { engine, card, heard, posted, restore };
}

const settle = () => new Promise((r) => setTimeout(r, 30));

test('the worker is given the engine\'s hash, and an example of the page is built through the kept builds', async () => {
  const r = await rig('hashA');
  try {
    r.card.edit({ text: 'tile one', system: 'db', seed: null, version: 1, cells: 24, tier: 'low', persist: true });
    await settle();
    const init = r.posted.find((m) => m.type === 'init');
    assert.equal(init?.engine, 'hashA', 'the kept builds are of this engine alone');
    const job = r.posted.find((m) => m.type === 'job');
    assert.equal(job?.persist, true);
    assert.equal(r.posted.some((m) => m.type === 'key'), false, 'the page makes no key: the engine does');
    assert.ok(r.heard.some((e) => e.type === 'built'));
  } finally { r.restore(); }
});

test('an edited text and a full build are not kept between visits', async () => {
  const r = await rig('hashA');
  try {
    r.card.edit({ text: 'edited', system: 'db', seed: null, version: 1, cells: 24, tier: 'low', persist: false });
    await settle();
    r.card.full({ text: 'edited', system: 'db', seed: null, version: 2, cpuSecs: 5 });
    await settle();
    const jobs = r.posted.filter((m) => m.type === 'job');
    assert.equal(jobs.length, 2);
    assert.deepEqual(jobs.map((j) => j.persist), [false, false]);
  } finally { r.restore(); }
});

test('sliding back to a value built in the visit is served from the visit\'s cache, not built again', async () => {
  const r = await rig('hashA');
  try {
    r.card.edit({ text: 'tile two', system: 'db', seed: null, version: 1, cells: 24, tier: 'low', persist: true });
    await settle();
    r.card.edit({ text: 'tile two', system: 'db', seed: null, version: 2, cells: 24, tier: 'low', persist: true });
    await settle();
    assert.equal(r.posted.filter((m) => m.type === 'job').length, 1);
    assert.ok(r.heard.some((e) => e.type === 'built' && e.cached === true));
  } finally { r.restore(); }
});
