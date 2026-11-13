// A mesh is fetched by the revision of the document it is of, and a mesh of another revision is never handed
// to the page: the program refuses it (409) and the host says null, so there is nothing to show.

import test from 'node:test';
import assert from 'node:assert/strict';
import { browserHost } from '../js/host.js';

/** A program that holds one mesh, of revision `rev`, and answers as the real one does. */
function fakeProgram(rev) {
  const asked = [];
  globalThis.fetch = async (url) => {
    asked.push(String(url));
    const u = new URL(String(url), 'http://x');
    const want = u.searchParams.get('docRev');
    if (want == null) return new Response('{"error":{"kind":"refused","message":"ask for a revision"}}', { status: 400 });
    if (Number(want) !== rev) return new Response(JSON.stringify({ error: { kind: 'refused', message: 'another revision' }, docRev: rev }), { status: 409 });
    return new Response(new Uint8Array([1, 2, 3]), { status: 200 });
  };
  return asked;
}

test('the mesh is asked for by revision, and the revision it is of is the only one answered', async () => {
  const asked = fakeProgram(5);
  const host = browserHost();
  assert.equal((await host.fetchMesh('t1', 5)).byteLength, 3);
  assert.match(asked[0], /\/api\/tiles\/t1\/mesh\?docRev=5$/);
  assert.equal(await host.fetchMesh('t1', 4), null, 'an older revision is not served as if it were this one');
  assert.equal(await host.fetchMesh('t1', 6), null);
});

test('a built tier is fetched by the revision the program announced, and one of another revision is null', async () => {
  const asked = fakeProgram(2);
  const host = browserHost();
  assert.equal(await host.tierResult('t1|low|-|-|-', 1), null);
  assert.match(asked[0], /\/api\/tiers\/result\?key=t1%7Clow%7C-%7C-%7C-&docRev=1$/);
  assert.equal((await host.tierResult('t1|low|-|-|-', 2)).byteLength, 3);
});

test('the page shows a tile\'s mesh only where it asks by the summary\'s revision (no call without one)', async () => {
  const { readFileSync } = await import('node:fs');
  const src = readFileSync(new URL('../js/editor/open.js', import.meta.url), 'utf8');
  assert.match(src, /host\.fetchMesh\(t\.id, docRev\)/);
  assert.match(src, /\(tile\(\)\?\.docRev \?\? 0\) !== docRev/, 'a mesh that comes after the tile moved on is dropped');
});
