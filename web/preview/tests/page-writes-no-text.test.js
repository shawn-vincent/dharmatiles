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

// The site's cards (docs/assets/live) hold to the same rule as the editor: they send the program a typed edit (`Engine.edit`, the
// program's `dt_service::edit::apply`) and never write a quote or count a byte of a tile's text.
const live = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', 'docs', 'assets', 'live');
const liveSources = readdirSync(live).filter((f) => f.endsWith('.js')).map((f) => join(live, f));

/** What makes tile text in a script: a KDL quote written by hand (`'"' +`, `+ '"'`, `"\""`), byte-length arithmetic, a slice-and-join of the source. */
const MAKES_TEXT = [
  [/['"`]\\?"['"`]\s*\+|\+\s*['"`]\\?"['"`]/, 'writes a KDL quote by hand'],
  [/\+\s*bytes\(|\bbytes\(\s*written/, 'counts bytes of text it wrote'],
  [/\.slice\(\s*0\s*,[^)]*\)\s*\+[^;]*\.slice\(/, 'splices the source'],
  [/`"\$\{[^`]*\}"`/, 'writes a quoted value in a template'],
];
const makesText = (src) => MAKES_TEXT.filter(([re]) => re.test(src)).map(([, why]) => why);

test('the site\'s cards make no tile text: every change to the source is an edit the program makes', () => {
  assert.ok(liveSources.length > 5, 'the scan found the site\'s scripts');
  for (const p of liveSources) {
    assert.deepEqual(makesText(code(p)), [], `${relative(live, p)} makes tile text`);
  }
});

test('the scan fails on text made by hand: a planted quote or byte count is caught', () => {
  assert.deepEqual(makesText(`const w = '"' + number + '"';`), ['writes a KDL quote by hand']);
  assert.deepEqual(makesText(`return { span: [at, bytes(written)], text: s.slice(0, from) + written + s.slice(to) };`).sort(), ['counts bytes of text it wrote', 'splices the source']);
  assert.deepEqual(makesText('const t = `"${n}${unit}"`;'), ['writes a quoted value in a template']);
  assert.deepEqual(makesText(`const total = a + bytesOf(b);`), []);
});

// The editor's place tool and the marks it adds hold to it too: a node is the typed parts of `NewNode` (js/newnode.js) and the program's
// `make` edit writes its words, quotes and points.  No tool builds a node as a string.
const TOOLS = ['place.js', 'points.js', 'newnode.js', 'editor/map.js', 'editor/rows.js'].map((f) => join(js, f));
const NODE_AS_TEXT = [
  [/\b(?:insert|tileEdit)\([^;]*[`'"](?:line|region|scatter|rock|tree|wall)\s/, 'hands a node as a string'],
  [/`[^`]*\b(?:at|through|boundedBy|fill|count)[= ][^`]*\$\{/, 'a template builds a node'],
  [/\bsq\)?["'`]\s*\)?;?\s*$|sq,\s*\$\{|\$\{[^}]*\}sq/m, 'writes a point with its unit'],
];
const buildsNodeText = (src) => NODE_AS_TEXT.filter(([re]) => re.test(src)).map(([, why]) => why);

test('the place tool and the marks make no node text: a node is typed parts the program writes', () => {
  for (const p of TOOLS) {
    // a hover label is shown, never written
    const src = code(p).split('\n').filter((l) => !/\blabel\b/.test(l)).join('\n').replace(/export function pointLabel[\s\S]*?\n}\n/, '');
    assert.deepEqual(buildsNodeText(src), [], `${name(p)} makes node text`);
  }
});

test('the scan fails on a node made as a string', () => {
  assert.deepEqual(buildsNodeText('await this.api.insert(\'stack\', `scatter at=${r} count=3`)'), ['hands a node as a string', 'a template builds a node']);
  assert.deepEqual(buildsNodeText('const fill = `${a}sq`;'), ['writes a point with its unit']);
  assert.deepEqual(buildsNodeText("await this.api.insert('stack', made(point(q)))"), []);
});
