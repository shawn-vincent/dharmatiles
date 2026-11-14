// The square's size is the program's (`Target.squareMm`, 35 on DungeonBlocks, 25.4 on OpenLOCK): the page never guesses it.
// A fallback to a number would silently draw the wrong grid on the other system.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { Overlay } from '../js/overlay.js';

const root = new URL('../js/', import.meta.url).pathname;

/** Every .js file under web/preview/js, as paths relative to it. */
const sources = () => readdirSync(root, { recursive: true }).filter((f) => String(f).endsWith('.js')).map(String);

test('no square size is guessed in the page: no ?? 35, || 35, "= 35" default or 25.4 literal', () => {
  const seen = [];
  for (const f of sources()) {
    readFileSync(root + f, 'utf8').split('\n').forEach((line, i) => {
      if (/^\s*(\/\/|\*|\/\*)/.test(line)) return;
      if (/(\?\?|\|\|)\s*\[?35\b|\bsq\w*\s*=\s*35\b|\bsquare\s*=\s*35\b|\b25\.4\b/.test(line)) seen.push(`${f}:${i + 1}: ${line.trim()}`);
    });
  }
  assert.deepEqual(seen, []);
});

test('an overlay without the program\'s square size draws no map', () => {
  const o = new Overlay({ getContext: () => ({}) }, {});
  const map = { cols: 1, rows: 1, regions: [], lines: [] };
  o.setMap(map, undefined);
  assert.equal(o.map, null);
  assert.equal(o.sq, null);
  o.setMap(map, 25.4);
  assert.equal(o.map, map);
  assert.equal(o.sq, 25.4);
  o.setMap(map, undefined);
  assert.equal(o.map, null, 'a size that goes missing is not kept from before');
});
