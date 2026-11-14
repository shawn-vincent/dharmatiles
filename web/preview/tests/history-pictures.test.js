// The history menu shows each step's effect (D28): a picture of the tile at the text each row leaves it at, built low by the program
// through the one thumbnail service (js/thumbs.js): kept by the text, cancelled when the menu closes, a mistake in a text said in place of a picture.
import test from 'node:test';
import assert from 'node:assert/strict';
import { install, until } from './dom.js';
import { createThumbs, thumbKey } from '../js/thumbs.js';
import { tierReady } from '../js/tiers.js';

/** A fake program that holds every want until told it is done, and remembers each post of the set. */
function fakeHost() {
  const posts = [];
  const keyOf = (w) => `${w.tile}|${w.tier}|${w.text ?? '-'}`;
  const host = {
    async tiersWant(body) { posts.push(body.wants.map(keyOf)); return { status: 200, body: { keys: body.wants.map(keyOf) } }; },
    async tierResult(key) { return new TextEncoder().encode(key).buffer; },
  };
  return { host, posts, wanted: () => posts.at(-1) };
}

test('a picture of a text is kept by the text, and the program is asked to build that text of that file', async () => {
  const a = { tile: 't', text: 'tile size="1x1" { stack { dirt } }', file: '/x/t.dtile' };
  assert.equal(thumbKey(a), thumbKey({ ...a }));
  assert.notEqual(thumbKey(a), thumbKey({ ...a, text: 'tile size="1x1" { stack { dirt rock } }' }), 'another text, another picture');
  assert.notEqual(thumbKey(a), thumbKey({ tile: 't' }), 'not the file as it is');
  const asked = [];
  const host = { async tiersWant(body) { asked.push(...body.wants); return { status: 200, body: { keys: ['k'] } }; }, async tierResult() { return new ArrayBuffer(1); } };
  const svc = createThumbs({ host, draw: () => 'url', scope: 'h1' });
  const p = svc.picture(a);
  await until(() => asked.length >= 1, 'the program to be asked');
  assert.deepEqual([asked[0].text, asked[0].file, asked[0].tier], [a.text, a.file, 'super-low']);
  tierReady({ key: 'k', docRev: 1 });
  assert.equal(await p, 'url');
});

test('a picture whose element goes away after it was asked is cancelled: its want leaves the set the program holds', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: () => 'u', scope: 'h2' });
  const a = { tile: 't', text: 'one' }, b = { tile: 't', text: 'two' };
  const pa = svc.picture(a), pb = svc.picture(b);
  await until(() => f.wanted()?.length === 2, 'both wants posted');
  assert.deepEqual(f.wanted().sort(), ['t|super-low|one', 't|super-low|two']);
  svc.release(b); // the menu closed
  await until(() => f.wanted().length === 1, 'the withdrawn want to leave the set');
  assert.deepEqual(f.wanted(), ['t|super-low|one'], 'the one nobody waits for is withdrawn, so the program cancels it');
  assert.equal(await pb, null);
  tierReady({ key: 't|super-low|one', docRev: 1 });
  assert.equal(await pa, 'u');
});

test('a text with a mistake in it rejects with the mistake marked, so the row says so and the page does not call it a failure', async () => {
  const f = fakeHost();
  const svc = createThumbs({ host: f.host, draw: () => 'u', scope: 'h3' });
  const p = svc.picture({ tile: 't', text: 'broken' });
  await until(() => f.posts.length >= 1, 'the program to be asked');
  tierReady({ key: 't|super-low|broken', docRev: 1, error: 'no element or definition is called `dirtt`', mistake: true });
  await assert.rejects(p, (e) => e.mistake === true && /dirtt/.test(e.message));
});

test('the menu draws the tile now and as each row leaves it, only when it is given pictures', async () => {
  const root = install();
  const { render, html } = await import('../js/view/html.js');
  const { HistoryMenu } = await import('../js/view/title-bar.js');
  const asked = [];
  const svc = { picture: async (s) => { asked.push(s.text); return null; }, release() {} };
  const items = [1, 2].map((target) => ({ entry: { label: `Changed seed on line ${target}`, change: null }, target, distance: 3 - target, direction: 'Undo', text: `state ${target}` }));
  render(html`<${HistoryMenu} items=${items} onPick=${() => {}} />`, root);
  await until(() => root.querySelectorAll('.action').length === 2, 'the two rows drawn');
  assert.equal(root.querySelectorAll('.pics').length, 0, 'a closed menu, or a part file, has no pictures');
  assert.equal(asked.length, 0);
  const pictures = { svc, now: 'state now', spec: (text) => ({ tile: 't', text }), onError: () => {} };
  render(html`<${HistoryMenu} items=${items} onPick=${() => {}} pictures=${pictures} />`, root);
  // the pictures are drawn at once; each asks for its picture from an effect, run after the paint: wait for the three asks
  await until(() => root.querySelectorAll('.pics').length === 2 && new Set(asked).size === 3, 'two pairs drawn and the three texts asked for');
  assert.equal(root.querySelectorAll('.pics').length, 2, 'one pair a row');
  assert.deepEqual([...new Set(asked)].sort(), ['state 1', 'state 2', 'state now'], 'the current text once, each row\'s text');
  assert.match(root.textContent, /Changed seed on line 1/, 'the label stays');
});
