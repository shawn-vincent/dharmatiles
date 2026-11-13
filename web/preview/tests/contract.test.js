// The contract between the server and the page is checked by `tsc` (npm test runs it first): the page's
// request layer and the files that call it are checked against js/api.d.ts, which a Rust test keeps equal to
// the server's request and response types.  This test shows the check bites: with a field renamed in the
// contract, as a rename in Rust would after the file is regenerated, `tsc` fails in the page's own code.

import test from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tsc = join(root, 'node_modules', '.bin', 'tsc');

/** The page's js tree with `edit` applied to the contract, checked: {status, out}. */
function checkWith(edit) {
  const dir = mkdtempSync(join(tmpdir(), 'dt-contract-'));
  try {
    cpSync(join(root, 'js'), join(dir, 'js'), { recursive: true });
    cpSync(join(root, 'tsconfig.json'), join(dir, 'tsconfig.json'));
    cpSync(join(root, 'vendor'), join(dir, 'vendor'), { recursive: true });
    const file = join(dir, 'js', 'api.d.ts');
    writeFileSync(file, edit(readFileSync(file, 'utf8')));
    const r = spawnSync(tsc, ['-p', dir], { encoding: 'utf8' });
    // tsc could not be run at all (not installed): say so, not an empty failure
    if (r.error) throw new Error(`cannot run ${tsc}: ${r.error.message} (run npm install in web/preview)`);
    return { status: r.status, out: `${r.stdout}${r.stderr}` };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('the page checks clean against the contract as it is', () => {
  const r = checkWith((t) => t);
  assert.equal(r.status, 0, r.out);
});

test('a field renamed in the contract fails in the page, in the code that used it', () => {
  // the revision an edit names: every map edit, print list edit and save says it
  const r = checkWith((t) => t.replaceAll('docRev: number', 'revision: number'));
  assert.notEqual(r.status, 0, 'tsc passed although the contract no longer has docRev');
  assert.match(r.out, /docRev/);
  assert.match(r.out, /js\/(editor\/)?[a-z-]+\.js/, r.out);
});

test('a route that is not in the contract cannot be called', () => {
  const r = checkWith((t) => t.replace('"POST /api/maps/set"', '"POST /api/maps/changed"'));
  assert.notEqual(r.status, 0);
  assert.match(r.out, /host\.js/);
});

test('a field of a message or an event renamed in the program fails in the page that reads it', () => {
  // the gate's numbers: the panel's rows read them (the record showed "NaN mm²" when the page read other names)
  let r = checkWith((t) => t.replaceAll('band_mm2', 'bandMm2'));
  assert.notEqual(r.status, 0, 'tsc passed although the gate data no longer has band_mm2');
  assert.match(r.out, /messages\.js/, r.out);
  // a message's own field
  r = checkWith((t) => t.replace('tile_name: string', 'tileName: string'));
  assert.notEqual(r.status, 0, 'tsc passed although a message no longer has tile_name');
  assert.match(r.out, /tile_name/, r.out);
  // an event of the stream, read by the stream's reader
  r = checkWith((t) => t.replace('"type": "tile", rev: number, tile: TileSummary', '"type": "tile", rev: number, summary: TileSummary'));
  assert.notEqual(r.status, 0, 'tsc passed although the tile event no longer has tile');
  assert.match(r.out, /host\.js/, r.out);
});

test('a resource the program can tell and the page does not handle fails tsc', () => {
  const r = checkWith((t) => t.replace('{ "name": "list", "value": List }', '{ "name": "list", "value": List } | { "name": "newThing", "value": string }'));
  assert.notEqual(r.status, 0, 'tsc passed although a resource has no case in the page');
  assert.match(r.out, /live\.js/, r.out);
});
