// One error path, server to screen (docs/standards/errors.html): the server's
// error shape becomes a ServerError in host.js, and problems.js shows and
// copies the whole chain with both stacks and the environment.

import test from 'node:test';
import assert from 'node:assert/strict';

import { browserHost } from '../js/host.js';
import { ServerError, serverErrorOf } from '../js/server-error.js';
import { chainOf, collectEnvironment, describe, detailsOf, dismiss, failed, problems, setEnvironment, showError } from '../js/problems.js';

const serverBody = (over = {}) => ({
  error: {
    kind: 'bug',
    message: 'a test panic in a route',
    help: 'This is a bug in dharmatiles, not in your tile.',
    causes: ['panicked at crates/dharmatiles/src/preview/route.rs:102:42'],
    backtrace: '   0: dharmatiles::preview::route::route\n   1: dharmatiles::preview::server::answer',
    context: { route: 'POST /api/tiles/abc/save', tile: 'abc', version: '1.0.0+test.abc', atMs: 1 },
    ...over,
  },
});

/** A `fetch` that answers the session, then `status` and `body` (or `raw`) for everything else. */
function fakeFetch(status, body, raw) {
  return async (url) => {
    if (String(url).includes('/api/session')) return new Response(JSON.stringify({ token: 't' }), { status: 200 });
    return new Response(raw ?? JSON.stringify(body), { status });
  };
}
async function withFetch(fn, impl) {
  const original = globalThis.fetch, log = console.error;
  globalThis.fetch = impl;
  console.error = () => {};
  try { return await fn(); } finally { globalThis.fetch = original; console.error = log; }
}

test('a failed reply becomes a ServerError holding the server\'s kind, message, causes and backtrace', async () => {
  const r = await withFetch(() => browserHost().save('abc', { text: 'x', base: 'y' }), fakeFetch(500, serverBody()));
  assert.equal(r.status, 500);
  assert.ok(r.error instanceof ServerError);
  assert.equal(r.error.kind, 'bug');
  assert.equal(r.error.message, 'a test panic in a route');
  assert.deepEqual(r.error.causes, ['panicked at crates/dharmatiles/src/preview/route.rs:102:42']);
  assert.match(r.error.backtrace, /preview::server::answer/);
  assert.equal(r.error.context.route, 'POST /api/tiles/abc/save');
  assert.equal(r.body.error, 'a test panic in a route', 'the words stay where callers show them');
});

test('a reply without the structure still becomes one kind of error', async () => {
  const r = await withFetch(() => browserHost().save('abc', {}), fakeFetch(403, null, 'not this page'));
  assert.equal(r.error.kind, 'refused');
  assert.equal(r.error.message, 'not this page');
  assert.equal(serverErrorOf(502, { error: 'bad gateway' }).kind, 'failure');
  assert.equal(serverErrorOf(200, { ok: true }), null);
});

test('a fetch that fails throws the page\'s error with the server\'s as its cause', async () => {
  const impl = fakeFetch(500, serverBody({ kind: 'failure', message: 'cannot read the file' }));
  const e = await withFetch(() => browserHost().fetchMesh('abc').then(() => null, (x) => x), impl);
  assert.ok(e instanceof Error && !(e instanceof ServerError));
  assert.match(e.message, /^mesh: cannot read the file/);
  assert.ok(e.cause instanceof ServerError);
  assert.equal(e.cause.kind, 'failure');
});

test('a failure and a refusal are the headline in their own words; anything else is a bug inside dharmatiles', () => {
  const se = (kind) => new ServerError({ kind, message: 'cannot write x', help: 'Free some space.', causes: [], backtrace: '', context: {} });
  const d = describe(new Error('Could not save', { cause: se('failure') }));
  assert.deepEqual([d.kind, d.headline, d.help, d.detail], ['failure', 'cannot write x', 'Free some space.', null]);
  assert.equal(describe(new Error('Could not save', { cause: se('refused') })).kind, 'refused');
  const bug = describe(new Error('Could not save', { cause: se('bug') }));
  assert.equal(bug.kind, 'bug');
  assert.equal(bug.headline, 'Something went wrong inside dharmatiles');
  assert.equal(bug.detail, 'cannot write x');
  const js = describe(new TypeError('x is undefined'));
  assert.equal(js.kind, 'bug');
  assert.equal(js.detail, 'x is undefined');
});

