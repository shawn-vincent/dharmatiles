// A page of cards that cannot hurt a phone: the pure policy (docs/assets/live/limits.js), the draft store's key and bound, and the
// engine's waiting line and workers driven with a stand-in worker and fetch.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ByteCache, CACHE_BYTES, costWords, deviceVerdict, keptBytes, runawayMs } from '../../../docs/assets/live/limits.js';
import { Engine } from '../../../docs/assets/live/engine.js';
import { DraftStore, MAX_ENTRIES, oldestOver } from '../../../docs/assets/live/draft-store.js';

test('the result cache is bounded by bytes: the oldest go first, a use keeps one, and a value over the whole limit is refused', () => {
  const c = new ByteCache(100, (v) => v.n);
  assert.equal(c.set('a', { n: 40 }), true);
  c.set('b', { n: 40 });
  c.get('a'); // a is the newest now
  c.set('c', { n: 40 }); // 120 > 100: b, the oldest, goes
  assert.deepEqual([...c.items.keys()], ['a', 'c']);
  assert.equal(c.bytes, 80);
  assert.equal(c.set('big', { n: 101 }), false);
  assert.equal(c.get('big'), undefined);
  c.set('a', { n: 10 }); // replacing a key does not count it twice
  assert.equal(c.bytes, 50);
  assert.equal(CACHE_BYTES, 64 * 1024 * 1024);
});

test('a kept result weighs its arrays, so 64 MB holds dozens of drafts but not two full builds', () => {
  const draft = { built: { positions: new Float32Array(150_000), colours: new Float32Array(150_000), stl: new Uint8Array(0) }, checked: {} };
  assert.equal(keptBytes(draft), 1_200_000 + 20_000);
  const c = new ByteCache(CACHE_BYTES, keptBytes);
  for (let i = 0; i < 100; i++) c.set(`d${i}`, draft);
  assert.ok(c.size >= 50 && c.bytes <= CACHE_BYTES);
  const fullish = { built: { positions: new Float32Array(4_000_000), colours: new Float32Array(4_000_000), stl: new Uint8Array(22_000_000) } };
  const f = new ByteCache(CACHE_BYTES, keptBytes);
  for (let i = 0; i < 5; i++) f.set(`f${i}`, fullish);
  assert.ok(f.size <= 1, `${f.size} full-sized results kept`);
});

test('the store keeps the newest drafts', () => {
  assert.deepEqual(oldestOver([{ at: 3 }, { at: 1 }, { at: 2 }], 2).map((e) => e.at), [1]);
  assert.equal(oldestOver([{ at: 1 }], MAX_ENTRIES).length, 0);
});

test('the store does without IndexedDB: nothing found, nothing thrown', async () => {
  const none = new DraftStore('e1', undefined);
  assert.equal(await none.get('k'), null);
  await none.put('k', {});
});

test('the runaway limit is three times the estimate, within bounds, and the longest when there is none', () => {
  assert.equal(runawayMs(20), 60_000);
  assert.equal(runawayMs(1), 30_000);
  assert.equal(runawayMs(500), 240_000);
  assert.equal(runawayMs(null), 240_000);
  assert.equal(runawayMs(undefined), 240_000);
});

test('the button says the cost, and a device that cannot carry it is told so from what it reports', () => {
  const grass = { peakMb: 2050, cpuSecs: 21 };
  assert.equal(costWords(grass), 'about 2 GB, 20 s');
  assert.equal(costWords({ peakMb: 300, cpuSecs: 0.5 }), 'about 300 MB, under 1 s');
  assert.equal(deviceVerdict(grass, { deviceMemory: 4, coarse: true, smallScreen: true }).fits, false);
  assert.equal(deviceVerdict(grass, { deviceMemory: 8, coarse: false, smallScreen: false }).fits, true);
  // no deviceMemory (Safari, Firefox): a phone is taken to carry about 1 GB, a desktop to carry it
  assert.equal(deviceVerdict(grass, { coarse: true, smallScreen: true }).fits, false);
  assert.equal(deviceVerdict({ peakMb: 400, cpuSecs: 2 }, { coarse: true, smallScreen: true }).fits, true);
  assert.equal(deviceVerdict(grass, { coarse: false, smallScreen: false }).fits, true);
  assert.match(deviceVerdict(grass, { deviceMemory: 2, coarse: true, smallScreen: true }).why, /2 GB/);
});

