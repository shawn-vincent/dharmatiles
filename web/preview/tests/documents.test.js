import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createDocuments } from '../js/documents.js';
import { createHistory } from '../js/doc-history.js';
import { piecesOf, printerOf, selectedOf, verdictOf } from '../js/doc-views.js';

// ---- a fake program: one map, its revisions, its answers and its event stream ---------------------------------

const PRINTERS = [{ name: 'default', label: 'Default' }, { name: 'bambu/x1c', label: 'Bambu X1C' }, { name: 'sovol/sv06', label: 'Sovol SV06' }];

/** The program's side of one map: pieces by id, a printer, a revision that only rises. */
function fakeProgram(id, pieces) {
  const p = { id, rev: 1, pieces: structuredClone(pieces), printer: 'default', seq: 0, refused: 0 };
  p.sceneOf = () => ({
    docRev: p.rev, map: true, placed: p.pieces.length, printer: p.printer, system: 'db', colour: 'single',
    pieces: p.pieces.map((q) => ({ id: q.id, name: q.id, tileId: 't1', seed: null, system: 'db', canon: 'lib:t', seedUsed: 1, colour: 'single', print: 'missing', at: [q.x * 35, 0], rot: q.rot, size: [35, 35], sq: 35, bottomMm: 0, count: 1, error: null })),
  });
  p.platesOf = () => ({ printer: p.printer, label: PRINTERS.find((x) => x.name === p.printer).label, pieces: p.pieces.length, rev: p.rev });
  p.text = () => JSON.stringify({ printer: p.printer, pieces: p.pieces });
  p.load = (t) => { const d = JSON.parse(t); p.printer = d.printer; p.pieces = d.pieces; };
  p.next = () => `p${++p.seq + 100}`;
  /** The change is written: a new revision. */
  p.write = (mutate) => { const before = p.text(); mutate(); p.rev += 1; return before; };
  p.state = () => ({ id, docRev: p.rev, scene: p.sceneOf(), plates: p.platesOf() });
  /** The program's answer to a call (the page's host calls). */
  p.answer = (name, body) => {
    const stale = body.docRev !== p.rev;
    if (stale) { p.refused += 1; return { status: 409, body: { error: { message: 'the file changed' }, text: p.text(), docRev: p.rev, scene: p.sceneOf() } }; }
    let before;
    if (name === 'mapsBatch') {
      for (const e of body.edits) if (!p.pieces.some((q) => q.id === e.id)) return { status: 400, body: { error: { message: `no piece ${e.id}` } } };
      before = p.write(() => { for (const e of body.edits) Object.assign(p.pieces.find((q) => q.id === e.id), e.set); });
    } else if (name === 'mapsDup') {
      const i = p.pieces.findIndex((q) => q.id === body.id);
      if (i < 0) return { status: 400, body: { error: { message: `no piece ${body.id}` } } };
      before = p.write(() => p.pieces.splice(i + 1, 0, { ...p.pieces[i], id: p.next(), x: p.pieces[i].x + 1 }));
    } else if (name === 'mapsRemove') {
      if (!p.pieces.some((q) => q.id === body.id)) return { status: 400, body: { error: { message: `no piece ${body.id}` } } };
      before = p.write(() => { p.pieces = p.pieces.filter((q) => q.id !== body.id); });
    } else if (name === 'platesSet') {
      before = p.write(() => { p.printer = body.value; });
    } else if (name === 'listsRestore') {
      before = p.write(() => p.load(body.text));
    } else throw new Error(`the fake program has no call ${name}`);
    return { status: 200, body: { ok: true, list: id, before, text: p.text(), docRev: p.rev, scene: p.sceneOf() } };
  };
  return p;
}

