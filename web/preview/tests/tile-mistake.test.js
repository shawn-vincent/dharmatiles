// A tile with a mistake in its document (`dt::seam`, a line ending off a seam) that a picture, a map piece or a plate piece is asked of:
// it reaches the caller as the tile's mistake with its code, never as a failure of the page or the program ("kind=bug"); and a row's Copy
// is the program's one text of the message plus the environment.
import test from 'node:test';
import assert from 'node:assert/strict';
import { install } from './dom.js';
import { createThumbs, brokenWords, brokenCode } from '../js/thumbs.js';
import { tierReady } from '../js/tiers.js';
import { TileMistake } from '../js/server-error.js';
import { problems, dismiss, showError } from '../js/problems.js';
import { detailsText } from '../js/messages.js';

const host = () => ({
  async tiersWant(body) { return { status: 200, body: { keys: body.wants.map((w) => `k:${w.tile}`) } }; },
  async tierResult() { return new ArrayBuffer(1); },
});

test("a build that stopped on the document's mistake rejects every picture as the tile's mistake, with its code and message", async () => {
  install();
  const svc = createThumbs({ host: host(), draw: () => 'u', scope: 'tm1' });
  const p = svc.picture({ tile: 'broken' });
  await new Promise((r) => setTimeout(r, 10));
  tierReady({ key: 'k:broken', docRev: 1, error: 'column 1, row 1, west side, at 0.62 is not a seam', mistake: true, code: 'dt::seam' });
  await assert.rejects(p, (e) => e instanceof TileMistake && e.mistake === true && e.code === 'dt::seam' && /not a seam/.test(e.message));
  const e = new TileMistake('x is not a seam', 'dt::seam');
  assert.equal(brokenWords(e), 'dt::seam: x is not a seam');
  assert.equal(brokenCode(brokenWords(e)), 'dt::seam');
});

test("showError leaves a tile's mistake to the picture and the messages (never a bug entry); a real failure is still listed", () => {
  install();
  const before = problems().length;
  const info = console.info; console.info = () => {};
  showError('Could not draw a picture of a tile', new TileMistake('not a seam', 'dt::seam'));
  console.info = info;
  assert.equal(problems().length, before, 'a document mistake is not a failure');
  const err = console.error; console.error = () => {};
  showError('Could not draw a picture of a tile', new Error('the kernel failed'));
  console.error = err;
  assert.equal(problems().length, before + 1);
  dismiss();
});

test("a row's copy is the program's text of the message and then the environment: the mistake keeps its file, code, place, source and help", async () => {
  const text = [
    '15:01:03.388 error dt::seam first-plate broken.dtile:4:40',
    '  file: /My tiles/broken.dtile', '  seen from: first-plate', '  doing: the super-low build of broken, system db, printer default',
    '  column 1, row 1, west side, at 0.62 is not a seam', '  help: did you mean `wsw`?',
    '     4 |     line shore wobble="5dmm" { through "(0sq, 0.62sq)" }', '       ^^^^', '  kind: mistake (in the tile)',
  ].join('\n');
  const asked = [];
  const h = { messageText: async (seq) => { asked.push(seq); return `${text}\n`; } };
  const env = { version: '1.0.0+1', userAgent: 'test', page: 'http://x/', at: '2026-11-14T00:00:00Z' };
  const out = await detailsText(/** @type {any} */ ({ seq: 7, kind: 'error', severity: 'error', code: 'dt::seam', text: 'x', state: 'open' }), h, { env });
  assert.deepEqual(asked, [7]);
  for (const want of ['dt::seam', 'file: /My tiles/broken.dtile', 'broken.dtile:4:40', 'seen from: first-plate', 'doing: the super-low build', 'help: did you mean', '^^^^', 'kind: mistake', '1.0.0+1']) {
    assert.ok(out.includes(want), `${want} in\n${out}`);
  }
});

test("a bug's copy carries the program's kind line, causes and backtrace the same way", async () => {
  const text = "15:01 error bug first-plate\n  file: /x.dtile\n  doing: Could not draw a picture of a tile\n  boom\n  kind: bug (the program's own, found by the page)\n  caused by: inner\n  backtrace:\n    a\n    b";
  const out = await detailsText(/** @type {any} */ ({ seq: 9, kind: 'server' }), { messageText: async () => text }, { env: { version: '1.0.0' } });
  for (const want of ['kind: bug', 'caused by: inner', 'backtrace:', 'file: /x.dtile']) assert.ok(out.includes(want), want);
});
