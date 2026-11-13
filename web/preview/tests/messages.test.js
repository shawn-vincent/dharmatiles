import test from 'node:test';
import assert from 'node:assert/strict';

import { FILTERS, MSG_MIN, countsText, createLog, detailsFor, filterRows, fitMsgHeight, jobWords, statusOf } from '../js/messages.js';

let n = 0;
const msg = (over = {}) => ({
  seq: ++n, t: '2026-11-12T10:41:40.771Z', kind: 'error', severity: 'error', scope: 'check', code: 'dt::look', tile: 'a', tile_name: 'tree-err',
  node: null, span: { file: 'tree-err.dtile', line: 8, col: 9, offset: 100, len: 20 }, text: 'the trunk leaves no room', help: 'make trunk height less',
  data: {}, state: 'open', ...over,
});
const note = (over = {}) => msg({ kind: 'note', severity: 'note', code: 'dt::steep', text: 'the dirt is forced to 64°', help: 'soften the heights', ...over });
const build = (over = {}) => msg({ kind: 'build', severity: 'info', scope: 'build', code: null, span: null, text: 'Built in 2.1 s', help: null, data: { build_s: 2.1, triangles: 162262 }, ...over });
const gate = (over = {}) => msg({ kind: 'gate', severity: 'info', scope: 'build', code: 'gate', span: null, text: 'Passes the printability gate', help: null, ...over });
const cleared = (m) => ({ ...m, state: 'cleared' });

test('open messages are rows, errors first and the newest first; the build\'s record is apart', () => {
  const log = createLog();
  const e1 = msg(), nt = note(), e2 = msg({ text: 'second' }), b = build(), g = gate();
  [e1, nt, e2, b, g].forEach((m) => log.apply(m));
  const v = log.view('a');
  assert.deepEqual(v.rows.map((r) => r.m.seq), [e2.seq, e1.seq, nt.seq]);
  assert.equal(v.record.build.seq, b.seq);
  assert.equal(v.record.gate.seq, g.seq);
  assert.deepEqual(v.counts, { errors: 2, notes: 1 });
  assert.equal(countsText(v.counts), '2 errors · 1 note');
  assert.equal(countsText({ errors: 0, notes: 0 }), '');
  assert.equal(log.view('other').rows.length, 0, 'another tile\'s view has none of these');
});

test('a cleared message leaves the rows', () => {
  const log = createLog();
  const e = msg();
  log.apply(e);
  log.apply(cleared(e));
  assert.equal(log.view('a').rows.length, 0);
  assert.deepEqual(log.view('a').counts, { errors: 0, notes: 0 });
});

test('while a build runs the last build\'s rows are dimmed; new ones are not; on finish they come back and what it cleared is one line', () => {
  const log = createLog();
  const fixed = msg({ text: 'fixed by this build', scope: 'build' }), still = note(), b = build();
  [fixed, still, b].forEach((m) => log.apply(m));
  log.beginBuild('a');
  let v = log.view('a');
  assert.equal(v.running, true);
  assert.deepEqual(v.rows.map((r) => r.dim), [true, true], 'from the last build');
  assert.equal(v.record.dim, true);
  const fresh = msg({ text: 'new in this build', scope: 'build' });
  log.apply(fresh);
  log.apply(cleared(fixed));
  v = log.view('a');
  assert.equal(v.rows.find((r) => r.m.seq === fresh.seq).dim, false, 'said during the build: full strength');
  assert.equal(v.rows.find((r) => r.m.seq === still.seq).dim, true);
  assert.equal(v.rows.some((r) => r.m.seq === fixed.seq), false);
  assert.deepEqual(v.cleared, [], 'nothing is said of what it cleared until it finishes');
  log.endBuild('a');
  v = log.view('a');
  assert.equal(v.running, false);
  assert.ok(v.rows.every((r) => !r.dim), 'still true: full strength again');
  assert.deepEqual(v.cleared.map((m) => m.seq), [fixed.seq], '1 cleared by this build');
  assert.equal(v.record.dim, false);
  // the next build that ends without clearing anything says so with no line
  log.beginBuild('a');
  log.endBuild('a');
  assert.deepEqual(log.view('a').cleared, []);
});

