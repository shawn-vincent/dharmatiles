// The whole page is type-checked, by construction: `checkJs` is on in both tsconfigs, so a new file is checked from its
// first line, and nothing may opt out of it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const here = resolve(import.meta.dirname, '..');
const root = resolve(here, '..', '..');

const config = (name) => JSON.parse(readFileSync(join(here, name), 'utf8'));

function jsFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...jsFiles(p));
    else if (name.endsWith('.js')) out.push(p);
  }
  return out;
}

test('both tsconfigs check every JavaScript file they include', () => {
  for (const name of ['tsconfig.json', 'tsconfig.site.json']) {
    assert.equal(config(name).compilerOptions.checkJs, true, `${name} must have checkJs on`);
  }
});

test('the page include covers every file under js/, so a new file is checked by default', () => {
  assert.ok(config('tsconfig.json').include.includes('js/**/*.js'));
});

test('no script opts out of the check', () => {
  const dirs = [join(here, 'js'), join(root, 'docs', 'assets')];
  for (const f of dirs.flatMap(jsFiles)) {
    assert.ok(!/@ts-nocheck/.test(readFileSync(f, 'utf8')), `${f} says @ts-nocheck`);
  }
});

test('every site script is in the site tsconfig', () => {
  const include = config('tsconfig.site.json').include;
  const site = join(root, 'docs', 'assets');
  const covered = (f) => include.some((g) => {
    const abs = resolve(here, g.replace(/\*\.js$/, ''));
    return g.endsWith('*.js') ? resolve(f, '..') === abs : resolve(here, g) === f;
  });
  // the editor's own copy under docs/editor is the page's code, checked by tsconfig.json
  for (const f of jsFiles(site)) assert.ok(covered(f), `${f} is in no tsconfig`);
});
