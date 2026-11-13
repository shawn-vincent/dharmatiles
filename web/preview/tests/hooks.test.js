import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { hookSet } from '../js/hooks.js';

test('every hook runs, in the order added, and the remover takes only its own', () => {
  const h = hookSet();
  const seen = [];
  const off1 = h.add(() => seen.push(1));
  h.add(() => seen.push(2));
  h.add((x) => seen.push(`3:${x}`));
  h.run('a');
  assert.deepEqual(seen, [1, 2, '3:a']);
  off1();
  seen.length = 0;
  h.run('b');
  assert.deepEqual(seen, [2, '3:b']);
  assert.equal(h.size, 2);
});

test('a hook that throws does not stop the others, and is not hidden', () => {
  const h = hookSet();
  const seen = [];
  h.add(() => { throw new Error('first'); });
  h.add(() => seen.push('second'));
  assert.throws(() => h.run(), /first/);
  assert.deepEqual(seen, ['second']);
  assert.throws(() => h.add(null), TypeError);
});

/** Every .js file under `dir`. */
const sources = (dir) => readdirSync(dir).flatMap((n) => {
  const p = join(dir, n);
  return statSync(p).isDirectory() ? sources(p) : p.endsWith('.js') ? [p] : [];
});

test('no part of the page assigns a viewer hook slot: each adds its own to the list', () => {
  const slot = /\bviewer\.(onRender|onUserViewChange)\s*=(?!=)/;
  const bad = sources(new URL('../js', import.meta.url).pathname)
    .filter((f) => slot.test(readFileSync(f, 'utf8')));
  assert.deepEqual(bad, [], 'use viewer.onRenderHook(fn) / viewer.onUserViewChangeHook(fn)');
});