test('a message replayed as cleared (the stream came back) is not "cleared by this build"', () => {
  const log = createLog();
  log.beginBuild('a');
  log.apply(cleared(msg()));
  log.endBuild('a');
  assert.deepEqual(log.view('a').cleared, []);
});

test('a record (build, gate) cleared by a newer one is not counted as a problem cleared', () => {
  const log = createLog();
  const b = build();
  log.apply(b);
  log.beginBuild('a');
  log.apply(cleared(b));
  log.apply(build());
  log.endBuild('a');
  assert.deepEqual(log.view('a').cleared, []);
});

test('progress is never kept', () => {
  const log = createLog();
  log.apply(msg({ kind: 'progress', severity: 'info', scope: 'build', text: 'Building tree-err' }));
  assert.equal(log.all().length, 0);
  assert.equal(log.view('a').rows.length, 0);
});

test('a message of the page\'s own is kept with a seq below zero, and a dismissed one goes from the view only', () => {
  const log = createLog();
  const m = log.addLocal({ text: 'x is not a function', tile: 'a' });
  assert.ok(m.seq < 0);
  assert.equal(log.view('a').rows.length, 1);
  log.beginBuild('a');
  assert.equal(log.view('a').rows[0].dim, false, 'a page failure is not from a build');
  log.endBuild('a');
  log.dismiss(m.seq);
  assert.equal(log.view('a').rows.length, 0);
  assert.equal(log.byId(m.seq).text, 'x is not a function');
});

test('the filters keep errors, notes, or the build\'s own', () => {
  const log = createLog();
  [msg(), note(), build(), msg({ kind: 'export', severity: 'info', scope: 'export', text: 'Exported', span: null, code: null })].forEach((m) => log.apply(m));
  const rows = log.view('a').rows;
  assert.deepEqual(FILTERS.map((f) => f[0]), ['all', 'errors', 'notes', 'build']);
  assert.equal(filterRows(rows, 'all').length, 3);
  assert.deepEqual(filterRows(rows, 'errors').map((r) => r.m.kind), ['error']);
  assert.deepEqual(filterRows(rows, 'notes').map((r) => r.m.kind), ['note']);
  assert.deepEqual(filterRows(rows, 'build').map((r) => r.m.kind), ['export']);
});

test('badges and marks find their messages: by stack entry, and by file', () => {
  const log = createLog();
  const e = msg({ node: 'tile/tree/0' }), nt = note({ span: { file: 'lib:parts/edges.dtile', line: 13, col: 59, offset: 1, len: 2 } });
  [e, nt].forEach((m) => log.apply(m));
  assert.deepEqual(log.byNode('a').get('tile/tree/0').map((m) => m.seq), [e.seq]);
  assert.deepEqual(log.inFile('a', 'tree-err.dtile').map((m) => m.seq), [e.seq]);
  assert.deepEqual(log.inFile('a', 'lib:parts/edges.dtile').map((m) => m.seq), [nt.seq]);
  log.apply(cleared(e));
  assert.equal(log.byNode('a').size, 0, 'fix the cause and the badge goes with it');
});