const mulberry = (a) => () => { a |= 0; a = (a + 0x6D2B79F5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };

const pieces3 = [{ id: 'p1', x: 0, rot: 0 }, { id: 'p2', x: 1, rot: 0 }, { id: 'p3', x: 2, rot: 0 }];

/** A page's documents against the fake program, with the answers and events delivered by a virtual clock. */
function world(seed) {
  const rand = mulberry(seed);
  const program = fakeProgram('m1', pieces3);
  const clock = { now: 0, tasks: [], order: 0 };
  const later = (delay, fn) => clock.tasks.push({ at: clock.now + delay, n: clock.order++, fn });
  const reports = { refusals: 0, failures: 0, notes: 0 };
  const drawn = [];
  const { docs, receive } = createDocuments({
    send: (name, body) => new Promise((resolve) => {
      // the call reaches the program after a while, which answers at once; the answer takes its own time back
      later(rand() * 30, () => { const a = program.answer(name, structuredClone(body)); stream(a); later(rand() * 30, () => resolve(a)); });
    }),
    report: { refusal: () => { reports.refusals += 1; }, failure: () => { reports.failures += 1; }, note: () => { reports.notes += 1; } },
  });
  /** What the program says on its event stream after a change: the scene first, then the whole plan, each after its own delay. */
  const stream = (a) => {
    if (a.status !== 200) return;
    const s = program.state();
    later(rand() * 60, () => receive({ id: 'm1', docRev: s.docRev, plan: { scene: s.scene, writable: true } }));
    later(30 + rand() * 90, () => receive({ id: 'm1', docRev: s.docRev, plan: { scene: s.scene, plates: s.plates, writable: true } }));
  };
  const heldRevs = [];
  docs.subscribe((id) => { heldRevs.push(docs.get(id).docRev); drawn.push(views(docs.get(id))); });
  return { rand, program, docs, receive, clock, later, reports, drawn, heldRevs, stream };
}

/** Everything the page draws of the map, worked out from a document state and nothing else. */
function views(state, selection = []) {
  const tiles = [{ id: 't1', meshVersion: 1 }];
  const ps = piecesOf(state?.plan?.scene ?? null, tiles);
  return {
    pieces: ps.map((p) => [p.id, p.rot, p.at?.[0]]),
    selected: selectedOf(selection, ps).map((p) => p.id),
    printer: printerOf(state, PRINTERS, { map: true }),
    verdict: verdictOf(state?.plan?.scene ?? null),
  };
}

async function run(w, until) {
  for (;;) {
    // let every promise the page is waiting on run before the clock moves
    for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
    const ready = w.clock.tasks.filter((t) => t.at <= until);
    if (!ready.length) { if (w.clock.tasks.length && until === Infinity) { until = Infinity; } else break; }
    ready.sort((a, b) => a.at - b.at || a.n - b.n);
    const t = ready[0];
    w.clock.tasks.splice(w.clock.tasks.indexOf(t), 1);
    w.clock.now = Math.max(w.clock.now, t.at);
    t.fn();
  }
}

// ---- the rules -------------------------------------------------------------------------------------------------

test('a state is taken only if its docRev is newer; the same docRev is a later part of the same plan', () => {
  const told = [];
  const { docs, receive } = createDocuments({ send: async () => ({ status: 200, body: {} }) });
  docs.subscribe((id) => told.push(docs.get(id).docRev));
  assert.equal(receive({ id: 'a', docRev: 3, plan: { scene: { n: 3 } } }), true);
  assert.equal(receive({ id: 'a', docRev: 2, plan: { scene: { n: 2 } } }), false, 'older: dropped');
  assert.equal(docs.get('a').plan.scene.n, 3);
  // the plates come after the scene, at the same revision, and keep it
  receive({ id: 'a', docRev: 3, plan: { plates: { p: 1 } } });
  assert.deepEqual(docs.get('a').plan, { scene: { n: 3 }, plates: { p: 1 } });
  // an answer that carries only a scene, at a newer revision, takes the newer scene and keeps the plan it was given
  receive({ id: 'a', docRev: 4, scene: { n: 4 } });
  assert.equal(docs.get('a').plan.scene.n, 4);
  assert.deepEqual(told, [3, 3, 4], 'each state taken is told once');
  assert.throws(() => receive({ id: 'a', plan: {} }), /docRev/);
  assert.throws(() => { docs.get('a').plan.scene = 1; }, TypeError, 'a state is frozen: nobody writes it');
});

test('an edit names the document and the revision held when it is made; the answer is taken before the next step', async () => {
  const sent = [];
  let rev = 5;
  const { docs, receive } = createDocuments({
    send: async (name, body) => { sent.push([name, body]); rev += 1; return { status: 200, body: { ok: true, docRev: rev, before: 'a', text: 'b', scene: { docRev: rev } } }; },
  });
  receive({ id: 'm', docRev: 5, plan: { scene: { docRev: 5 } } });
  const r = await docs.edit('m', () => ({ label: 'two steps', steps: [{ name: 'mapsSet', body: { id: 'p1' } }, { name: 'mapsSet', body: (s) => ({ id: 'p2', sawRev: s.docRev }) }] }));
  assert.deepEqual(sent.map(([, b]) => [b.list, b.docRev]), [['m', 5], ['m', 6]]);
  assert.equal(sent[1][1].sawRev, 6, 'a step sees the state the earlier one left');
  assert.equal(r.entry.docRev, 7);
  assert.equal(docs.get('m').docRev, 7);
});

test('actions on one document run one after another and each is built from the state it finds', async () => {
  const w = world(1);
  w.receive(w.program.state() && { id: 'm1', docRev: 1, plan: { scene: w.program.sceneOf() } });
  const turn = (id) => w.docs.edit('m1', (s) => {
    const p = s.plan.scene.pieces.find((q) => q.id === id);
    return { label: 'turn', steps: [{ name: 'mapsBatch', body: { edits: [{ id, set: { rot: (p.rot + 90) % 360 } }] } }] };
  });
  // two quick turns of one piece: both are asked before either has been answered
  const a = turn('p2'), b = turn('p2');
  await run(w, Infinity);
  await Promise.all([a, b]);
  assert.equal(w.program.pieces.find((q) => q.id === 'p2').rot, 180, 'the second turn began from the first');
  assert.equal(w.program.refused, 0);
  assert.equal(w.docs.get('m1').docRev, w.program.rev);
});

test('a refusal takes the program\'s current state, is shown, and ends the history it could not honour', async () => {
  const w = world(2);
  w.receive({ id: 'm1', docRev: 1, plan: { scene: w.program.sceneOf() } });
  const first = w.docs.edit('m1', () => ({ label: 'turn', steps: [{ name: 'mapsBatch', body: { edits: [{ id: 'p1', set: { rot: 90 } }] } }] }));
  await run(w, Infinity); await first;
  assert.equal(w.docs.canUndo('m1'), true);
  // the file is changed from outside; the page has not heard
  w.program.write(() => { w.program.pieces = w.program.pieces.filter((q) => q.id !== 'p3'); });
  const r = await (async () => { const p = w.docs.edit('m1', () => ({ label: 'turn', steps: [{ name: 'mapsBatch', body: { edits: [{ id: 'p2', set: { rot: 90 } }] } }] })); await run(w, Infinity); return p; })();
  assert.equal(r, null);
  assert.equal(w.reports.refusals, 1);
  assert.equal(w.docs.get('m1').docRev, w.program.rev, 'the current state was taken');
  assert.deepEqual(w.docs.get('m1').plan.scene.pieces.map((p) => p.id), ['p1', 'p2']);
  assert.equal(w.docs.canUndo('m1'), false, 'the history could not be honoured: it ends');
  assert.equal(w.program.pieces.find((q) => q.id === 'p2').rot, 0, 'nothing was written');
});

test('Undo and Redo are calls against the revision the history left the file at; a toast\'s Undo only while its change is last', async () => {
  const w = world(3);
  w.receive({ id: 'm1', docRev: 1, plan: { scene: w.program.sceneOf() } });
  const change = async (label, id, rot) => { const p = w.docs.edit('m1', () => ({ label, steps: [{ name: 'mapsBatch', body: { edits: [{ id, set: { rot } }] } }] })); await run(w, Infinity); return p; };
  const one = await change('one', 'p1', 90);
  const two = await change('two', 'p2', 180);
  const settle = async (p) => { await run(w, Infinity); return p; };
  assert.equal(await settle(w.docs.undoIf('m1', one.entry)), false, 'not the last change: nothing is written');
  assert.equal(w.program.pieces[0].rot, 90);
  assert.equal(await settle(w.docs.undoIf('m1', two.entry)), true);
  assert.equal(w.program.pieces[1].rot, 0);
  assert.equal(await settle(w.docs.undo('m1')), true);
  assert.equal(w.program.pieces[0].rot, 0);
  assert.equal(await settle(w.docs.undo('m1')), false, 'nothing left to undo');
  assert.equal(await settle(w.docs.redo('m1')), true);
  assert.equal(await settle(w.docs.redo('m1')), true);
  assert.deepEqual(w.program.pieces.map((q) => q.rot), [90, 180, 0]);
  // a new change drops what was undone
  await settle(w.docs.undo('m1'));
  await change('three', 'p3', 270);
  assert.equal(w.docs.canRedo('m1'), false);
  assert.equal(w.docs.get('m1').docRev, w.program.rev);
});

test('the history keeps entries of one shape, for a source and for a map alike', () => {
  const h = createHistory();
  h.push('text:a', { before: 'x', after: 'xy', label: 'typed' });
  h.push('m', { before: 'one', after: 'two', label: 'turn', docRev: 4 });
  assert.equal(h.head('m'), 4);
  assert.equal(h.stateText('text:a', 0), 'x');
  assert.equal(h.stateText('text:a', 1), 'xy');
  h.amend('text:a', { after: 'xyz' });
  assert.equal(h.last('text:a').after, 'xyz');
  h.moved('m', -1, 5);
  assert.deepEqual([h.canUndo('m'), h.canRedo('m'), h.head('m')], [false, true, 5]);
  h.clear('m');
  assert.equal(h.has('m'), false);
});

// ---- one way in ------------------------------------------------------------------------------------------------

test('only the event stream imports the setter of a document\'s state', () => {
  const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'js');
  const files = [];
  const walk = (d) => { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name); if (e.isDirectory()) { if (e.name !== 'vendor' && e.name !== 'viewer') walk(p); } else if (p.endsWith('.js')) files.push(p); } };
  walk(root);
  const users = files.filter((f) => /\bapplyDocState\b/.test(fs.readFileSync(f, 'utf8'))).map((f) => path.relative(root, f)).sort();
  assert.deepEqual(users, ['editor/docs.js', 'editor/live.js'], 'the setter is made in docs.js and used by live.js (the program\'s word) and nothing else');
  // the core hands the setter out apart from the readers and actions: `docs` has no way to set a state
  const { docs } = createDocuments({ send: async () => ({ status: 200, body: {} }) });
  for (const k of Object.keys(docs)) assert.ok(!/^(receive|apply|set|commit)/i.test(k), `docs.${k} must not set a state`);
  // and nothing outside live.js reaches for what an old page held on to
  const old = ['takeScene', 'currentScene', 'undoLast', 'carriedScene'];
  for (const f of files) {
    const text = fs.readFileSync(f, 'utf8');
    for (const w of old) assert.ok(!new RegExp(`\\b${w}\\b`).test(text), `${path.relative(root, f)} still uses ${w}`);
    // a tile's plan is its summary's; a print list's or a map's is its document's (planOf, docs.scene)
    assert.ok(!/\b(t|tile\(\))\??\.plan\??\.(scene|plates)\b/.test(text.replace(/\/\/.*$/gm, '')), `${path.relative(root, f)} reads a list's plan from its tile: ask planOf(t) or docs.scene(id)`);
  }
});

