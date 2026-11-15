// The table of the page's non-plain elements by id (`IdKinds`, js/editor/app.js) is the markup's: each id it lists is in
// index.html as the tag its type names, and every input, dialog, canvas, form and button of the markup is in it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const app = readFileSync(new URL('../js/editor/app.js', import.meta.url), 'utf8');

const TAG = { HTMLInputElement: 'input', HTMLDialogElement: 'dialog', HTMLCanvasElement: 'canvas', HTMLFormElement: 'form', HTMLButtonElement: 'button' };

test('the page-element table names each input, dialog, canvas, form and button of index.html, as the tag it is', () => {
  const table = new Map();
  const block = app.slice(app.indexOf('@typedef {{'), app.indexOf('}} IdKinds'));
  for (const m of block.matchAll(/'([a-z0-9-]+)': (HTML[A-Za-z]+Element)/g)) table.set(m[1], TAG[m[2]]);
  const markup = new Map();
  for (const m of html.matchAll(/<([a-z0-9]+)\b[^>]*\bid="([^"]+)"/g)) if (Object.values(TAG).includes(m[1])) markup.set(m[2], m[1]);
  assert.deepEqual(Object.fromEntries([...table].sort()), Object.fromEntries([...markup].sort()));
});
