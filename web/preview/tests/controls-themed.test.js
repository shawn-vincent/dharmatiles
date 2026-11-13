// Every text-like input and every select takes the theme's control style (docs/assets/theme.css), and none is left to the
// browser's own: the one rule names the types it leaves out, so a missing or a new `type` cannot reopen the unthemed field
// (the map piece's Seed field had no styled `type`; the browser drew it in Arial with an inset border).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const theme = readFileSync(new URL('../../../docs/assets/theme.css', import.meta.url), 'utf8');

/** The page's sources: index.html and every js file under js/ (not the vendored libraries). */
function sources() {
  const out = [['index.html', readFileSync(new URL('../index.html', import.meta.url), 'utf8')]];
  const walk = (dir, rel) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.name === 'vendor') continue;
      if (e.isDirectory()) walk(new URL(`${e.name}/`, dir), `${rel}${e.name}/`);
      else if (e.name.endsWith('.js')) out.push([`${rel}${e.name}`, readFileSync(new URL(e.name, dir), 'utf8')]);
    }
  };
  walk(new URL('../js/', import.meta.url), 'js/');
  return out;
}

test('the control rule covers every input but the named non-text types, whatever its type attribute says', () => {
  const rule = theme.match(/^(input:where\(:not\(([^)]*)\)\)[^{]*)\{/m);
  assert.ok(rule, 'theme.css has the one control rule, `input:where(:not([type=…], …))`, selecting by what it leaves out');
  const left = [...rule[2].matchAll(/\[type="([a-z]+)"\]/g)].map((m) => m[1]);
  assert.deepEqual(left.filter((t) => ['text', 'search', 'number', 'email', 'url', 'tel', 'password'].includes(t)), [], 'a text-like type is never left out');
  assert.match(rule[1], /select/, 'the same rule styles select');
  assert.doesNotMatch(theme, /^input\[type="(text|search)"\][^{]*\{/m, 'no second rule that names text and search only');
  // the page's own inputs: each is a text-like input (themed by the rule) or one of the types the rule names
  const seen = [];
  for (const [file, src] of sources()) {
    for (const m of src.matchAll(/<input\b([^>]*)>/g)) {
      const t = m[1].match(/\btype=(?:"([a-z]+)"|\$\{)/);
      const type = t ? (t[1] ?? '(computed)') : null;
      if (type === '(computed)') seen.push(`${file}: a computed type: ${m[0].slice(0, 80)}`);
      else if (type != null && left.includes(type) === false && !['text', 'search', 'number'].includes(type)) seen.push(`${file}: type="${type}" is neither themed nor named by the rule`);
    }
  }
  assert.deepEqual(seen, []);
});

test('no stylesheet of the page draws a field the browser\'s way', () => {
  const css = readFileSync(new URL('../style.css', import.meta.url), 'utf8');
  assert.doesNotMatch(css, /\binset\b\s+(rgb|#|var)|border:\s*2px\s+inset/, 'no inset border');
  assert.doesNotMatch(css, /appearance:\s*auto/);
});
