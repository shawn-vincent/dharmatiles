// The page never makes the text of a tile: every change to it is the program's (an edit's or a step's answer, `host.edit` and
// `host.step`) or what the user typed in the pane.  So no module splices bytes into the text, the one module that holds the text
// (editor/document.js) is the only one that puts text in the pane or the document store, and every other module hands `edited`
// the text of an edit's answer, nothing it made itself.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', 'js');
const sources = (dir = js) => readdirSync(dir).flatMap((f) => {
  const p = join(dir, f);
  return statSync(p).isDirectory() ? (f === 'vendor' ? [] : sources(p)) : p.endsWith('.js') ? [p] : [];
});
const code = (p) => readFileSync(p, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
const name = (p) => relative(js, p);

test('no module splices bytes into the text: there is no way to', () => {
  for (const p of sources()) {
    assert.ok(!/replaceBytes|\bapi\.replace\(|\.replace\(\s*(start|d\.start|\w+\.valueStart)\b/.test(code(p)), `${name(p)} splices text`);
  }
});

test('only the document module puts text in the pane or in the document store', () => {
  const own = new Set(['editor/document.js', 'text.js']);
  for (const p of sources().filter((f) => !own.has(name(f)))) {
    const src = code(p);
    assert.ok(!/\.setText\(/.test(src), `${name(p)} puts text in the pane`);
    assert.ok(!/\bdoc\.set\(\{[^}]*\btext\b\s*[:,}]/.test(src), `${name(p)} puts text in the document store`);
  }
});

test('every other module gives `edited` the text of an answer of the program', () => {
  for (const p of sources().filter((f) => name(f) !== 'editor/document.js')) {
    for (const m of code(p).matchAll(/\bedited\(([^,)]+)/g)) {
      assert.equal(m[1].trim(), 'r.body.text', `${name(p)} gives edited() ${m[1]}, not the program's answer`);
    }
  }
});