const edit = (version) => ({ text: 't', system: 'db', seed: null, cells: 24, tier: 'low', version });
const until = async (/** @type {() => boolean} */ ok) => { for (let i = 0; i < 200 && !ok(); i++) await new Promise((r) => setTimeout(r, 5)); };

test('when the engine fails to start, every waiting card is told in its turn, and the line is not stranded', async () => {
  const realFetch = globalThis.fetch;
  globalThis.fetch = async () => { throw new Error('offline'); };
  try {
    const engine = new Engine(new URL('http://127.0.0.1:1/'));
    /** @type {Record<string, string[]>} */
    const heard = { a: [], b: [] };
    const [a, b] = ['a', 'b'].map((id) => engine.register(id, (ev) => heard[id].push(ev.type)));
    a.edit(edit(1));
    b.edit(edit(1));
    await until(() => heard.a.includes('failed') && heard.b.includes('failed'));
    assert.ok(heard.a.includes('failed'), `a heard ${heard.a}`);
    assert.ok(heard.b.includes('failed'), `b heard ${heard.b}: its job was left waiting`);
    assert.equal(engine.running, null);
  } finally {
    globalThis.fetch = realFetch;
  }
});

test('a full build is not kept, the worker that made it is replaced, and the next job runs on the fresh one', async () => {
  const realFetch = globalThis.fetch, realWorker = globalThis.Worker, realCompile = WebAssembly.compileStreaming;
  /** @type {any[]} */
  const made = [];
  globalThis.Worker = /** @type {any} */ (class {
    terminated = false;
    /** @type {any[]} */ sent = [];
    /** @type {any} */ onmessage = null;
    constructor() { made.push(this); }
    /** @param {any} m */
    postMessage(m) {
      this.sent.push(m);
      if (m.type === 'init') queueMicrotask(() => this.onmessage({ data: { type: 'ready', version: 'v' } }));
      if (m.type === 'job') {
        const built = { type: 'built', id: m.id, ms: 1, meta: { built: true, footprint: [35, 35] }, positions: new Float32Array(9), colours: new Float32Array(9), stl: new Uint8Array(4) };
        queueMicrotask(() => {
          if (m.steps.includes('check')) this.onmessage({ data: { type: 'checked', id: m.id, result: { ok: true, errors: [], notes: [], map: null, squareMm: 35, fullCost: null }, ms: 1 } });
          this.onmessage({ data: built });
        });
      }
    }
    terminate() { this.terminated = true; }
  });
  globalThis.fetch = /** @type {any} */ (async (/** @type {URL} */ url) => String(url).endsWith('engine.json')
    ? { ok: true, json: async () => ({ hash: 'h', js: 'g.js', wasm: 'm.wasm', library: 'l.json' }) }
    : { ok: true, text: async () => '{}' });
  WebAssembly.compileStreaming = /** @type {any} */ (async () => ({}));
  try {
    const engine = new Engine(new URL('http://127.0.0.1:1/'));
    /** @type {string[]} */
    const heard = [];
    const a = engine.register('a', (ev) => heard.push(`a:${ev.type}${ev.kind ? `:${ev.kind}` : ''}`));
    const b = engine.register('b', (ev) => heard.push(`b:${ev.type}${ev.kind ? `:${ev.kind}` : ''}`));
    a.full({ text: 't', system: 'db', seed: null, version: 1, cpuSecs: 20 });
    b.edit(edit(1)); // waits behind the full build, and must run on the fresh worker
    await until(() => heard.includes('b:idle:edit') && made.length >= 2);
    assert.ok(heard.includes('a:built:full'), `heard ${heard}`);
    assert.equal([...engine.cache.items.keys()].filter((k) => k.startsWith('full|')).length, 0, 'a full result was kept');
    const full = made.findIndex((w) => w.sent.some((m) => m.type === 'job' && m.tier === 'full'));
    assert.ok(full >= 0 && made[full].terminated, 'the worker that made the full build was not replaced');
    assert.ok(made.length > full + 1, 'no fresh worker followed the full build');
    // a draft names its card (its step cache in the worker); a full build names none
    assert.ok(made[full + 1].sent.some((m) => m.type === 'job' && m.tier === 'low' && m.card === 'b'), 'the draft did not run on the fresh worker, under its card');
    assert.ok(made[full].sent.every((m) => m.type !== 'job' || m.tier !== 'full' || m.card === null));
  } finally {
    globalThis.fetch = realFetch;
    globalThis.Worker = realWorker;
    WebAssembly.compileStreaming = realCompile;
  }
});
