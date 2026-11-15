// The service under Node: load the wasm bundle, put one library tile into the worker's `Service`, and call `check`,
// `save` and `build` by verb, as the page's worker will: `build` is HTTP's verb (it queues the job, the worker host
// runs it, and the record arrives on the event stream). Prints one JSON line: what the calls answered. Usage:
//   node service.mjs <bundle-dir> <tile>@<system>
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [dir, job] = process.argv.slice(2);
// each phase's time goes to stderr (the run's own record of where its time went)
let lapAt = performance.now();
const lap = (what) => { const now = performance.now(); console.error(`service.mjs ${what}: ${((now - lapAt) / 1000).toFixed(1)} s`); lapAt = now; };
const [tile, system] = job.split('@');
const manifest = JSON.parse(readFileSync(join(dir, 'engine.json'), 'utf8'));
const engine = await import(pathToFileURL(join(dir, manifest.js)).href);
await engine.default({ module_or_path: readFileSync(join(dir, manifest.wasm)) });
const libraryText = readFileSync(join(dir, manifest.library), 'utf8');
lap('load');
engine.set_library(libraryText);
const text = JSON.parse(libraryText)[tile];
if (text === undefined) throw new Error(`${tile} is not in the library`);

// the worker's host hears a build's progress as it happens: one StreamEvent a stage
const streamed = [];
globalThis.dtStream = (json) => streamed.push(JSON.parse(json));
// the worker's one store of kept builds, memory alone here (the page's worker also holds it in IndexedDB: js/kept-idb.js)
const kept = new engine.KeptBuilds('parity');
const service = new engine.Service(crypto.getRandomValues(new Uint8Array(32)), kept);
service.set_target(system);
const path = `/memory/My tiles/${tile.split('/').pop()}`;
service.put(path, text);
const checked = JSON.parse(service.call('check', path, JSON.stringify({ text })));
if (!checked.ok) throw new Error(`check: ${tile} does not read: ${JSON.stringify(checked.errors)}`);
// a save of the same text names the revision it was made from, and answers the revision the file has now
lap('check+save');
const saved = JSON.parse(service.call('save', path, JSON.stringify({ text, docRev: 1 })));
// build: the same verb as HTTP's. It answers that it was asked; the record comes on the stream.
const accepted = service.call('build', path, '{}');
if (accepted !== '{}') throw new Error(`build did not answer Accepted: ${accepted}`);
lap('build');
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
// the worker keeps what it built (dt-store's one stored-build function over the worker's memory): the text changed and put back builds
// twice, the changed text made and the original read from the kept builds, no tile built twice
const saveText = (t, rev) => JSON.parse(service.call('save', path, JSON.stringify({ text: t, docRev: rev })));
const lastReady = () => JSON.parse(service.events()).filter((e) => e.type === 'tile' && e.tile.status === 'ready').at(-1);
const changed = saveText(`${text}\n// changed\n`, saved.docRev);
const madeNew = lastReady();
if (madeNew?.tile.record.cache.state !== 'miss') throw new Error(`a changed text was not built: ${JSON.stringify(madeNew?.tile.record.cache)}`);
const restored = saveText(text, changed.docRev);
const readBack = lastReady();
if (readBack?.tile.record.cache.state !== 'hit') throw new Error(`the original text was not read from the kept builds: ${JSON.stringify(readBack?.tile.record.cache)}`);
if (JSON.stringify(readBack.tile.record.audit) !== JSON.stringify(built.record.audit)) throw new Error('the kept build is not the first build');
// the debug layer rides on `build`: the page's switch on collects the marks (the tile builds again with them), and the mesh carries them
const meshHead = (m) => JSON.parse(new TextDecoder().decode(m.subarray(8, 8 + new DataView(m.buffer, m.byteOffset).getUint32(4, true))));
if (meshHead(service.mesh(path, restored.docRev)).debug != null) throw new Error('marks were collected with the layer off');
if (service.call('build', path, JSON.stringify({ debug: true })) !== '{}') throw new Error('build with debug did not answer Accepted');
const marked = lastReady();
if (marked?.tile.record.cache.state !== 'miss') throw new Error(`the build with the layer on was not made again: ${JSON.stringify(marked?.tile.record.cache)}`);
if (meshHead(service.mesh(path, restored.docRev)).debug == null) throw new Error('the build with the layer on carries no marks');
// the reading verbs, answered with no tile and no build: the page's startup and typing calls
const about = JSON.parse(service.call('about', '', '{}'));
if (!about.version || !about.formats) throw new Error(`about says nothing: ${JSON.stringify(about)}`);
if (!JSON.parse(service.call('elements', '', '{}')).some((e) => e.name === 'rock')) throw new Error('the element schema has no rock');
if (!JSON.parse(service.call('names', '', JSON.stringify({ text }))).ok) throw new Error('names did not read the tile');
const read = JSON.parse(service.call('text', path, '{}'));
if (read.text !== text || !read.writable) throw new Error('text did not give the stored document');
// a draft is the mesh's bytes (the same payload), asked for with call_bytes; a text with a mistake is a refusal that carries its status
lap('reading verbs, mesh');
const draft = service.call_bytes('draft', path, JSON.stringify({ text }));
if (new TextDecoder().decode(draft.subarray(0, 4)) !== 'DTM2') throw new Error('the draft is not a DTM2 payload');
let mistake = '';
try { service.call_bytes('draft', path, JSON.stringify({ text: 'tile {' })); } catch (e) { mistake = String(e.message ?? e); }
if (JSON.parse(mistake).status !== 422) throw new Error(`a draft of a mistake was not a 422: ${mistake}`);
// the first message of a stream, and a library tile copied into My tiles by its id
lap('draft');
const snapshot = JSON.parse(service.snapshot());
if (!snapshot.roots.some((r) => r.kind === 'library') || snapshot.tiles.length === 0) throw new Error('the snapshot lists no library');
// the copies are of a soil tile: a copy in My tiles is built by the worker as soon as it is listed, and the lists, maps, tiers and
// folder moves below are the same calls whatever the tile (the tile under test is built once, by `build`)
const SOIL = 'lib:tiles/ground/1x1-soil.dtile';
const mine = JSON.parse(service.call('mine', SOIL, '{}'));
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
// exports: the tile's files and the print list's plates are downloads (a name, a media type, base64 bytes), made by the one
// function the program writes them with; the printer is set to one that makes STL plates (a Bambu project needs a template
// file, which a web page has no folder for: refused by name)
const sha = (b) => createHash('sha256').update(b).digest('hex');
const bytesOf = (d) => Buffer.from(d.data, 'base64');
lap('lists and maps');
// the lane built this tile above (`build`), under the tile's own path: an export of it, at the same options, is the same key, so it reads
// that build (a hit) and keeps nothing more
const keptBefore = kept.len();
const exported = call('exportTile', path, { threemf: true });
if (exported.build.cache.state !== 'hit') throw new Error(`the export did not read the lane's build of the same tile: ${JSON.stringify(exported.build.cache)}`);
if (kept.len() !== keptBefore) throw new Error(`the export kept another entry (${keptBefore} before, ${kept.len()} after): a second key for one tile`);
const stlDownload = exported.downloads.find((d) => d.name.endsWith('.stl'));
if (!stlDownload || stlDownload.mediaType !== 'model/stl' || exported.files.length !== 0) throw new Error(`exportTile gave no STL download: ${JSON.stringify(exported).slice(0, 300)}`);
if (bytesOf(exported.downloads.find((d) => d.name.endsWith('.3mf'))).subarray(0, 2).toString() !== 'PK') throw new Error('the 3MF download is not a zip');
if (call('exportTile', path, { where: true }).out !== 'downloads') throw new Error('where did not say downloads');
lap('exportTile');
// the plates are of a soil tile (a plate of full grass tiles is a long build in a browser); the program lays out the same list
const soil = JSON.parse(service.call('mine', SOIL, '{}'));
const soilList = call('listsAdd', '', { tile: soil.id });
const listPath = () => soilList.path;
const listDoc = () => JSON.parse(service.call('text', soilList.list, '{}'));
call('platesSet', soilList.list, { list: soilList.list, key: 'printer', value: 'sovol/sv06', docRev: listDoc().docRev });
// each plate set is made once (a plate of full grass tiles is a long build); an STL's 80-byte header names the document it was
// made from (the list's path, which is not the same where the program runs it), so its hash is of what follows
const plateSet = (format) => call('platesExport', '', { list: soilList.list, format }).downloads;
const hashes = (files) => Object.fromEntries(files.map((d) => [d.name, sha(d.name.endsWith('.stl') ? bytesOf(d).subarray(80) : bytesOf(d))]));
const stlPlates = plateSet('stl');
const threePlates = plateSet('3mf');
// SERVICE_DUMP=<folder> keeps the files the service made, to compare with the program's by eye
if (process.env.SERVICE_DUMP) {
  const { writeFileSync } = await import('node:fs');
  for (const d of [...exported.downloads, ...stlPlates, ...threePlates]) writeFileSync(join(process.env.SERVICE_DUMP, d.name), bytesOf(d));
}
lap('plates stl+3mf');
const platesStl = hashes(stlPlates);
const plates3mf = hashes(threePlates);
if (Object.keys(platesStl).length === 0 || Object.keys(plates3mf).length === 0) throw new Error('platesExport made no plates');
let noTemplate = '';
try { call('platesExport', '', { list: gridded.id }); } catch (e) { noTemplate = String(e.message ?? e); }
if (JSON.parse(noTemplate).status !== 422 || !noTemplate.includes('Bambu Studio project')) throw new Error(`a Bambu plate set was not refused by name: ${noTemplate.slice(0, 300)}`);
const listFile = listPath();
const listText = listDoc().text;
// the map's tiers, built in the call and kept: the page asks, is told on the stream, then reads the mesh by key and revision
const tierKey = call('tiersWant', '', { scope: 'node', wants: [{ tile: mine.id, tier: 'low' }] }).keys[0];
const tierEvent = JSON.parse(service.events()).find((e) => e.type === 'tier' && e.key === tierKey);
if (!tierEvent || tierEvent.error) throw new Error(`no tier was announced for ${tierKey}: ${JSON.stringify(tierEvent)}`);
const tierMesh = service.call_bytes('tierResult', '', JSON.stringify({ key: tierKey, docRev: tierEvent.docRev }));
if (new TextDecoder().decode(tierMesh.subarray(0, 4)) !== 'DTM2') throw new Error('the kept tier is not a DTM2 payload');
if (service.call_bytes('tierResult', '', JSON.stringify({ key: tierKey, docRev: tierEvent.docRev + 9 })).length !== 0) throw new Error('a tier of another revision was given');
lap('tiers');
// the messages as text
if (typeof call('messages', '', {}).text !== 'string') throw new Error('messages did not answer text');
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
lap('folders, verbs');
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
    exportedStlSha256: sha(bytesOf(stlDownload).subarray(80)),
    listFile: listFile.split('/').pop(),
    tileFile: soil.path.split('/').pop(),
    tileSource: SOIL,
    listText,
    platesStl,
    plates3mf,
  }),
);
