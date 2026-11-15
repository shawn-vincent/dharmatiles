import test from 'node:test';
import assert from 'node:assert/strict';
import { workerHost, engineBase, siteBase } from '../js/host.js';

test('the engine is the website\'s own when the page is opened from it, else the one beside the page', () => {
  const here = 'https://shawn.example/dharmatiles/editor/index.html';
  assert.equal(engineBase('?host=worker', here), 'https://shawn.example/dharmatiles/editor/engine/');
  assert.equal(engineBase('?host=worker&site=%2E%2E', here), 'https://shawn.example/dharmatiles/engine/');
  assert.equal(engineBase('?site=%2E%2E%2F', here), 'https://shawn.example/dharmatiles/engine/');
  assert.equal(siteBase('?site=%2E%2E%2F', here), 'https://shawn.example/dharmatiles/');
  assert.equal(siteBase('?host=worker', here), null);
});

/**
 * A stand-in for the editor's worker (js/editor-worker.js): `answers` is what the service says to each verb, by name; the calls it
 * got are kept.  The protocol is the worker's own (editor-worker.js's typedefs).
 */
function fakeEngine(answers) {
  const calls = [];
  const streams = [];
  return {
    calls,
    stream: (event) => streams.forEach((f) => f(JSON.stringify(event))),
    ready: Promise.resolve({ id: 0, ok: true, events: '[]' }),
    onStream: (f) => streams.push(f),
    async ask(msg) {
      calls.push(msg);
      const a = answers[msg.op === 'call' || msg.op === 'bytes' ? msg.verb : msg.op];
      if (!a) throw new Error(`the fake engine has no answer for ${msg.op} ${msg.verb ?? ''}`);
      return a(msg);
    },
  };
}
const refused = (status, message, extra = {}) => {
  const e = new Error('the service refused');
  e.refusal = JSON.stringify({ status, body: { error: { kind: 'refused', message, help: null, causes: [], backtrace: '', context: { route: 'x', tile: null, version: 'v', atMs: 0 } }, ...extra } });
  e.events = '[]';
  return e;
};

test('a call is its verb and its request as JSON, about the tile by its id, and the answer is the page\'s reply', async () => {
  const engine = fakeEngine({ check: () => ({ ok: true, json: JSON.stringify({ ok: true, errors: [], nodes: [] }), events: '[]' }) });
  const host = workerHost(undefined, engine);
  const r = await host.check('abc', { text: 'tile {}' });
  assert.equal(r.status, 200);
  assert.equal(r.body.ok, true);
  assert.deepEqual(engine.calls[0], { op: 'call', verb: 'check', tile: 'abc', request: JSON.stringify({ text: 'tile {}' }) });
});

test('a refusal comes back as the HTTP error body: its status, its words, and what it says beside them', async () => {
  const engine = fakeEngine({ save: () => { throw refused(409, 'the file moved on', { text: 'newer', docRev: 7 }); } });
  const host = workerHost(undefined, engine);
  const r = await host.save('abc', { text: 'old', docRev: 1 });
  assert.equal(r.status, 409);
  assert.equal(r.body.error, 'the file moved on');
  assert.equal(r.body.text, 'newer');
  assert.equal(r.body.docRev, 7);
  assert.equal(r.error?.kind, 'refused');
});

test('what the service does not answer yet is refused in words, by name, never answered empty', async () => {
  const host = workerHost(undefined, fakeEngine({}));
  const r = await host.platesShow('list', 0);
  assert.equal(r.status, 501);
  assert.match(r.body.error, /the plates is not in the browser editor yet/);
  await assert.rejects(() => host.messagesText('abc'), /the messages as text is not in the browser editor yet/);
});

test('a trap of the engine is thrown loudly with its words, not turned into an answer', async () => {
  const engine = fakeEngine({ build: () => { throw new Error('unreachable executed'); } });
  const host = workerHost(undefined, engine);
  await assert.rejects(() => host.build('abc'), /unreachable executed/);
});

