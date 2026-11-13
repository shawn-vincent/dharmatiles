import test from 'node:test';
import assert from 'node:assert/strict';
import { groupPrinters, selectionOf, sizeWords } from '../js/printers.js';

const P = (name, brand, model, extra = {}) => ({ name, brand, model, bed: [256, 256], slots: 1, mine: false, ...extra });
const LIST = [
  { name: 'default', brand: null, model: null, bed: null, slots: 1, mine: false },
  P('bambu/x1c', 'Bambu Lab', 'X1 Carbon'),
  P('bambu/x1c-ams', 'Bambu Lab', 'X1 Carbon + AMS', { slots: 4 }),
  P('bambu/h2d', 'Bambu Lab', 'H2D', { verify: 'sources disagree' }),
  P('sovol/sv06', 'Sovol', 'SV06'),
  P('bambu-lab/p1p', 'Bambu Lab', 'P1P', { mine: true, from_slicer: 'Bambu Studio 2.9.1: P1P' }),
];

test('groups: yours first, then brands in order, floors-only last', () => {
  const g = groupPrinters(LIST);
  assert.deepEqual(g.map((x) => x.label), ['Yours', 'Bambu Lab', 'Sovol', 'Floors only']);
});

test('an AMS variant is the model\'s switch, not a model', () => {
  const bambu = groupPrinters(LIST).find((x) => x.label === 'Bambu Lab');
  assert.deepEqual(bambu.models.map((m) => m.title), ['X1 Carbon', 'H2D']);
  assert.equal(bambu.models[0].ams.name, 'bambu/x1c-ams');
  assert.deepEqual(selectionOf(LIST, 'bambu/x1c-ams'), { model: 'bambu/x1c', ams: true });
  assert.deepEqual(selectionOf(LIST, 'bambu/x1c'), { model: 'bambu/x1c', ams: false });
});

test('your own profiles carry their brand in the name, the others do not', () => {
  const g = groupPrinters(LIST);
  assert.equal(g[0].models[0].title, 'Bambu Lab P1P');
});

test('a variant with no plain model stays a model of its own', () => {
  const only = [P('a/m-ams', 'A', 'M + AMS')];
  assert.equal(groupPrinters(only)[0].models.length, 1);
  assert.deepEqual(selectionOf(only, 'a/m-ams'), { model: 'a/m-ams', ams: false });
});

test('the plates chooser needs a bed: floors-only profiles are left out', () => {
  assert.ok(!groupPrinters(LIST, { needBed: true }).some((g) => g.kind === 'floors'));
});

test('a search takes every word, over brand, model and name', () => {
  assert.deepEqual(groupPrinters(LIST, { query: 'bambu carbon' }).flatMap((g) => g.models.map((m) => m.name)), ['bambu/x1c']);
  assert.deepEqual(groupPrinters(LIST, { query: 'zzz' }), []);
});

test('a profile that does not read is listed, not dropped', () => {
  const g = groupPrinters([...LIST, { name: 'mine/bad', brand: null, model: null, bed: null, mine: true, broken: 'no bed' }], { needBed: true });
  assert.ok(g[0].models.some((m) => m.name === 'mine/bad'));
});

test('the size in words', () => {
  assert.equal(sizeWords(P('x', 'a', 'b', { slots: 4 })), '256 × 256 mm · 4 filaments');
  assert.equal(sizeWords(LIST[0]), 'printability floors only');
});
