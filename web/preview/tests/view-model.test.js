// The model (js/editor/model.js) and the map's panel (js/view/map-panel.js) against the real documents module, the
// real stores and the real Preact, with only the program and the browser's DOM faked: what the page shows is a
// function of the document, the resources and the view, whatever order the program's words arrive in.
import test, { mock } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { install, press, rendered, type, until } from './dom.js';

// ---- a fake program: one map, a revision that only rises, the nearest free square settled in its write -----------

const program = {
  rev: 1, seq: 0, history: [],
  pieces: [
    { id: 'p1', x: 0, rot: 0, seed: 11 }, { id: 'p2', x: 1, rot: 0, seed: 12 },
    { id: 't1', x: null, rot: 0, seed: 13 }, { id: 't2', x: null, rot: 0, seed: 14 },
  ],
  text() { return JSON.stringify(this.pieces); },
  scene() {
    return {
      docRev: this.rev, map: true, placed: this.pieces.filter((p) => p.x != null).length, printer: 'default', system: 'db', colour: 'single', toPrint: 4, have: 0, shelfExists: false, heights: [], grid: null,
      pieces: this.pieces.map((q) => ({
        id: q.id, name: q.id, tileId: 'tile1', seed: q.seed, seedUsed: q.seed, system: 'db', canon: 'lib:t', colour: 'single', print: 'missing', cols: 1, rows: 1,
        at: q.x == null ? null : [q.x * 35, 0], rot: q.rot, size: [35, 35], sq: 35, bottom: null, bottomMm: 0, count: 1, error: null, ownedSeeds: [], shelf: { state: 'need' },
      })),
    };
  },
  state() { return { id: 'm1', docRev: this.rev, plan: { scene: this.scene(), writable: true } }; },
  /** Every call is written against the revision it names, or refused. */
  answer(name, body) {
    if (body.docRev !== this.rev) return { status: 409, body: { error: { message: 'the file changed' }, docRev: this.rev, scene: this.scene() } };
    const before = this.text();
    if (name === 'mapsSet') {
      const q = this.pieces.find((p) => p.id === body.id);
      if (body.set.near) {
        const taken = new Set(this.pieces.filter((p) => p !== q && p.x != null).map((p) => p.x));
        let x = body.set.near.x;
        while (taken.has(x)) x += 1;
        q.x = x;
      }
      if (body.set.rot != null) q.rot = body.set.rot;
      if (body.set.seed != null) q.seed = body.set.seed;
    } else if (name === 'listsRestore') {
      this.pieces = JSON.parse(body.text);
    } else throw new Error(`the fake program has no call ${name}`);
    this.rev += 1;
    return { status: 200, body: { ok: true, before, text: this.text(), docRev: this.rev, scene: this.scene() } };
  },
};
// the program takes a moment, and answers in the order it was asked
const call = (name) => async (body) => { await new Promise((r) => setTimeout(r, 2)); return program.answer(name, structuredClone(body)); };
const host = { mapsSet: call('mapsSet'), mapsBatch: call('mapsBatch'), listsRestore: call('listsRestore'), listsCount: call('listsCount') };

// only what is the browser's is faked: the editor's core (the host, the 3D view, the toast) and the inventory panel
const toasts = [];
mock.module(new URL('../js/editor/app.js', import.meta.url).href, { namedExports: { $: () => null, host, viewer: {}, toast: (m) => toasts.push(m), pickView: () => {}, onMakeMine: () => {}, makeMine: () => false } });
mock.module(new URL('../js/editor/inventory.js', import.meta.url).href, { namedExports: { say: () => {} } });

const root = install();
const { docs, applyDocState } = await import('../js/editor/docs.js');
const { server, view } = await import('../js/editor/stores.js');
const model = await import('../js/editor/model.js');
const { steps } = await import('../js/steps.js');
const { render, html } = await import('../js/view/html.js');
const { MapLevel, PieceLevel, PropLevel } = await import('../js/view/map-panel.js');
// the levels as the page has them: each in its own section of the shell (which shows one of them at a time; here all are drawn)
const propBox = () => root.childNodes[2];
const MapPanel = ({ m, act }) => html`<div id="lens-map"><${MapLevel} m=${m} act=${act} /></div><div id="lens-piece"><${PieceLevel} m=${m} act=${act} /></div><div id="lens-prop"><${PropLevel} m=${m} act=${act} /></div>`;

server.set({ tiles: [{ id: 'm1', path: '/home/m.dmap', name: 'm', status: 'ok', errors: [], root: 0 }], ready: true });
view.set({ tileId: 'm1' });
applyDocState(program.state());

