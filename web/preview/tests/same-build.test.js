// A page and a program of different builds: the page shows only that, and what to do (js/editor/same-build.js).
// The stamp is js/contract.js, which the Rust test the_page_contract_is_current writes from js/api.d.ts and the
// program compiles in; the first snapshot of a stream carries the program's.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CONTRACT } from '../js/contract.js';
import { refuseOtherBuild, sameBuild } from '../js/editor/same-build.js';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', 'js');

test('the stamp is 16 hex digits, and only the same stamp is the same build', () => {
  assert.match(CONTRACT, /^[0-9a-f]{16}$/);
  assert.equal(sameBuild(CONTRACT), true);
  const other = (CONTRACT[0] === '0' ? '1' : '0') + CONTRACT.slice(1);
  assert.equal(sameBuild(other), false);
  assert.equal(sameBuild(undefined), false, 'a program too old to tell a stamp is another build too');
});

test('another build hides the whole editor and shows only the words for it', () => {
  const el = (hidden = false, text = '') => ({ hidden, textContent: text });
  const app = el(), box = el(true), title = el(false, "The editor didn't start."), why = el();
  const byId = { app, 'boot-error': box, 'boot-title': title, 'boot-why': why };
  refuseOtherBuild(/** @type {any} */ ({ getElementById: (id) => byId[id] ?? null }));
  assert.equal(app.hidden, true, 'nothing of the editor stays to be misread');
  assert.equal(box.hidden, false);
  assert.match(title.textContent, /different builds/);
  assert.match(why.textContent, /Reload the page.*restart the program/);
});

test('a snapshot from another build is refused before anything of it is taken', () => {
  const src = readFileSync(join(js, 'editor', 'live.js'), 'utf8');
  const body = src.slice(src.indexOf('export function applySnapshot(m) {'));
  const first = body.split('\n')[1].trim();
  assert.match(first, /^if \(!sameBuild\(m\.contract\)\) \{ refuseOtherBuild\(\); return; \}/, 'the check is the first thing applySnapshot does');
});
