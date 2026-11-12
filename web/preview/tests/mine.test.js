import test from 'node:test';
import assert from 'node:assert/strict';
import { copiedNote, mapPos, patchOf, patchOnCopy } from '../js/mine.js';

const lib = 'tile size="1x1" seed=100 {\n    use "parts/grass"\n    stack { dirt }\n}\n';
const pinned = lib.replace('"parts/grass"', '"lib:parts/grass"');

test('a change is the middle that differs', () => {
  const at = lib.indexOf('seed=100') + 8;
  assert.deepEqual(patchOf(lib, lib.replace('seed=100', 'seed=1007')), { from: at, to: at, insert: '7' });
  assert.deepEqual(patchOf('abc', 'abc'), { from: 3, to: 3, insert: '' });
});

test('the typed key lands in the copy, after the text the copy pinned', () => {
  const at = lib.indexOf('100') + 3;
  const r = patchOnCopy({ from: at, to: at, insert: '7' }, lib, pinned);
  assert.equal(r.text, pinned.replace('seed=100', 'seed=1007'));
  assert.equal(r.text.slice(0, r.caret).endsWith('seed=1007'), true);
});

test('the same text maps every position to itself', () => {
  assert.equal(mapPos(12, lib, lib), 12);
  const r = patchOnCopy({ from: 5, to: 9, insert: 'Z' }, lib, lib);
  assert.equal(r.text, `${lib.slice(0, 5)}Z${lib.slice(9)}`);
});

test('a change inside what the copy rewrote is not guessed', () => {
  assert.equal(patchOnCopy({ from: 3, to: 3, insert: 'x' }, 'abcdef', 'abXYef'), null);
  // but a change beside what it pinned is carried across it
  const at = lib.indexOf('parts') + 2;
  assert.match(patchOnCopy({ from: at, to: at, insert: 'x' }, lib, pinned).text, /lib:paxrts/);
});

test('a value written with its unit is not given the unit again', async () => {
  const { carriesUnit } = await import('../js/lens.js');
  assert.equal(carriesUnit('3.5dmm..6.5dmm'), true);
  assert.equal(carriesUnit('20%'), true);
  assert.equal(carriesUnit('3.5 .. 6.5'), false);
  assert.equal(carriesUnit(''), false);
});

test('the toast says where the copy is, and whether the change came with it', () => {
  assert.match(copiedNote('grass-angle', true), /Copied to My tiles as grass-angle\. Your change is in the copy\./);
  assert.match(copiedNote('grass-angle', false), /Make your change there/);
});