const place = (id) => docs.edit('m1', () => ({ label: 'Could not place the piece', steps: [steps.mapsSet({ id, set: { near: { x: 0, y: 0 }, rot: 0 } })] }));
const turn = (id, rot) => docs.edit('m1', () => ({ label: 'Could not turn the piece', steps: [steps.mapsSet({ id, set: { rot } })] }));
const at = () => model.pieces.value.map((p) => [p.id, p.at ? p.at[0] / 35 : null]);

test('the model is derived: the pieces on the map are the document\'s, and the mode is the document\'s', () => {
  assert.equal(model.mapOn.value, true);
  assert.deepEqual(at(), [['p1', 0], ['p2', 1], ['t1', null], ['t2', null]]);
  assert.equal(model.level.value, 'map');
});

test('two quick Places: both are made, one after the other, and end on two squares (the program settles them)', async () => {
  const a = place('t1'), b = place('t2'); // asked in the same moment
  await Promise.all([a, b]);
  const squares = model.pieces.value.filter((p) => p.at).map((p) => p.at[0] / 35);
  assert.equal(new Set(squares).size, 4, `four pieces on four squares: ${squares}`);
  assert.deepEqual(model.pieces.value.filter((p) => p.id.startsWith('t')).map((p) => p.at && p.at[0] / 35), [2, 3]);
  assert.equal(docs.get('m1').docRev, program.rev, 'the page holds the program\'s revision');
});

test('an old answer arriving after a new one does not put the old map back', () => {
  const old = program.state();
  const rev = program.rev;
  program.answer('mapsSet', { docRev: rev, id: 'p1', set: { rot: 90 } });
  assert.equal(applyDocState(program.state()), true);
  assert.equal(model.pieces.value[0].rot, 90);
  assert.equal(applyDocState(old), false, 'older: not taken');
  assert.equal(model.pieces.value[0].rot, 90, 'what the panel reads is still the newest');
});

test('Undo after a map edit puts the map back through the program, and the model follows; Redo puts it forward', async () => {
  await turn('p2', 180);
  assert.equal(model.pieces.value.find((p) => p.id === 'p2').rot, 180);
  await docs.step('m1', -1);
  assert.equal(model.pieces.value.find((p) => p.id === 'p2').rot, 0, 'undone');
  await docs.step('m1', 1);
  assert.equal(model.pieces.value.find((p) => p.id === 'p2').rot, 180, 'redone');
});

test('the panel draws the model and is drawn again when the model changes, with the same markup as before', async () => {
  const acts = [];
  const act = new Proxy({}, { get: (_t, name) => (...args) => acts.push([name, ...args]) });
  render(html`<${MapPanel} m=${model.mapPanel} act=${act} />`, root);
  await until(() => /On the map/.test(root.textContent), 'the map level drawn');
  assert.match(root.textContent, /On the map/);
  assert.equal(root.querySelectorAll('.mp-group').length, 1, 'one tile, one group');
  // a piece selected in the view: the panel is on its level, by itself
  view.set({ msel: ['p1'] });
  await until(() => /Turn/.test(root.textContent) && root.querySelector('.mp-title'), 'the piece level drawn');
  assert.match(root.textContent, /Turn/);
  assert.ok(root.querySelector('.mp-title'), 'the piece\'s own title (the trail above the panel is the page\'s: levels.js)');
  // the program tells a new state: the panel follows without anyone asking it to
  program.answer('mapsSet', { docRev: program.rev, id: 'p1', set: { rot: 270 } });
  applyDocState(program.state());
  await until(() => /270°/.test(root.textContent), 'the turn drawn');
  assert.match(root.textContent, /270°/);
  view.set({ msel: [] });
  await until(() => /On the map/.test(root.textContent), 'the map level drawn again');
  assert.match(root.textContent, /On the map/);
});

test('a half-typed field survives the program telling the panel a new state, and Enter commits what was typed', async () => {
  const acts = [];
  const act = new Proxy({}, { get: (_t, name) => (...args) => acts.push([name, ...args]) });
  render(html`<${MapPanel} m=${model.mapPanel} act=${act} />`, root);
  view.set({ msel: ['p1'], mprop: 'seed' });
  await until(() => propBox().querySelector('input'), 'the seed field drawn');
  const input = propBox().querySelector('input');
  assert.ok(input, 'the seed field');
  assert.equal(input.value, '11');
  type(input, '4'); // half of 42
  // the program says the document changed (a build finished, an outside change): the same panel is drawn again
  program.answer('mapsSet', { docRev: program.rev, id: 'p2', set: { rot: 90 } });
  applyDocState(program.state());
  await until(() => model.pieces.value.find((p) => p.id === 'p2').rot === 90, 'the model to take the program\'s state');
  await rendered(); // the panel is drawn from the model in a microtask: one tick and it has been
  assert.equal(propBox().querySelector('input'), input, 'the same field, not a new one');
  assert.equal(input.value, '4', 'what the user typed is still there');
  // the word the program holds for the field changes under the typing: still the user's until they commit or leave
  program.answer('mapsSet', { docRev: program.rev, id: 'p1', set: { seed: 99 } });
  applyDocState(program.state());
  await until(() => model.pieces.value.find((p) => p.id === 'p1').seedUsed === 99, 'the model to take the program\'s seed');
  await rendered();
  assert.equal(input.value, '4');
  type(input, '42');
  press(input, 'Enter');
  assert.deepEqual(acts.at(-1).slice(0, 3), ['setPiece', 'p1', { seed: 42 }]);
  await until(() => input.value === '99', 'the field to show the model\'s word again');
  assert.equal(input.value, '99', 'committed: the field shows the model\'s word again');
  view.set({ msel: [], mprop: null });
  await rendered();
});

