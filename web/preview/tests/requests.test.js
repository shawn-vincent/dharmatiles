import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { browserHost } from '../js/host.js';
import { createRequests, SUPERSEDED } from '../js/requests.js';

const tick = () => new Promise((r) => setTimeout(r, 5));
const reply = (body, status = 200) => ({ ok: status < 400, status, text: async () => JSON.stringify(body), json: async () => body });

/** A fetch the test answers by hand: each call waits in `pending` until it is resolved. */
function fakeNetwork() {
  const pending = [];
  globalThis.fetch = (url, opts = {}) => new Promise((resolve) => {
    if (url === '/api/session') return resolve(reply({ token: 't' }));
    pending.push({ url, body: opts.body ? JSON.parse(opts.body) : null, answer: (b, s) => resolve(reply(b, s)) });
  });
  return pending;
}

test('of the requests under one key only the newest one\'s answer is taken, in whatever order the answers come', async () => {
  const q = createRequests();
  const gates = [];
  const ask = (n) => q.latest('names', () => new Promise((r) => gates.push(() => r(n))));
  // the newest answered first, the old one after it
  const a = ask('old'), b = ask('new');
  gates[1](); gates[0]();
  assert.equal(await b, 'new');
  assert.equal(await a, SUPERSEDED, 'an old answer arriving after a new one is thrown away');
  // the old one answered first: it is not taken either, the newest has been asked
  gates.length = 0;
  const c = ask('c'), d = ask('d');
  gates[0](); gates[1]();
  assert.equal(await c, SUPERSEDED);
  assert.equal(await d, 'd');
  // another key is no one's business
  const e = q.latest('check', async () => 'e');
  assert.equal(await e, 'e');
});

test('a queue runs requests of one key one after the other, and everything in flight is counted', async () => {
  const q = createRequests();
  const seen = [];
  const gate = [];
  const one = q.queue('doc', () => new Promise((r) => gate.push(() => { seen.push(1); r(1); })));
  const two = q.queue('doc', async () => { seen.push(2); return 2; });
  await tick();
  assert.deepEqual(seen, [], 'the second waits for the first');
  assert.ok(q.inflight >= 2);
  gate[0]();
  assert.deepEqual([await one, await two], [1, 2]);
  assert.deepEqual(seen, [1, 2]);
  await q.idle();
  assert.equal(q.inflight, 0);
});

test('the host\'s calls: a newer check of a tile takes the place of an older one, and the old one says nothing', async () => {
  const pending = fakeNetwork();
  const host = browserHost();
  const a = host.check('t1', { text: 'a' });
  const b = host.check('t1', { text: 'ab' });
  const other = host.check('t2', { text: 'x' });
  await tick();
  assert.equal(pending.length, 3);
  pending[1].answer({ ok: true, stack: [] });
  pending[0].answer({ ok: false, errors: [] });
  pending[2].answer({ ok: true });
  const ra = await a;
  assert.equal(ra.status, 0, 'it is not an error and not an answer');
  assert.equal(ra.body.superseded, true);
  assert.deepEqual((await b).body, { ok: true, stack: [] });
  assert.equal((await other).body.ok, true, 'another tile\'s check is its own key');
  await host.requests.idle();
});

test('a write is never dropped for a newer one: two saves are both made and both answered', async () => {
  const pending = fakeNetwork();
  const host = browserHost();
  const a = host.save('t1', { text: 'one', docRev: 1 });
  const b = host.save('t1', { text: 'two', docRev: 2 });
  await tick();
  assert.equal(pending.length, 2);
  pending[1].answer({ docRev: 3 });
  pending[0].answer({ docRev: 2 });
  assert.equal((await a).body.docRev, 2);
  assert.equal((await b).body.docRev, 3);
});

test('every call of the page is made through the request layer: host.js reaches the network in one place', () => {
  const src = fs.readFileSync(new URL('../js/host.js', import.meta.url), 'utf8');
  const lines = src.split('\n').filter((l) => /\bfetch\(/.test(l) && !/^\s*(\/\/|\*|\/\*)/.test(l));
  assert.equal(lines.length, 1, `fetch( appears outside the one function that wraps it: ${lines.join(' | ')}`);
  assert.match(lines[0], /await fetch\(\.\.\.args\)/);
  assert.match(src, /const netFetch = \(\.\.\.args\) => requests\.direct\(/);
  // and no other module of the page calls the program by itself
  const stray = [];
  const walk = (dir) => {
    for (const f of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = `${dir}/${f.name}`;
      if (f.isDirectory()) { if (f.name !== 'api') walk(p); continue; }
      if (!f.name.endsWith('.js') || f.name === 'host.js') continue;
      if (/[^.\w]fetch\(|new EventSource|XMLHttpRequest/.test(fs.readFileSync(p, 'utf8'))) stray.push(p);
    }
  };
  walk(new URL('../js', import.meta.url).pathname);
  assert.deepEqual(stray, [], 'only host.js talks to the program');
});