test('the status line says the worst thing first, with the counts, and the build under way', () => {
  const log = createLog();
  const ready = { status: 'ready' };
  assert.equal(statusOf(log.view('a'), { t: ready }).verdict, 'passes');
  [build(), gate(), note()].forEach((m) => log.apply(m));
  let s = statusOf(log.view('a'), { t: ready });
  assert.deepEqual([s.verdict, s.tone, s.counts], ['passes · 2.10 s', 'note', '1 note']);
  const fail = gate({ severity: 'error', text: 'Fails the printability gate: holes' });
  log.apply(cleared(gate({ seq: 0 })));
  log.apply(fail);
  s = statusOf(log.view('a'), { t: ready });
  assert.equal(s.verdict, 'fails the gate');
  assert.equal(s.tone, 'bad');
  assert.equal(statusOf(log.view('a'), { t: { status: 'failed' } }).verdict, 'no build', 'a tile that failed with errors open has no build');
  assert.equal(statusOf(createLog().view('a'), { t: { status: 'failed' } }).verdict, 'build failed');
  log.beginBuild('a');
  s = statusOf(log.view('a'), { t: { status: 'building' }, p: { fraction: 0.44, label: 'grass' } });
  assert.deepEqual([s.verdict, s.tone, s.progress], ['building 44% · grass', 'busy', 0.44]);
  assert.equal(statusOf(log.view('a'), { t: ready, live: false }).verdict, 'reconnecting…');
});

test('a build the cache kept is said so: "rebuilding" while it is on show, "cached" when it came from there', () => {
  const log = createLog();
  log.beginBuild('a');
  const shown = { status: 'building', shown: { kind: 'earlier', note: 'the build made by 1.0.0 3 h ago' } };
  const s = statusOf(log.view('a'), { t: shown, p: { fraction: 0.2, label: 'grass' } });
  assert.deepEqual([s.verdict, s.tone], ['rebuilding 20% · grass', 'busy']);
  const own = statusOf(log.view('a'), { t: { status: 'building', shown: null }, p: { fraction: 0.2, label: 'grass' } });
  assert.equal(own.verdict, 'building 20% · grass', 'a tile with nothing kept on show is only building');
  const hit = createLog();
  hit.apply(build({ data: { build_s: 0.05, cache: 'hit', triangles: 10 }, text: 'From the cache (2.1 s when it was built)' }));
  assert.equal(statusOf(hit.view('a'), { t: { status: 'ready' } }).verdict, 'passes · cached');
});

test('the copy text of a tile\'s mistake is its code, where, words, help and the source line', () => {
  const env = { version: '1.0.0', system: 'db', printer: 'default', tile: 't', file: 'f', userAgent: 'x', platform: 'p', screen: 's', time: 'now', url: 'u' };
  const source = 'tile {\n    stack {\n        dirt\n        tree\n    }\n}\n';
  const text = detailsFor(msg({ span: { file: 'f', line: 4, col: 9, offset: 0, len: 1 } }), { source, env });
  assert.match(text, /^dharmatiles tile error/);
  assert.match(text, /Code:\s+dt::look/);
  assert.match(text, /Where:\s+f:4:9/);
  assert.match(text, /Said:\s+the trunk leaves no room/);
  assert.match(text, /Help:\s+make trunk height less/);
  assert.match(text, />    4 \|         tree/);
  // a note is the same report
  assert.match(detailsFor(note(), { env }), /Code:\s+dt::steep/);
});

test('the copy text of a failure of the program\'s or the page\'s is the whole chain, both stacks and the environment', () => {
  const env = { version: '1.0.0', system: 'db', printer: 'default', tile: 't', file: 'f', userAgent: 'x', platform: 'p', screen: 's', time: 'now', url: 'u' };
  const m = msg({
    kind: 'server', code: 'bug', span: null, text: 'a test panic', help: 'This is a bug in dharmatiles',
    data: { origin: 'server', kind: 'bug', route: 'POST /api/tiles/abc/save', causes: ['panicked at route.rs:1'], backtrace: '   0: dharmatiles::preview::route', version: '1.0.0' },
  });
  const text = detailsFor(m, { env });
  assert.match(text, /dharmatiles error report/);
  assert.match(text, /POST \/api\/tiles\/abc\/save/);
  assert.match(text, /a test panic/);
  assert.match(text, /panicked at route\.rs:1/);
  assert.match(text, /backtrace \(Rust\)/);
  assert.match(text, /dharmatiles\s+1\.0\.0/);
});