// ---- checked, not trusted --------------------------------------------------------------------------------------

/** One random session: actions made at random times, answers and stream events arriving in random order and after random delays. */
async function session(seed) {
  const w = world(seed);
  const { rand, program, docs, clock } = w;
  w.receive({ id: 'm1', docRev: 1, plan: { scene: program.sceneOf(), plates: program.platesOf(), writable: true } });
  let selection = [];
  const steps = 40 + Math.floor(rand() * 30);
  const pending = [];
  for (let k = 0; k < steps; k++) {
    const when = clock.now + rand() * 40;
    const choice = rand();
    // the user's actions are made at `when` against the page's state then, by id
    w.later(when - clock.now, () => {
      const state = docs.get('m1');
      const ids = state.plan.scene.pieces.map((p) => p.id);
      const pick = () => ids[Math.floor(rand() * ids.length)];
      if (choice < 0.25 && ids.length) {
        const id = pick();
        pending.push(docs.edit('m1', (s) => {
          const p = s.plan.scene.pieces.find((q) => q.id === id);
          return p ? { label: 'turn', steps: [{ name: 'mapsBatch', body: { edits: [{ id, set: { rot: (p.rot + 90) % 360 } }] } }] } : null;
        }));
      } else if (choice < 0.35 && ids.length) {
        const id = pick();
        pending.push(docs.edit('m1', (s) => (s.plan.scene.pieces.some((q) => q.id === id) ? { label: 'copy', steps: [{ name: 'mapsDup', body: { id } }] } : null)));
      } else if (choice < 0.45 && ids.length > 1) {
        const id = pick();
        pending.push(docs.edit('m1', (s) => (s.plan.scene.pieces.some((q) => q.id === id) ? { label: 'remove', steps: [{ name: 'mapsRemove', body: { id } }] } : null)));
      } else if (choice < 0.55) {
        const printer = PRINTERS[Math.floor(rand() * PRINTERS.length)].name;
        pending.push(docs.edit('m1', () => ({ label: 'printer', steps: [{ name: 'platesSet', body: { key: 'printer', value: printer } }] })));
      } else if (choice < 0.7) pending.push(docs.undo('m1'));
      else if (choice < 0.8) pending.push(docs.redo('m1'));
      else if (choice < 0.9) {
        // the file is changed from outside; the stream says so, after a while
        program.write(() => { const q = program.pieces[Math.floor(rand() * program.pieces.length)]; if (q) q.rot = (q.rot + 180) % 360; if (rand() < 0.3) program.pieces.push({ id: program.next(), x: 5, rot: 0 }); });
        w.stream({ status: 200 });
      } else selection = rand() < 0.5 ? [] : [pick(), pick()].filter(Boolean);
    });
  }
  await run(w, Infinity);
  await Promise.all(pending);
  await run(w, Infinity);

  const held = docs.get('m1');
  const truth = program.state();
  // one order: the revision held only rose, and it is the program's
  for (let i = 1; i < w.heldRevs.length; i++) assert.ok(w.heldRevs[i] >= w.heldRevs[i - 1], `seed ${seed}: the held revision went back`);
  assert.equal(held.docRev, truth.docRev, `seed ${seed}: the page holds the program's revision`);
  // the state, whole: the scene and the plates of the newest revision, whatever order they came in
  assert.deepEqual(held.plan.scene, truth.scene, `seed ${seed}: the scene`);
  assert.deepEqual(held.plan.plates, truth.plates, `seed ${seed}: the plates`);
  // one way out: every view equals a fresh working-out from the newest state, and the last thing drawn was it
  const fresh = views({ id: 'm1', docRev: truth.docRev, plan: { scene: truth.scene, plates: truth.plates } }, selection);
  assert.deepEqual(views(held, selection), fresh, `seed ${seed}: the views`);
  const last = w.drawn.at(-1);
  assert.deepEqual({ ...last, selected: [] }, { ...fresh, selected: [] }, `seed ${seed}: what was last drawn is what the newest state draws`);
  assert.equal(fresh.printer.name, truth.scene.printer, `seed ${seed}: the chip names the document's printer`);
  assert.equal(fresh.verdict.startsWith(`${truth.scene.pieces.length} piece`), true);
  // a selection names ids: those the map no longer has select nothing
  for (const id of fresh.selected) assert.ok(truth.scene.pieces.some((p) => p.id === id));
  return { refusals: w.reports.refusals, failures: w.reports.failures, revs: truth.docRev };
}

