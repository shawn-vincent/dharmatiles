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

test('null is checked: strictNullChecks is on in the page and the site, and the tests inherit it', () => {
  for (const name of ['tsconfig.json', 'tsconfig.site.json']) {
    assert.equal(config(name).compilerOptions.strictNullChecks, true, `${name} must have strictNullChecks on`);
  }
  assert.equal(config('tsconfig.tests.json').extends, './tsconfig.json', 'the tests must take the page\'s settings');
  assert.notEqual(config('tsconfig.tests.json').compilerOptions.strictNullChecks, false);
});

test('strict is on, noImplicitAny and strictNullChecks with it, in the page and the site, and the tests inherit it', () => {
  for (const name of ['tsconfig.json', 'tsconfig.site.json']) {
    const o = config(name).compilerOptions;
    assert.equal(o.strict, true, `${name} must have strict on`);
    assert.equal(o.noImplicitAny, true, `${name} must have noImplicitAny on: every parameter says its type`);
    assert.equal(o.strictNullChecks, true, `${name} must have strictNullChecks on`);
    // the flags `strict` brings must not be switched off one by one
    for (const flag of ['strictFunctionTypes', 'strictBindCallApply', 'strictPropertyInitialization', 'noImplicitThis', 'alwaysStrict', 'useUnknownInCatchVariables']) {
      assert.notEqual(o[flag], false, `${name} must not turn ${flag} off`);
    }
  }
  const tests = config('tsconfig.tests.json');
  assert.equal(tests.extends, './tsconfig.json');
  for (const flag of ['strict', 'noImplicitAny']) assert.notEqual(tests.compilerOptions[flag], false, `tsconfig.tests.json must not turn ${flag} off`);
});

test('the tests are checked too, and the map they draw is the whole fixture the program is held to', () => {
  const tests = config('tsconfig.tests.json');
  assert.ok(tests.include.includes('tests/*.js'), 'tsconfig.tests.json must include the tests');
  // a test that feeds the overlay a map says `@ts-check` and builds it from the fixture, never by hand
  for (const name of ['overlay-geometry', 'overlay-marks', 'square-size', 'map-fixture']) {
    const f = join(here, 'tests', name === 'map-fixture' ? 'map-fixture.js' : `${name}.test.js`);
    assert.match(readFileSync(f, 'utf8'), /^\/\/ @ts-check/, `${f} must be checked`);
  }
  const pkg = JSON.parse(readFileSync(join(here, 'package.json'), 'utf8'));
  assert.match(pkg.scripts.test, /tsc -p tsconfig\.tests\.json/, 'npm test must run the tests\' tsc pass');
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
