// Every file of the page parses.  Most of the editor's modules need a browser to be imported, so a mistake in one (a name
// declared twice after an edit) would only show when the page loads; this reads each one with `node --check`.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../js', import.meta.url));
const files = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(join(dir, e.name)) : e.name.endsWith('.js') ? [join(dir, e.name)] : []));

test('every javascript file of the page parses', () => {
  const bad = [];
  for (const f of files(root)) {
    const r = spawnSync(process.execPath, ['--check', f], { encoding: 'utf8' });
    if (r.status !== 0) bad.push(`${f.slice(root.length + 1)}: ${r.stderr.split('\n').find((l) => /Error/.test(l)) ?? r.stderr.slice(0, 120)}`);
  }
  assert.deepEqual(bad, []);
});