test('random actions with answers and events arriving in any order leave the page showing the newest state (seeded)', async () => {
  let refusals = 0, revs = 0;
  for (let seed = 1; seed <= 150; seed++) {
    const r = await session(seed);
    refusals += r.refusals; revs += r.revs;
  }
  // the sessions really did race: edits were refused for a file that had moved on, and the file moved a lot
  assert.ok(refusals > 20, `only ${refusals} refusals: the sessions did not race`);
  assert.ok(revs > 1000, `only ${revs} revisions in all`);
});

// ---- a source is a document too: the text on disk, its revision, and the saves made against it ---------------

/** The program's side of one file: its text, a revision that only rises, and what it was asked. */
function fakeFile(text = 'a') {
  const f = { text, rev: 1, saves: [], told: new Set([1]), pending: false, violations: [] };
  f.read = async () => { f.pending = false; return { file: 'f.dtile', text: f.text, writable: true, docRev: f.rev }; };
  f.outside = (t) => { f.text = t; f.rev += 1; };
  f.answer = (name, body) => {
    assert.equal(name, 'save');
    f.saves.push(body.docRev);
    if (body.docRev !== f.rev) f.pending = true;
    else if (f.pending) f.violations.push(`a save was written over the disk's text after a refusal, with no choice made: ${body.text}`);
    if (body.docRev !== f.rev) return { status: 409, body: { error: { message: 'moved' }, text: f.text, docRev: f.rev } };
    if (body.text !== f.text) { f.text = body.text; f.rev += 1; }
    return { status: 200, body: { docRev: f.rev } };
  };
  return f;
}

