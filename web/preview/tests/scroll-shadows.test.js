import test from 'node:test';
import assert from 'node:assert/strict';
import { edges } from '../js/scroll-shadows.js';

const box = (o) => ({ scrollTop: 0, scrollLeft: 0, clientHeight: 100, scrollHeight: 300, clientWidth: 100, scrollWidth: 100, ...o });

test('shadows follow the scroll position', () => {
  assert.deepEqual(edges(box({})), { t: false, b: true, l: false, r: false });
  assert.deepEqual(edges(box({ scrollTop: 90 })), { t: true, b: true, l: false, r: false });
  assert.deepEqual(edges(box({ scrollTop: 200 })), { t: true, b: false, l: false, r: false });
});
test('content that fits, and fractional rounding, show nothing', () => {
  assert.deepEqual(edges(box({ scrollHeight: 100 })), { t: false, b: false, l: false, r: false });
  assert.equal(edges(box({ scrollTop: 199.6 })).b, false);
});
test('horizontal edges', () => {
  const w = { scrollWidth: 300 };
  assert.deepEqual(edges(box({ ...w, scrollHeight: 100, scrollLeft: 50 })), { t: false, b: false, l: true, r: true });
});
