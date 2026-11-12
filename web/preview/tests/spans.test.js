import test from 'node:test';
import assert from 'node:assert/strict';

import { spansFresh } from '../js/spans.js';

const clean = { text: 'tile {}', checked: 'tile {}', ok: true, tileId: 't1', file: 'a.dtile', checkedFile: 'a.dtile', checkedTile: 't1' };

test('spans may be used when the text shown is the text checked cleanly', () => {
  assert.equal(spansFresh(clean, 't1'), true);
});

test('spans are refused once the text has moved past the check', () => {
  assert.equal(spansFresh({ ...clean, text: 'tile { }' }, 't1'), false);
});

test('spans are refused while the check found a mistake', () => {
  assert.equal(spansFresh({ ...clean, ok: false }, 't1'), false);
});

test('spans are refused for another file or tile than the one checked', () => {
  assert.equal(spansFresh({ ...clean, checkedFile: 'b.dtile' }, 't1'), false);
  assert.equal(spansFresh({ ...clean, checkedTile: 't0' }, 't1'), false);
  assert.equal(spansFresh(clean, 't2'), false);
});

test('spans are refused before anything was checked', () => {
  assert.equal(spansFresh({ ...clean, checked: null }, 't1'), false);
  assert.equal(spansFresh({ ...clean, text: null, checked: null }, 't1'), false);
});