function sourceWorld(file) {
  const said = [];
  const { docs, receive } = createDocuments({ send: async (n, b) => file.answer(n, b), report: { failure: (l) => said.push(l) } });
  return { docs, receive, said };
}

test('a source is read into the document and saved against the revision its text is of', async () => {
  const f = fakeFile('one');
  const { docs } = sourceWorld(f);
  const st = await docs.loadSource('t1', { tile: 't1', own: true }, f.read);
  assert.equal(st.source.text, 'one');
  assert.equal(st.source.rev, 1);
  assert.deepEqual(await docs.saveSource('t1', 'two'), { status: 'saved', docRev: 2 });
  assert.equal(docs.get('t1').source.text, 'two');
  assert.equal(docs.get('t1').source.rev, 2);
  assert.deepEqual(f.saves, [1]);
});

test('the program saying the file changed does not let a save through against text that was not read', async () => {
  const f = fakeFile('one');
  const { docs, receive } = sourceWorld(f);
  await docs.loadSource('t1', { tile: 't1', own: true }, f.read);
  f.outside('theirs');
  receive({ id: 't1', docRev: f.rev, plan: { printer: 'x' } }); // the stream tells the revision, not the text
  assert.equal(docs.get('t1').docRev, 2);
  assert.equal(docs.get('t1').source.rev, 1, 'the text held is still the revision it was read at');
  const r = await docs.saveSource('t1', 'mine');
  assert.equal(r.status, 'conflict', 'the save is made against revision 1 and refused');
  assert.equal(r.disk, 'theirs');
  assert.equal(f.text, 'theirs', 'nothing was overwritten');
  assert.equal(docs.get('t1').source.text, 'theirs', 'the disk\'s text is what is held now');
  docs.resolveConflict('t1'); // the user chose to keep theirs
  assert.deepEqual(await docs.saveSource('t1', 'mine'), { status: 'saved', docRev: 3 });
});