test('the messages panel drags between its minimum and leaving the view 80 px', () => {
  assert.equal(fitMsgHeight(300, 600), 300);
  assert.equal(fitMsgHeight(10, 600), MSG_MIN);
  assert.equal(fitMsgHeight(900, 600), 520);
  assert.equal(fitMsgHeight(250.6, 600), 251);
});

test('the gate\'s rows read the numbers under the names the program sends them (one fixture, held to by a program test)', async () => {
  const { readFileSync } = await import('node:fs');
  const { gateRows } = await import('../js/messages.js');
  const data = JSON.parse(readFileSync(new URL('./fixtures/gate-numbers.json', import.meta.url), 'utf8'));
  const rows = gateRows(data, { overhangDeg: 55, steepestDeg: 65 });
  const text = rows.map(([k, v]) => `${k}: ${v}`).join(' | ');
  assert.doesNotMatch(text, /NaN|undefined/, text);
  assert.deepEqual(rows.map((r) => r[0]), ['Triangles', 'Watertight', 'Band 55–65°', 'Past 65°', 'Dropped bodies', 'Voids filled', 'Holes']);
  assert.equal(rows[2][1], '1.25 / 3.50 mm²');
  assert.equal(rows[2][2], 'warn', 'some band area inside its budget is a warning');
  assert.equal(rows[3][1], '0.00 / 3.50 mm²');
  assert.equal(rows[3][2], 'ok');
  assert.equal(rows[5][1], 2);
  // past its budget is bad; a number the message does not carry is left out, not shown as NaN
  assert.equal(gateRows({ ...data, overhang_mm2: 5 }, { overhangDeg: 55, steepestDeg: 65 })[3][2], 'bad');
  assert.equal(gateRows({ triangles: 1, watertight: true }, { overhangDeg: 55, steepestDeg: 65 }).length, 2);
});

test('work in flight for a document is one record: a tile build and a map\'s pieces loading are said from it, and a tile build reads as it did', () => {
  const log = createLog();
  const tile = { id: 'a', name: 'first-plate', status: 'ready', shown: null };
  const building = { what: 'building', fraction: 0.45, label: 'grass', key: 'grass', count: ['blades', 120, 800], elapsedS: 3, etaS: 4, estimateS: 7 };
  const loading = { what: 'loading', fraction: 0.3, label: '1x1-grass', key: 'pieces', count: ['pieces', 3, 10], elapsedS: 3, etaS: 7, estimateS: 10 };
  // a tile build: the words it always had
  let s = statusOf(log.view('a'), { t: { ...tile, status: 'building' }, p: building });
  assert.equal(s.verdict, 'building 45% · grass');
  assert.equal(s.progress, 0.45);
  // a map's pieces: the same record, the same line, the same bar; the document itself is ready
  s = statusOf(log.view('a'), { t: tile, p: loading });
  assert.equal(s.verdict, 'loading 30% · 3 of 10 pieces · 1x1-grass');
  assert.equal(s.tone, 'busy');
  assert.equal(s.progress, 0.3);
  // the running row says it from the one function
  assert.equal(jobWords(building, 'x').title, 'Building x');
  assert.equal(jobWords(building, 'x').stage, 'grass: blades 120/800');
  assert.equal(jobWords(loading, 'first-plate').title, 'Loading the pieces of first-plate');
  assert.equal(jobWords(loading, 'first-plate').stage, '1x1-grass: pieces 3/10');
  // no job, no building: the verdict is the document's own
  assert.equal(statusOf(log.view('a'), { t: tile }).verdict, 'passes');
});

test('a build stage with no words yet shows its percent alone, never a separator with nothing after it', () => {
  const t = { status: 'building', name: 'x', shown: { checked: false } };
  assert.equal(statusOf(createLog().view('a'), { t, p: { fraction: 0.77, label: '', what: 'building' } }).verdict, 'rebuilding 77%');
  assert.equal(statusOf(createLog().view('a'), { t, p: { fraction: 0.5, label: 'grass', what: 'building' } }).verdict, 'rebuilding 50% · grass');
});
