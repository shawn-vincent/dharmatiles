// The browser half of `cargo xtask parity`: load the wasm bundle under Node and print, one JSON line a tile, what
// the engine's `parity_line` says (the same function the native half calls). Usage:
//   node parity.mjs <bundle-dir> <tile>@<system> ...
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [dir, ...jobs] = process.argv.slice(2);
const engine = await import(pathToFileURL(join(dir, 'dt_web.js')).href);
await engine.default({ module_or_path: readFileSync(join(dir, 'dt_web_bg.wasm')) });
engine.set_library(readFileSync(join(dir, 'library.json'), 'utf8'));
for (const job of jobs) {
  const [tile, system] = job.split('@');
  console.log(engine.parity_line(tile, system));
}