test('while a conflict stands nothing is saved; choosing ends it, and keep-mine saves against the disk\'s revision', async () => {
  const f = fakeFile('one');
  const { docs } = sourceWorld(f);
  await docs.loadSource('t1', { tile: 't1', own: true }, f.read);
  assert.equal((await docs.saveSource('t1', 'two')).status, 'saved');
  f.outside('theirs');
  assert.equal((await docs.saveSource('t1', 'three')).status, 'conflict');
  const asked = f.saves.length;
  // the user types on, without choosing: nothing is sent, the disk keeps its text
  assert.equal((await docs.saveSource('t1', 'four')).status, 'conflict');
  assert.equal((await docs.saveSource('t1', 'five')).status, 'conflict');
  assert.equal(f.saves.length, asked, 'no save reached the program');
  assert.equal(f.text, 'theirs');
  assert.equal(docs.resolveConflict('t1'), 'theirs');
  f.pending = false; // the user has chosen
  assert.deepEqual(await docs.saveSource('t1', 'five'), { status: 'saved', docRev: 4 }, 'keep mine: the page\'s text against the disk\'s revision');
  assert.equal(f.text, 'five');
  assert.deepEqual(f.violations, []);
});

test('saves of one document run one after another, each from the revision the last left', async () => {
  const f = fakeFile('a');
  const { docs } = sourceWorld(f);
  await docs.loadSource('t1', { tile: 't1', own: true }, f.read);
  const rs = await Promise.all(['b', 'c', 'd'].map((t) => docs.saveSource('t1', t)));
  assert.deepEqual(rs.map((r) => r.status), ['saved', 'saved', 'saved']);
  assert.deepEqual(f.saves, [1, 2, 3]);
  assert.equal(f.text, 'd');
});

