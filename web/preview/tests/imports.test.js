// Every ES module the editor and the site's live cards ship names only what its source module exports: an import of a missing
// name fails the whole module in a browser (every card on the site once did, after `fixedText` was deleted from spans.js), and
// the tests that import one module at a time do not see it in another.  A static check over the real files, resolved the way
// the page resolves them (tests/vendor-resolve.mjs: the import map, and the site's /editor/ path).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const preview = fileURLToPath(new URL('..', import.meta.url));
const repo = resolve(preview, '../..');
const html = readFileSync(join(preview, 'index.html'), 'utf8');
const { imports: importMap } = JSON.parse(html.match(/<script type="importmap">([\s\S]*?)<\/script>/)[1]);

/** @param {string} dir @returns {string[]} the .js files under it, not the vendored ones */
function modules(dir) {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return n === 'vendor' ? [] : modules(p);
    return n.endsWith('.js') ? [p] : [];
  });
}

/** The file a specifier names from `from`, as the page resolves it, or null for one the page does not serve (`node:`, urls). */
function target(from, spec) {
  const site = spec.match(/^(?:\.\.\/)+editor\/(js|vendor)\/(.*)$/);
  if (site) return join(preview, site[1], site[2]);
  if (spec.startsWith('.')) return resolve(dirname(from), spec);
  if (importMap[spec]) return join(preview, importMap[spec]);
  return null;
}

/** The names a module exports (`*` re-exports followed), or null for a vendored file we do not read. */
function exportsOf(file, seen = new Set()) {
  if (seen.has(file)) return new Set();
  seen.add(file);
  if (file.includes('/vendor/')) return null;
  const src = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
  const names = new Set();
  for (const m of src.matchAll(/^export\s+(?:async\s+)?(?:function\*?|class|const|let|var)\s+([A-Za-z_$][\w$]*)/gm)) names.add(m[1]);
  // `export const A = 1, B = 2;`: each declarator of the statement, found at bracket depth 0
  for (const m of src.matchAll(/^export\s+(?:const|let|var)\s+(?=[A-Za-z_$])/gm)) {
    let depth = 0, at = m.index + m[0].length, start = at, quote = '';
    for (; at < src.length; at++) {
      const c = src[at];
      if (quote) { if (c === '\\') at++; else if (c === quote) quote = ''; continue; }
      if (c === '"' || c === "'" || c === '`') quote = c;
      else if ('([{'.includes(c)) depth++;
      else if (')]}'.includes(c)) depth--;
      else if ((c === ',' && depth === 0) || (c === ';' && depth === 0)) {
        const d = src.slice(start, at).match(/^\s*([A-Za-z_$][\w$]*)/);
        if (d) names.add(d[1]);
        start = at + 1;
        if (c === ';') break;
      }
    }
    const d = src.slice(start, at).match(/^\s*([A-Za-z_$][\w$]*)/);
    if (d) names.add(d[1]);
  }
  for (const m of src.matchAll(/^export\s+(?:const|let|var)\s*\{([^}]*)\}\s*=/gm)) {
    for (const part of m[1].split(',')) {
      const n = part.trim().split(':').pop().split('=')[0].trim();
      if (n) names.add(n);
    }
  }
  for (const m of src.matchAll(/^export\s*\{([^}]*)\}/gm)) {
    for (const part of m[1].split(',')) {
      const n = part.trim().split(/\s+as\s+/).pop();
      if (n) names.add(n);
    }
  }
  for (const m of src.matchAll(/^export\s*\*\s*from\s*['"]([^'"]+)['"]/gm)) {
    const t = target(file, m[1]);
    const inner = t && exportsOf(t, seen);
    if (inner) inner.forEach((n) => names.add(n));
  }
  if (/^export\s+default\b/m.test(src)) names.add('default');
  return names;
}

const files = [...modules(join(preview, 'js')), ...modules(join(repo, 'docs/assets/live'))];

test('the module lists are not empty (the check looks at real files)', () => {
  assert.ok(files.length > 20);
  assert.ok(files.some((f) => f.endsWith('docs/assets/live/card.js')));
});

for (const file of files) {
  test(`${relative(repo, file)} imports only what exists`, () => {
    const src = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
    for (const m of src.matchAll(/^import\s+([\s\S]*?)\s+from\s+['"]([^'"]+)['"]/gm)) {
      const [, clause, spec] = m;
      const to = target(file, spec);
      if (!to) continue;
      // docs/engine/ is what `cargo xtask wasm` builds: not in a checkout until it is built
      if (to.startsWith(join(repo, 'docs/engine/'))) continue;
      assert.ok(existsSync(to), `${spec} is not a file the page serves (${to})`);
      const have = exportsOf(to);
      if (!have) continue;
      const named = clause.match(/\{([^}]*)\}/);
      for (const part of named ? named[1].split(',') : []) {
        const name = part.trim().split(/\s+as\s+/)[0];
        if (name) assert.ok(have.has(name), `${relative(repo, file)} imports { ${name} } from ${spec}, which does not export it`);
      }
      if (/^[A-Za-z_$][\w$]*\s*(,|$)/.test(clause.trim())) assert.ok(have.has('default'), `${spec} has no default export`);
    }
  });
}
