import test from 'node:test';
import assert from 'node:assert/strict';
import { createTiers, tierReady } from '../js/tiers.js';

/** A fake program: it answers a post at once with keys, holds nothing, and says a tier is done when told. */
function fakeHost() {
  const posts = [];
  let open = 0, mostOpen = 0;
  const results = new Map();
  const host = {
    async tiersWant(body) {
      open += 1; mostOpen = Math.max(mostOpen, open);
      posts.push(body.wants.map((w) => `${w.tile}:${w.tier}`));
      await Promise.resolve();
      open -= 1;
      return { status: 200, body: { keys: body.wants.map((w) => `${w.tile}|${w.tier}`) } };
    },
    async tierResult(key, docRev) { const r = results.get(key); return r && r.docRev === docRev ? r.buf : null; },
  };
  return { host, posts, results, mostOpen: () => mostOpen };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

test('what is wanted in one moment is one post of the whole set, and nothing waits on a build', async () => {
  const f = fakeHost();
  const set = createTiers({ host: f.host, scope: 's' });
  const a = set.want('a', { tile: 'a', tier: 'low' }), b = set.want('b', { tile: 'b', tier: 'low' }), c = set.want('c', { tile: 'c', tier: 'full' });
  await tick();
  assert.deepEqual(f.posts, [['a:low', 'b:low', 'c:full']]);
  assert.equal(f.mostOpen(), 1, 'one connection at most, and it is answered at once');
  f.results.set('a|low', { docRev: 3, buf: new ArrayBuffer(4) });
  tierReady({ key: 'a|low', docRev: 3 });
  assert.equal((await a).docRev, 3);
  set.drop('b'); set.drop('c');
  assert.equal(await b, null);
  assert.equal(await c, null);
});

test('letting go of a piece withdraws it: the next post is the set without it', async () => {
  const f = fakeHost();
  const set = createTiers({ host: f.host, scope: 's' });
  set.want('x', { tile: 'x', tier: 'low' }); set.want('y', { tile: 'y', tier: 'low' });
  await tick();
  set.drop('x');
  await tick();
  assert.deepEqual(f.posts, [['x:low', 'y:low'], ['y:low']]);
  set.drop('y'); // sets are shared by the stream's announcements: leave nothing wanted for the next test
});

test('a result of another revision is not delivered, and an error is the want\'s', async () => {
  const f = fakeHost();
  const set = createTiers({ host: f.host, scope: 's' });
  const a = set.want('a', { tile: 'a', tier: 'low' });
  const b = set.want('b', { tile: 'b', tier: 'low' });
  await tick();
  f.results.set('a|low', { docRev: 2, buf: new ArrayBuffer(1) });
  tierReady({ key: 'a|low', docRev: 5 }); // the program says 5, what it holds is 2: refused, so nothing to show
  assert.equal(await a, null);
  tierReady({ key: 'b|low', docRev: 1, error: 'the tile does not read' });
  await assert.rejects(b, /does not read/);
});

test('a set is posted again after a reconnect, so a tier announced while the stream was away is not lost', async () => {
  const f = fakeHost();
  const set = createTiers({ host: f.host, scope: 's' });
  set.want('a', { tile: 'a', tier: 'low' });
  await tick();
  const { tiersReconnected } = await import('../js/tiers.js');
  tiersReconnected();
  await tick();
  assert.equal(f.posts.length, 2);
});

test('a tier the program announces while it answers the post (already built) is delivered, whichever arrives first', { timeout: 2000 }, async () => {
  // the program announces what it holds while answering: the announcement reaches the page before the answer
  const f = fakeHost();
  f.results.set('e|full', { docRev: 4, buf: new ArrayBuffer(8) });
  const tiersWant = f.host.tiersWant;
  f.host.tiersWant = async (body) => {
    for (const w of body.wants) if (f.results.has(`${w.tile}|${w.tier}`)) tierReady({ key: `${w.tile}|${w.tier}`, docRev: 4 });
    return tiersWant(body);
  };
  const set = createTiers({ host: f.host, scope: 's' });
  const e = set.want('e', { tile: 'e', tier: 'full' });
  const late = set.want('g', { tile: 'g', tier: 'full' });
  await tick();
  assert.equal((await e)?.docRev, 4, 'the early announcement was held until the answer named its key');
  // one that is not built is announced after the answer, as before
  f.results.set('g|full', { docRev: 1, buf: new ArrayBuffer(2) });
  tierReady({ key: 'g|full', docRev: 1 });
  assert.equal((await late)?.docRev, 1);
  assert.equal(set.size, 0);
});