test('a library file has no revision and what it says now replaces what was held; it cannot be saved', async () => {
  const f = { text: 'v1' };
  const read = async () => ({ file: 'lib:a.dtile', text: f.text, writable: false, docRev: 0 });
  const { docs } = sourceWorld(fakeFile());
  await docs.loadSource('lib', { tile: 'lib', own: true }, read);
  f.text = 'v2';
  const st = await docs.loadSource('lib', { tile: 'lib', own: true }, read);
  assert.equal(st.source.text, 'v2');
  assert.equal((await docs.saveSource('lib', 'x')).status, 'unwritable');
});

test('random reads, saves and outside changes never save against text the page has not read (seeded)', async () => {
  for (let seed = 1; seed <= 60; seed++) {
    const rand = mulberry(seed);
    const f = fakeFile('t0');
    const sent = [];
    const { docs, receive } = createDocuments({ send: async (n, b) => { sent.push(b.docRev); return f.answer(n, b); }, report: {} });
    const readAt = new Set();
    const read = async () => { const v = await f.read(); readAt.add(v.docRev); return v; };
    await docs.loadSource('t1', { tile: 't1', own: true }, read);
    const ops = [];
    let n = 0;
    for (let i = 0; i < 25; i++) {
      const k = rand();
      if (k < 0.4) ops.push(docs.saveSource('t1', `s${++n}`).then((r) => { if (r.status === 'saved' || r.status === 'conflict') readAt.add(r.docRev); }));
      else if (k < 0.6) f.outside(`o${++n}`);
      else if (k < 0.7) receive({ id: 't1', docRev: f.rev, plan: { n } });
      else if (k < 0.8) { if (docs.resolveConflict('t1') != null) f.pending = false; }
      else ops.push(docs.loadSource('t1', { tile: 't1', own: true }, read));
      if (rand() < 0.5) await Promise.resolve();
    }
    await Promise.all(ops);
    assert.deepEqual(f.violations, [], `seed ${seed}`);
    for (const r of sent) assert.ok(readAt.has(r), `seed ${seed}: a save was made against revision ${r}, which the page never had the text of`);
    const h = docs.get('t1');
    assert.ok(h.source.rev <= f.rev && h.docRev <= f.rev, `seed ${seed}`);
  }
});

test('a source is written only through the documents module (one way out)', () => {
  const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'js');
  const found = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { if (e.name !== 'vendor') walk(p); }
      else if (p.endsWith('.js') && /\bhost\.save\(/.test(fs.readFileSync(p, 'utf8'))) found.push(path.relative(dir, p));
    }
  };
  walk(dir);
  assert.deepEqual(found.sort(), ['editor/docs.js'], `host.save is called only where the documents module is wired: ${found}`);
});

test('doc.tileId and doc.file are the held source\'s, written by one function (no second path to which file is shown)', () => {
  const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'js', 'editor');
  const bad = [];
  for (const f of fs.readdirSync(dir)) {
    if (!f.endsWith('.js')) continue;
    const src = fs.readFileSync(path.join(dir, f), 'utf8');
    // a doc.set({ ... tileId: or file: ... }) written by hand
    for (const m of src.matchAll(/\bdoc\.set\(\{([^}]*)\}\)/g)) if (/\b(tileId|file)\s*:/.test(m[1])) bad.push(`${f}: ${m[0].slice(0, 80)}`);
  }
  assert.deepEqual(bad, [], 'only sourceFields (editor/document.js) says which tile and file the text is of');
});

test('what shows a source changes in the same step as its state is taken: no reader sees the new state with the old pane', async () => {
  const { signal, effect } = await import('@preact/signals-core');
  const f = fakeFile('one');
  const { docs } = sourceWorld(f);
  const pane = signal('');      // what the pane shows
  const seen = [];
  effect(() => { seen.push([docs.get('t1')?.source?.text ?? null, pane.value]); }); // a reader of both, as the page's derived fields are
  await docs.loadSource('t1', { tile: 't1', own: true }, f.read, () => true, (state) => { pane.value = state.source.text; });
  assert.deepEqual(seen.at(-1), ['one', 'one']);
  assert.ok(seen.every(([held, shown]) => held === shown || (held == null && shown === '')), `a mismatched pair was seen: ${JSON.stringify(seen)}`);
  // a refused load applies nothing
  const before = seen.length;
  await docs.loadSource('t1', { tile: 't1', own: true }, f.read, () => false, () => { pane.value = 'not taken'; });
  assert.equal(pane.value, 'one');
  assert.equal(seen.length, before);
});