test('leaving the field without committing drops what was typed', async () => {
  const act = new Proxy({}, { get: () => () => {} });
  render(html`<${MapPanel} m=${model.mapPanel} act=${act} />`, root);
  view.set({ msel: ['p1'], mprop: 'seed' });
  await until(() => propBox().querySelector('input'), 'the seed field drawn');
  const input = propBox().querySelector('input');
  const seed = String(model.pieces.value.find((p) => p.id === 'p1').seedUsed);
  type(input, '7');
  assert.equal(input.value, '7');
  input.dispatchEvent({ type: 'blur', target: input });
  await until(() => input.value === seed, 'the typed 7 to be dropped for the model\'s seed');
  assert.equal(input.value, seed);
  view.set({ msel: [], mprop: null });
});

test('a piece\'s Seed level shows the same seed strip as the tile level: five candidates, the current first, one write when one is chosen', async () => {
  const acts = [];
  const act = new Proxy({}, { get: (_t, name) => (...args) => acts.push([name, ...args]) });
  const asked = [];
  // the model's own strip, with a pictures service that only counts what is asked (the real one asks the program)
  const strip = (p) => ({ svc: { picture: async (s) => { asked.push(s); return null; }, release() {} }, spec: { tile: 't1' }, current: p.seedUsed, owned: new Set([p.seedUsed]), onError: (e) => { throw e; } });
  render(html`<${MapPanel} m=${{ ...model.mapPanel, seedStrip: strip }} act=${act} />`, root);
  view.set({ msel: ['p1'], mprop: 'seed' });
  await until(() => propBox().querySelector('.dice'), 'the Seed level drawn, with its dice');
  assert.equal(propBox().querySelectorAll('.seed-card').length, 0, 'nothing is built until the dice is pressed');
  assert.equal(asked.length, 0);
  propBox().querySelector('.dice').dispatchEvent({ type: 'click' });
  // the cards are drawn at once, but each asks for its picture from an effect, which preact runs after the paint: wait for both
  await until(() => propBox().querySelectorAll('.seed-card').length === 5 && asked.length === 5, 'five cards drawn and five pictures asked for');
  const cards = propBox().querySelectorAll('.seed-card');
  assert.equal(cards.length, 5);
  const now = model.pieces.value.find((p) => p.id === 'p1').seedUsed;
  assert.match(cards[0].textContent, new RegExp(`${now}now`));
  assert.equal(propBox().querySelectorAll('.own').length, 1, 'the seed you own is marked');
  assert.equal(asked.length, 5);
  const picked = Number(cards[3].textContent.match(/\d+/)[0]);
  cards[3].dispatchEvent({ type: 'click' });
  await until(() => acts.length && propBox().querySelectorAll('.seed-card').length === 0, 'the seed written and the strip closed');
  assert.deepEqual(acts.at(-1).slice(0, 3), ['setPiece', 'p1', { seed: picked }]);
  assert.equal(propBox().querySelectorAll('.seed-card').length, 0, 'the strip closes');
  view.set({ msel: [], mprop: null });
});

test('a panel keeps no second copy of what it shows: no signature, no hand-made DOM, no stored view', () => {
  for (const f of ['../js/view/map-panel.js', '../js/view/field.js', '../js/editor/scene-panel.js', '../js/editor/model.js']) {
    const src = fs.readFileSync(new URL(f, import.meta.url), 'utf8').replace(/\/\/.*$/gm, '');
    assert.doesNotMatch(src, /createElement|innerHTML|replaceChildren|JSON\.stringify|\bdrawn\b/, `${f} draws by hand or by signature`);
  }
});

test('the source shown is one fact, the held source\'s id: view.textFile is gone', () => {
  const src = fs.readFileSync(new URL('../js/editor/stores.js', import.meta.url), 'utf8');
  assert.doesNotMatch(src, /textFile/);
  assert.match(src, /source: null,/);
});
