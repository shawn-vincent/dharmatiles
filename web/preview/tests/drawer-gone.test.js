// The catalog drawer is gone: the Add tiles level of the map is the one place to choose tiles, and it walks the folders with the
// browser's own list (view/places-panel.js), so there is no second walk of the places.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

test('nothing of the catalog drawer is left in the page', () => {
  const seen = [];
  for (const base of ['../js/', '../js/editor/', '../js/view/']) {
    const dir = new URL(base, import.meta.url);
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.js')) continue;
      readFileSync(new URL(f, dir), 'utf8').split('\n').forEach((line, i) => { if (/\bdrawer\b|drawerPanel|drawerUi|scene-drawer/i.test(line)) seen.push(`${base}${f}:${i + 1}: ${line.trim()}`); });
    }
  }
  assert.deepEqual(seen, []);
  assert.doesNotMatch(readFileSync(new URL('../index.html', import.meta.url), 'utf8'), /drawer/i);
  assert.doesNotMatch(readFileSync(new URL('../style.css', import.meta.url), 'utf8'), /#drawer|\.dr-|body\.drawer/);
});