test('the stream starts at the snapshot; what came before it is in it, and what comes after is told in order', async () => {
  const told = [];
  const snapshot = { contract: 'c', rev: 5, epoch: 'worker', tiles: [], roots: [] };
  const engine = fakeEngine({ snapshot: () => ({ ok: true, json: JSON.stringify(snapshot), events: '[]' }) });
  const host = workerHost(undefined, engine);
  // an event from before the page connected is not told: the snapshot holds it
  engine.stream({ type: 'progress', rev: 0, id: 't', progress: { fraction: 0.1 } });
  host.connect({
    onSnapshot: (s) => told.push(['snapshot', s.rev]),
    onTile: (t) => told.push(['tile', t.id]),
    onProgress: (id) => told.push(['progress', id]),
    onMessage: () => {}, onState: (s) => told.push(['state', s]), onRev: (rev) => told.push(['rev', rev]),
  });
  await new Promise((r) => setTimeout(r, 20));
  engine.stream({ type: 'progress', rev: 0, id: 't2', progress: { fraction: 0.5 } });
  engine.stream({ type: 'tile', rev: 6, tile: { id: 't2' } });
  assert.deepEqual(told, [['state', 'live'], ['snapshot', 5], ['rev', 5], ['progress', 't2'], ['tile', 't2'], ['rev', 6]]);
});

test('a draft answers its mesh bytes, and a step its head and its mesh', async () => {
  const head = new TextEncoder().encode(JSON.stringify({ text: 't', rev: 'r', draft: 'mesh' }));
  const step = new Uint8Array(4 + head.length + 3);
  new DataView(step.buffer).setUint32(0, head.length);
  step.set(head, 4);
  step.set([1, 2, 3], 4 + head.length);
  const engine = fakeEngine({
    draft: () => ({ ok: true, bytes: new Uint8Array([9, 8, 7]), events: '[]' }),
    step: () => ({ ok: true, bytes: step, events: '[]' }),
  });
  const host = workerHost(undefined, engine);
  const d = await host.draft('abc', { text: 'tile {}' });
  assert.deepEqual([...new Uint8Array(d.buffer)], [9, 8, 7]);
  const s = await host.step('abc', { edit: {} });
  assert.equal(s.head.draft, 'mesh');
  assert.deepEqual([...new Uint8Array(s.buffer)], [1, 2, 3]);
});

test('the folder, trash and target calls are the service\'s verbs, with their requests as the page writes them', async () => {
  const ok = (body) => () => ({ ok: true, json: JSON.stringify(body), events: '[]' });
  const engine = fakeEngine({
    newFolder: ok({ rev: 2, dir: 'a' }), renameFolder: ok({ rev: 3, dir: 'b' }), trashFolder: ok({ rev: 4, dir: 'b', path: null, undo: 1 }),
    moveTile: ok({ path: 'p', id: null, tile: null, rev: 5 }), duplicateTile: ok({ path: 'p', id: null, tile: null, rev: 6 }),
    trashTile: ok({ rev: 7, dir: null, path: 'p', undo: 2 }), undoTrash: ok({ path: 'p', id: null, tile: null, rev: 8 }),
    rename: ok({ path: 'q', id: null, tile: null }), deleteTile: ok({ path: 'p' }), setTarget: ok({ system: 'ol', printer: 'default' }),
  });
  const host = workerHost(undefined, engine);
  const said = async (call) => { const r = await call; assert.equal(r.status, 200); const m = engine.calls.at(-1); return [m.verb, m.tile, JSON.parse(m.request)]; };
  assert.deepEqual(await said(host.newFolder(0, '', 'a')), ['newFolder', '', { root: 0, dir: '', name: 'a' }]);
  assert.deepEqual(await said(host.renameFolder(0, 'a', 'b')), ['renameFolder', '', { root: 0, dir: 'a', name: 'b' }]);
  assert.deepEqual(await said(host.trashFolder(0, 'b')), ['trashFolder', '', { root: 0, dir: 'b' }]);
  assert.deepEqual(await said(host.moveTile('t', { root: 0, dir: 'b' })), ['moveTile', 't', { root: 0, dir: 'b' }]);
  assert.deepEqual(await said(host.duplicateTile('t')), ['duplicateTile', 't', {}]);
  assert.deepEqual(await said(host.trashTile('t')), ['trashTile', 't', {}]);
  assert.deepEqual(await said(host.undoTrash(2)), ['undoTrash', '', { undo: 2 }]);
  assert.deepEqual(await said(host.rename('t', 'new')), ['rename', 't', { name: 'new' }]);
  assert.deepEqual(await said(host.deleteTile('t')), ['deleteTile', 't', {}]);
  assert.deepEqual(await said(host.setTarget({ system: 'ol' })), ['setTarget', '', { system: 'ol' }]);
});

