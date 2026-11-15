// The service under Node: load the wasm bundle, put one library tile into the worker's `Service`, and call `check`,
// `save` and `build` by verb, as the page's worker will: `build` is HTTP's verb (it queues the job, the worker host
// runs it, and the record arrives on the event stream). Prints one JSON line: what the calls answered. Usage:
//   node service.mjs <bundle-dir> <tile>@<system>
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [dir, job] = process.argv.slice(2);
const [tile, system] = job.split('@');
const manifest = JSON.parse(readFileSync(join(dir, 'engine.json'), 'utf8'));
const engine = await import(pathToFileURL(join(dir, manifest.js)).href);
await engine.default({ module_or_path: readFileSync(join(dir, manifest.wasm)) });
const libraryText = readFileSync(join(dir, manifest.library), 'utf8');
engine.set_library(libraryText);
const text = JSON.parse(libraryText)[tile];
if (text === undefined) throw new Error(`${tile} is not in the library`);

const service = new engine.Service();
service.set_target(system);
const path = `/memory/My tiles/${tile.split('/').pop()}`;
service.put(path, text);
const checked = JSON.parse(service.call('check', path, JSON.stringify({ text })));
if (!checked.ok) throw new Error(`check: ${tile} does not read: ${JSON.stringify(checked.errors)}`);
// a save of the same text names the revision it was made from, and answers the revision the file has now
const saved = JSON.parse(service.call('save', path, JSON.stringify({ text, docRev: 1 })));
// build: the same verb as HTTP's. It answers that it was asked; the record comes on the stream.
const accepted = service.call('build', path, '{}');
if (accepted !== '{}') throw new Error(`build did not answer Accepted: ${accepted}`);
const events = JSON.parse(service.events());
const ready = events.find((e) => e.type === 'tile' && e.tile.status === 'ready');
if (ready === undefined) throw new Error(`no tile event said the build was ready: ${JSON.stringify(events.map((e) => e.type))}`);
const built = JSON.parse(service.built(path));
if (JSON.stringify(ready.tile.record.audit) !== JSON.stringify(built.record.audit)) {
  throw new Error('the stream\'s record is not the build\'s');
}
// a verb the service does not have is refused loudly, never answered empty
let refused = '';
try {
  service.call('nonesuch', path, '{}');
} catch (e) {
  refused = String(e);
}
if (!refused.includes('no such call')) throw new Error(`an unknown verb was not refused: ${refused}`);
console.log(
  JSON.stringify({
    tile,
    system,
    checkOk: checked.ok,
    nodes: checked.nodes.length,
    stack: checked.stack.length,
    events: events.length,
    docRev: saved.docRev,
    passed: built.passed,
    stlBytes: built.stlBytes,
    stlSha256: built.stlSha256,
    triangles: built.record.audit.triangles,
  }),
);
