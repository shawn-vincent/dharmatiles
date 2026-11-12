import test from 'node:test';
import assert from 'node:assert/strict';
import { cloudAtCaret, cloudOf, cloudOnLine, cloudsIn, columnGeometry, shadeAlpha, SHADE_SHEET, sheetGeometry } from '../js/cloud.js';

const src = `tile size="1x1" {
    stack {
        // a comment: top="1dmm"
        dirt top="$groundHeight + bell(0dmm .. 2dmm)" {
            pin top="$groundHeight + 4dmm" at=hill
            pin top="$groundHeight - 1dmm" at=path
        }
        scatter count=3 { rock size="9dmm" top="$bottom + high(3dmm .. 9dmm)" }
        grass stop=4
    }
}
`;
const enc = new TextEncoder();
const byte = (i) => enc.encode(src.slice(0, i)).length;
const span = (from, to) => [byte(from), byte(to) - byte(from)];
const at = (s) => src.indexOf(s);
const cloud = (kind, from, to, pin = null) => ({ kind, file: 't', span: span(from, to), pin });
const lineEnd = (s) => src.indexOf('\n', at(s));
// what the program sends: each entry's clouds, read off what it parsed (the comment holds none)
const items = [
  {
    key: 'tile/dirt/0',
    clouds: [
      cloud('top', at('top="$groundHeight + bell'), src.indexOf('"', at('bell')) + 1),
      cloud('pin', at('pin top="$groundHeight + 4dmm"'), lineEnd('at=hill'), 0),
      cloud('pin', at('pin top="$groundHeight - 1dmm"'), lineEnd('at=path'), 1),
    ],
  },
  { key: 'tile/scatter/0', clouds: [cloud('top', at('top="$bottom'), src.indexOf('"', at('high(')) + 1)] },
  { key: 'tile/grass/0', clouds: [] },
];

test('the tops and pins the program sends for each entry, as places in the text', () => {
  const c = cloudsIn(src, items.map((i) => ({ ...i, clouds: i.clouds.map((x) => ({ ...x, file: 't' })) })), 't');
  assert.deepEqual(c.map((x) => [x.key, x.pin]), [['tile/dirt/0', null], ['tile/dirt/0', 0], ['tile/dirt/0', 1], ['tile/scatter/0', null]]);
  assert.equal(src.slice(c[0].from, c[0].to), 'top="$groundHeight + bell(0dmm .. 2dmm)"');
  assert.equal(src.slice(c[1].from, c[1].to), 'pin top="$groundHeight + 4dmm" at=hill');
});

test('a non-ASCII text moves the places by characters, not bytes', () => {
  const text = 'tile { // \u00e9\u00e9\n dirt top="1dmm" }';
  const from = enc.encode(text.slice(0, text.indexOf('top='))).length;
  const items2 = [{ key: 'k', clouds: [{ kind: 'top', file: 'f', span: [from, 10], pin: null }] }];
  const [c] = cloudsIn(text, items2, 'f');
  assert.equal(text.slice(c.from, c.to), 'top="1dmm"');
});

test('the caret in a top or a pin selects its cloud; elsewhere none', () => {
  const c = cloudsIn(src, items, 't');
  assert.equal(cloudAtCaret(c, at('bell')).key, 'tile/dirt/0');
  assert.equal(cloudAtCaret(c, at('at=path')).pin, 1);
  assert.equal(cloudAtCaret(c, at('high(')).key, 'tile/scatter/0');
  assert.equal(cloudAtCaret(c, at('size="9')), null);
  assert.equal(cloudAtCaret(c, at('count')), null);
});

test('another file, or no stack, has no clouds', () => {
  assert.deepEqual(cloudsIn(src, items, 'other'), []);
  assert.deepEqual(cloudsIn(src, [], 't'), []);
});

test('a hovered line finds the cloud written on it', () => {
  const c = cloudsIn(src, items, 't');
  const line = (s) => src.slice(0, at(s)).split('\n').length;
  assert.equal(cloudOnLine(src, c, line('at=hill')).pin, 0);
  assert.equal(cloudOnLine(src, c, line('grass')), null);
  assert.equal(cloudOnLine(src, c, line('// a comment')), null);
});

test("an entry's sheets and clouds at a point", () => {
  const d = { sheets: [{ owner: 'a' }, { owner: 'c' }], columns: [{ owner: 'c', class: 'stone-cloud', v: [] }] };
  assert.deepEqual(cloudOf(d, 'c'), { sheets: [{ owner: 'c' }], columns: [{ owner: 'c', class: 'stone-cloud', v: [] }] });
  assert.deepEqual(cloudOf(null, 'c'), { sheets: [], columns: [] });
});

test('a sheet carries each node shade to its triangles', () => {
  const g = sheetGeometry({ nx: 2, ny: 2, step: 1, z: [0, 0, 0, 0], shade: [0, 0.25, 0.5, 1] });
  assert.equal(g.shade.length, 6, 'one shade per triangle vertex');
  assert.deepEqual([...g.shade].slice(0, 3), [0, 0.25, 1]);
  const flat = sheetGeometry({ nx: 2, ny: 2, step: 1, z: [0, 0, 0, 0] });
  assert.ok([...flat.shade].every((v) => v === 0.5), 'no shade sent: flat odds');
});

test('a shade is an opacity, clear when a height never comes up', () => {
  assert.equal(shadeAlpha(0, SHADE_SHEET), SHADE_SHEET[0]);
  assert.equal(shadeAlpha(1, SHADE_SHEET), SHADE_SHEET[1]);
  assert.equal(shadeAlpha(7, SHADE_SHEET), SHADE_SHEET[1]);
});

test('clouds at a point are a line from low to high and a marker at the roll', () => {
  const g = columnGeometry([1, 2, 10, 20, 15, 0.1, 0.9, 0.5, 3, 4, 11, 21, 12, 0.5, 0.5, 0.5]);
  assert.equal(g.n, 2);
  assert.deepEqual([...g.lines].slice(0, 6), [1, 2, 10, 1, 2, 20]);
  assert.deepEqual([...g.marks].slice(0, 3), [1, 2, 15]);
  assert.equal(g.lineShade.length, 4);
});