test('the print list and map calls are the service\'s verbs, with the request the page writes and no tile of their own', async () => {
  const edited = { list: 'l', path: 'p.dmap', before: null, text: 't', docRev: 2, scene: null };
  const ok = (body) => () => ({ ok: true, json: JSON.stringify(body), events: '[]' });
  const answers = Object.fromEntries(['listsCount', 'listsRemove', 'listsRestore', 'mapsSet', 'mapsRemove', 'mapsUnpin', 'mapsUnpinAll'].map((v) => [v, ok(edited)]));
  const piece = { ...edited, id: 'p1', shifted: 0, seed: null };
  const engine = fakeEngine({
    ...answers,
    listsNew: ok({ path: 'p.dmap', id: null, tile: null }),
    listsAdd: ok({ ...edited, count: 1, made: false }),
    mapsAdd: ok(piece), mapsDup: ok(piece), mapsPin: ok(piece), mapsBatch: ok({ ...edited, changed: 1, shifted: 0 }),
  });
  const host = workerHost(undefined, engine);
  const said = async (call) => { const r = await call; assert.equal(r.status, 200); const m = engine.calls.at(-1); return [m.verb, m.tile, JSON.parse(m.request)]; };
  assert.deepEqual(await said(host.listsNew({ name: 'x', map: true })), ['listsNew', '', { name: 'x', map: true }]);
  assert.deepEqual(await said(host.listsAdd({ tile: 't', list: 'l', docRev: 1 })), ['listsAdd', '', { tile: 't', list: 'l', docRev: 1 }]);
  assert.deepEqual(await said(host.listsCount({ list: 'l', id: 'p1', count: 2, docRev: 2 })), ['listsCount', '', { list: 'l', id: 'p1', count: 2, docRev: 2 }]);
  assert.deepEqual(await said(host.listsRemove({ list: 'l', id: 'p1', docRev: 2 })), ['listsRemove', '', { list: 'l', id: 'p1', docRev: 2 }]);
  assert.deepEqual(await said(host.listsRestore({ list: 'l', text: 't', docRev: 2 })), ['listsRestore', '', { list: 'l', text: 't', docRev: 2 }]);
  assert.deepEqual(await said(host.mapsAdd({ list: 'l', tile: 't', at: '(0sq, 0sq)', docRev: 2 })), ['mapsAdd', '', { list: 'l', tile: 't', at: '(0sq, 0sq)', docRev: 2 }]);
  assert.equal((await said(host.mapsSet({ list: 'l', id: 'p1', set: { rot: 90 }, docRev: 2 })))[0], 'mapsSet');
  assert.equal((await said(host.mapsBatch({ list: 'l', edits: [], docRev: 2 })))[0], 'mapsBatch');
  assert.equal((await said(host.mapsDup({ list: 'l', id: 'p1', at: '(1sq, 0sq)', docRev: 2 })))[0], 'mapsDup');
  assert.equal((await said(host.mapsRemove({ list: 'l', id: 'p1', docRev: 2 })))[0], 'mapsRemove');
  assert.equal((await said(host.mapsPin({ list: 'l', id: 'p1', plate: 1, x: 1, y: 1, docRev: 2 })))[0], 'mapsPin');
  assert.equal((await said(host.mapsUnpin({ list: 'l', id: 'p1', docRev: 2 })))[0], 'mapsUnpin');
  assert.equal((await said(host.mapsUnpinAll({ list: 'l', docRev: 2 })))[0], 'mapsUnpinAll');
});