test('the chain has the page\'s error and its stack, then the server\'s with causes and backtrace', () => {
  const se = new ServerError(serverBody().error, 500);
  const chain = chainOf(new Error('Could not save 1x1-cliff', { cause: se }));
  assert.equal(chain.length, 2);
  assert.equal(chain[0].side, 'page');
  assert.ok(chain[0].trace.some((l) => /\bat\b/.test(l)), 'the page\'s JS stack');
  assert.equal(chain[1].side, 'server');
  assert.deepEqual(chain[1].causes, se.causes);
  assert.equal(chain[1].trace.length, 2);
  assert.match(chain[1].context, /POST \/api\/tiles\/abc\/save, tile abc/);
});

test('the copied report holds what, both stacks and the environment', () => {
  const se = new ServerError(serverBody().error, 500);
  const page = new Error('Could not save 1x1-cliff', { cause: se });
  const fakeGlobals = {
    navigator: { userAgent: 'TestBrowser/1.0', platform: 'TestOS' },
    screen: { width: 1440, height: 900 }, devicePixelRatio: 2, innerWidth: 1200, innerHeight: 800,
    location: { href: 'http://127.0.0.1:7878/#tile' },
  };
  const env = collectEnvironment(fakeGlobals, {
    version: '1.0.0+7.gabc123', system: 'ol', printer: 'fine', tile: '/tiles/1x1-cliff.dtile', file: '/tiles/parts/edges.dtile',
  }, new Date('2026-11-11T10:20:30Z'));
  const text = detailsOf('Could not save 1x1-cliff', page, env);
  for (const must of [
    'What:     Could not save 1x1-cliff',
    'Something went wrong inside dharmatiles',
    '1.0.0+7.gabc123', 'ol', 'fine', '/tiles/1x1-cliff.dtile', '/tiles/parts/edges.dtile',
    'TestBrowser/1.0', 'TestOS', '1440x900 at 2x', '2026-11-11T10:20:30.000Z', 'http://127.0.0.1:7878/#tile',
    'stack (JavaScript):', 'backtrace (Rust):', 'dharmatiles::preview::server::answer',
    'panicked at crates/dharmatiles/src/preview/route.rs:102:42', 'POST /api/tiles/abc/save',
  ]) assert.ok(text.includes(must), `the report lacks ${must}\n${text}`);
  assert.ok(text.indexOf('stack (JavaScript):') < text.indexOf('backtrace (Rust):'), 'the page\'s stack, then the server\'s');
});

test('failed() puts a 5xx reply in the panel with both errors, and leaves a refusal to the caller', async () => {
  const log = console.error;
  console.error = () => {};
  try {
    setEnvironment(() => ({ version: '9.9.9', system: 'db', printer: 'default', tile: '/t.dtile', file: '/t.dtile' }));
    dismiss();
    const r = await withFetch(() => browserHost().save('abc', {}), fakeFetch(500, serverBody()));
    assert.equal(failed('Could not save abc', r), true);
    const [entry] = problems();
    assert.equal(entry.kind, 'bug');
    assert.equal(entry.chain.length, 2);
    assert.match(entry.details, /backtrace \(Rust\)/);
    assert.match(entry.details, /dharmatiles +9\.9\.9/);
    const refused = await withFetch(() => browserHost().save('abc', {}), fakeFetch(404, serverBody({ kind: 'refused', message: 'no such tile' })));
    assert.equal(failed('Could not save abc', refused), false);
    assert.equal(problems().length, 1);
    showError('A script error', new TypeError('boom'));
    assert.equal(problems()[1].kind, 'bug');
    dismiss();
  } finally {
    console.error = log;
  }
});

test('a tile error copies its code, place, words, help, source line and the environment', async () => {
  const { tileErrorDetails } = await import('../js/problems.js');
  const e = { code: 'dt::look', file: '/t.dtile', line: 3, column: 9, message: 'the top leaves the crown 0.2 mm', help: 'raise the top', unexpected: false };
  const env = { version: '1.0.0+9.gabc', system: 'db', printer: 'default', tile: 'mine', file: '/t.dtile', userAgent: 'UA', platform: 'macOS', screen: '1x1', time: 'T', url: 'U' };
  const r = tileErrorDetails(e, { source: 'a\nb\n    tree at=x\nd', env });
  for (const s of ['Code:     dt::look', 'Where:    /t.dtile:3:9', 'Said:     the top leaves', 'Help:     raise the top', 'a mistake in the tile', '>    3 |     tree at=x', 'dharmatiles   1.0.0+9.gabc', 'browser       UA']) {
    assert.ok(r.includes(s), `${s}\n---\n${r}`);
  }
  assert.ok(tileErrorDetails({ ...e, unexpected: true }, { env }).startsWith('dharmatiles bug report'));
});
