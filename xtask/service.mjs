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

// the worker's host hears a build's progress as it happens: one StreamEvent a stage
const streamed = [];
globalThis.dtStream = (json) => streamed.push(JSON.parse(json));
const service = new engine.Service(crypto.getRandomValues(new Uint8Array(32)));
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
if (!streamed.some((e) => e.type === 'progress' && e.progress.what === 'building' && e.progress.count?.[2] > 0)) {
  throw new Error(`the build said no progress on the stream: ${JSON.stringify(streamed.slice(0, 2))}`);
}
const built = JSON.parse(service.built(path));
if (JSON.stringify(ready.tile.record.audit) !== JSON.stringify(built.record.audit)) {
  throw new Error('the stream\'s record is not the build\'s');
}
// the page's mesh, by revision: the payload's magic and its header's triangle count are the build's
const mesh = service.mesh(path, saved.docRev);
if (!mesh) throw new Error('the worker has no mesh for the revision it built');
if (new TextDecoder().decode(mesh.subarray(0, 4)) !== 'DTM2') throw new Error('the worker\'s mesh is not a DTM2 payload');
const headLen = new DataView(mesh.buffer, mesh.byteOffset).getUint32(4, true);
const head = JSON.parse(new TextDecoder().decode(mesh.subarray(8, 8 + headLen)));
if (head.triangles !== built.record.audit.triangles) throw new Error(`the mesh has ${head.triangles} triangles, the build ${built.record.audit.triangles}`);
// a mesh of another revision is not given
if (service.mesh(path, saved.docRev + 5) != null) throw new Error('a mesh of another revision was given');
// the reading verbs, answered with no tile and no build: the page's startup and typing calls
const about = JSON.parse(service.call('about', '', '{}'));
if (!about.version || !about.formats) throw new Error(`about says nothing: ${JSON.stringify(about)}`);
if (!JSON.parse(service.call('elements', '', '{}')).some((e) => e.name === 'rock')) throw new Error('the element schema has no rock');
if (!JSON.parse(service.call('names', '', JSON.stringify({ text }))).ok) throw new Error('names did not read the tile');
const read = JSON.parse(service.call('text', path, '{}'));
if (read.text !== text || !read.writable) throw new Error('text did not give the stored document');
// a draft is the mesh's bytes (the same payload), asked for with call_bytes; a text with a mistake is a refusal that carries its status
const draft = service.call_bytes('draft', path, JSON.stringify({ text }));
if (new TextDecoder().decode(draft.subarray(0, 4)) !== 'DTM2') throw new Error('the draft is not a DTM2 payload');
let mistake = '';
try { service.call_bytes('draft', path, JSON.stringify({ text: 'tile {' })); } catch (e) { mistake = String(e.message ?? e); }
if (JSON.parse(mistake).status !== 422) throw new Error(`a draft of a mistake was not a 422: ${mistake}`);
// the first message of a stream, and a library tile copied into My tiles by its id
const snapshot = JSON.parse(service.snapshot());
if (!snapshot.roots.some((r) => r.kind === 'library') || snapshot.tiles.length === 0) throw new Error('the snapshot lists no library');
const mine = JSON.parse(service.call('mine', `lib:${tile.replace(/^lib:/, '')}`, '{}'));
if (!mine.tile || !mine.path.startsWith('/memory/My tiles/')) throw new Error(`mine did not list a copy: ${JSON.stringify(mine)}`);
const call = (verb, id, body) => JSON.parse(service.call(verb, id, JSON.stringify(body)));
// a print list and a map: made, filled, planned in the worker and settings changed, as the page asks
const listed = call('listsAdd', '', { tile: mine.id });
if (!listed.made || listed.count !== 1) throw new Error(`listsAdd did not make a list: ${JSON.stringify(listed)}`);
const gridded = call('listsNew', '', { name: 'Map', map: true });
const piece = call('mapsAdd', '', { list: gridded.id, tile: mine.id, at: '(1sq, 0sq)', docRev: JSON.parse(service.call('text', gridded.id, '{}')).docRev });
if (piece.scene?.placed !== 1 || piece.scene.pieces[0].error) throw new Error(`mapsAdd did not place a piece: ${JSON.stringify(piece.scene)}`);
const planned = call('platesSet', gridded.id, { list: gridded.id, key: 'printer', value: 'bambu/x1c', docRev: piece.docRev });
if (!planned.text.includes('bambu/x1c')) throw new Error('platesSet did not write the printer');
const planOf = (id) => JSON.parse(service.snapshot()).tiles.find((t) => t.id === id)?.plan;
const plan = planOf(gridded.id);
if (plan?.kind !== 'plates' || plan.plates?.planned !== true) throw new Error(`the map was not planned in the worker: ${JSON.stringify(plan)?.slice(0, 300)}`);
// the folder verbs: a folder made, the copy moved into it, trashed and put back, the folder renamed
if (call('newFolder', '', { root: 0, dir: '', name: 'rooms' }).dir !== 'rooms') throw new Error('newFolder did not make rooms');
const moved = call('moveTile', mine.id, { root: 0, dir: 'rooms' });
if (!moved.path.startsWith('/memory/My tiles/rooms/')) throw new Error(`moveTile did not move the copy: ${JSON.stringify(moved)}`);
const trashed = call('trashTile', moved.id ?? moved.path, {});
if (!Number.isInteger(trashed.undo)) throw new Error(`trashTile did not number its undo: ${JSON.stringify(trashed)}`);
const back = call('undoTrash', '', { undo: trashed.undo });
if (back.path !== moved.path) throw new Error(`undoTrash did not put the copy back: ${JSON.stringify(back)}`);
if (call('renameFolder', '', { root: 0, dir: 'rooms', name: 'halls' }).dir !== 'halls') throw new Error('renameFolder did not rename rooms');
let readOnly = '';
try { call('newFolder', '', { root: 1, dir: '', name: 'x' }); } catch (e) { readOnly = String(e.message ?? e); }
if (JSON.parse(readOnly).status !== 403) throw new Error(`a folder in the library was not refused: ${readOnly}`);
// every verb the page's table says the worker answers is a row of the service: asked with no body it refuses the request (400), never "no such call"
const { WORKER_VERBS } = await import(new URL('../web/preview/js/worker-calls.js', import.meta.url).href);
for (const verb of WORKER_VERBS) {
  try {
    service.call(verb, path, '"not a request"');
  } catch (e) {
    if (String(e.message ?? e).includes('no such call')) throw new Error(`the page's table says the worker answers ${verb}, and the service has no such call`);
  }
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
    meshBytes: mesh.length,
  }),
);
