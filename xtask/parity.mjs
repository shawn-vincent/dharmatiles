// The browser half of `cargo xtask parity`: load the wasm bundle under Node and print, one JSON line a tile, what
// the engine's `parity_line` says (the same function the native half calls). Usage:
//   node parity.mjs <bundle-dir> <tile>@<system> ...
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [dir, ...jobs] = process.argv.slice(2);
// the bundle's files carry their hashes in their names: engine.json says which
const manifest = JSON.parse(readFileSync(join(dir, 'engine.json'), 'utf8'));
const engine = await import(pathToFileURL(join(dir, manifest.js)).href);
await engine.default({ module_or_path: readFileSync(join(dir, manifest.wasm)) });
engine.set_library(readFileSync(join(dir, manifest.library), 'utf8'));
// a build that cannot get memory panics with what it asked for and the stage (the engine's panic hook prints it
// on the console) and then traps: the trap is reported with that text, as a line of its own, and the run fails
const said = [];
const log = console.error;
console.error = (...a) => { said.push(a.join(' ')); log(...a); };
for (const job of jobs) {
  const [tile, system] = job.split('@');
  try {
    console.log(engine.parity_line(tile, system));
  } catch (e) {
    console.log(JSON.stringify({ tile: job, error: said.join('\n') || String(e) }));
    process.exit(1);
  }
}
