// The thumbnail service (js/thumbs.js): at most five pictures at a time, a cache keyed by everything that makes a
// picture a different one, nothing built that nobody waits for, and the seed strip (view/sheet.js) built on it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { signal } from '@preact/signals-core';
import { install, settle } from './dom.js';
import { createThumbs, LIMIT, thumbKey } from '../js/thumbs.js';
import { tierReady } from '../js/tiers.js';

const tick = () => new Promise((r) => setTimeout(r, 0));

/** A fake program: it holds every want until told it is done, and remembers what it was asked. */
function fakeHost() {
  const asked = [];
  const open = new Set();
  const host = {
    async tiersWant(body) {
      for (const w of body.wants) { const k = `${w.tile}|${w.tier}|${w.seed ?? '-'}`; if (!asked.some((a) => a.key === k)) asked.push({ key: k, want: w }); }
      for (const w of body.wants) open.add(`${w.tile}|${w.tier}|${w.seed ?? '-'}`);
      return { status: 200, body: { keys: body.wants.map((w) => `${w.tile}|${w.tier}|${w.seed ?? '-'}`) } };
    },
    async tierResult(key, docRev) { return new TextEncoder().encode(`${key}@${docRev}`).buffer; },
  };
  return { host, asked, finish: (key) => tierReady({ key, docRev: 1 }) };
}

const drawn = (buf, spec) => `${new TextDecoder().decode(buf)}#${spec.plate ?? ''}`;

test('the queue runs at most five pictures at a time and starts the next as one finishes', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, scope: 'q1' });
  const out = Array.from({ length: 8 }, (_, i) => svc.picture({ tile: 't', seed: i + 1, tier: 'super-low' }));
  await tick(); await tick();
  assert.equal(LIMIT, 5);
  assert.deepEqual(svc.stats, { active: 5, waiting: 3, kept: 8 });
  assert.equal(f.asked.length, 5, 'the program was asked for five, not eight');
  f.finish('t|super-low|1');
  assert.equal(await out[0], 't|super-low|1@1#');
  await tick(); await tick();
  assert.equal(svc.stats.active, 5, 'the next waiting one took the free place');
  assert.equal(f.asked.length, 6);
  for (let i = 2; i <= 8; i++) { await tick(); f.finish(`t|super-low|${i}`); await tick(); await tick(); }
  const all = await Promise.all(out);
  assert.equal(all.length, 8);
  assert.deepEqual(svc.stats, { active: 0, waiting: 0, kept: 8 });
});

test('a picture is kept for the session: the same ask is the same picture and asks the program once', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, scope: 'q2' });
  const a = svc.picture({ tile: 't', seed: 3, plate: '#fff' });
  await tick(); f.finish('t|super-low|3');
  assert.equal(await a, 't|super-low|3@1##fff');
  assert.equal(await svc.picture({ tile: 't', seed: 3, plate: '#fff' }), await a);
  assert.equal(f.asked.length, 1);
});

test('what nobody waits for any more, and has not begun, is not built', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, scope: 'q3', limit: 1 });
  const first = { tile: 't', seed: 1 }, second = { tile: 't', seed: 2 };
  svc.picture(first);
  const gone = svc.picture(second);
  await tick();
  svc.release(second);
  assert.equal(await gone, null);
  f.finish('t|super-low|1');
  await tick(); await tick();
  assert.deepEqual(f.asked.map((a) => a.key), ['t|super-low|1'], 'the second was never asked for');
  assert.deepEqual(svc.stats, { active: 0, waiting: 0, kept: 1 });
});

test('a failed build rejects, is not kept, and asking again tries again', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, scope: 'q4' });
  const a = svc.picture({ tile: 't', seed: 9 });
  await tick();
  tierReady({ key: 't|super-low|9', docRev: 1, error: 'the build failed' });
  await assert.rejects(a, /the build failed/);
  assert.equal(svc.stats.kept, 0);
  const b = svc.picture({ tile: 't', seed: 9 });
  await tick(); f.finish('t|super-low|9');
  assert.equal(await b, 't|super-low|9@1#');
});

test('a mesh of another revision than the tile is not drawn', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, valid: () => false, scope: 'q5' });
  const a = svc.picture({ tile: 't', seed: 1 });
  await tick(); f.finish('t|super-low|1');
  assert.equal(await a, null);
  assert.equal(svc.stats.kept, 0, 'asked again at the next look');
});

test('the cache key names the tile, tier, seed, revision, system, printer, size and plate', () => {
  const base = { tile: 't', seed: 1, rev: '2', system: 'db', printer: 'default', w: 176, h: 124, plate: '#D9E0EB' };
  const k = thumbKey(base);
  for (const [name, other] of Object.entries({ tile: 'u', seed: 2, rev: '3', system: 'ol', printer: 'fine', w: 88, h: 62, plate: '#000', tier: 'low' })) {
    assert.notEqual(thumbKey({ ...base, [name]: other }), k, `${name} makes another picture`);
  }
  assert.equal(thumbKey({ ...base }), k);
  assert.equal(thumbKey({ tile: 't' }), thumbKey({ tile: 't', tier: 'super-low' }), 'super-low unless more is asked for');
});

test('the seed strip shows the current seed first and four new ones, marks one you own, and choosing one writes the seed once', async () => {
  const root = install();
  const { render, html } = await import('../js/view/html.js');
  const { Sheet } = await import('../js/view/sheet.js');
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: drawn, scope: 'strip' });
  const schema = { name: 'tile', controls: [{ name: 'seed', unit: 'number', numeric: true, default: '1' }] };
  const model = {
    title: 't', kind: 'tile', slim: true,
    groups: [{ at: 10, schema, written: [{ name: 'seed', text: 'seed=97', span: [20, 7] }] }],
    seedStrip: { svc, spec: { tile: 't', rev: '1' }, current: 97, owned: new Set([97]), onError: (e) => { throw e; } },
  };
  const calls = [];
  const api = { writable: () => true, canMine: () => false, replace() {}, set: (...a) => calls.push(a), onField() {}, checkValue: async () => null };
  const m = signal(model);
  render(html`<${Sheet} model=${m.value} api=${api} />`, root);
  await settle();
  assert.equal(root.querySelectorAll('.seed-card').length, 0, 'nothing is built until the dice is pressed');
  assert.equal(f.asked.length, 0);
  root.querySelector('.dice').dispatchEvent({ type: 'click' });
  await settle();
  const cards = root.querySelectorAll('.seed-card');
  assert.equal(cards.length, 5);
  assert.match(cards[0].textContent, /97now/);
  assert.equal(root.querySelectorAll('.own').length, 1, 'the one you own is marked');
  assert.equal(f.asked.length, 5, 'five low builds asked for');
  assert.ok(f.asked.every((a) => a.want.tier === 'super-low'));
  const picked = Number(cards[2].textContent.match(/\d+/)[0]);
  cards[2].dispatchEvent({ type: 'click' });
  await settle();
  assert.deepEqual(calls, [[10, 'seed', picked]], 'one write of the seed: one step of history');
  assert.equal(root.querySelectorAll('.seed-card').length, 0, 'the strip closes');
});
