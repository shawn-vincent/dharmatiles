// The title bar at phone width (400 px): its children are more than a phone row holds (the lotus, Tile/Map/Plates, Undo and Redo
// with their menus, the target, Export, the panel buttons), so below 560 px the bar wraps into rows instead of running off the
// screen or stacking the mode buttons over the view.  The layout itself is CSS, so this checks the rules that make it:
// checked by eye in headless Chrome at 400 and 1512, light and dark (out/ink-scratch, see the changelog).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('../style.css', import.meta.url), 'utf8');
const block = (q) => {
  const at = css.indexOf(q);
  assert.ok(at >= 0, `${q} rules exist`);
  let depth = 0, i = css.indexOf('{', at);
  const start = i;
  for (; i < css.length; i++) { if (css[i] === '{') depth++; else if (css[i] === '}' && --depth === 0) break; }
  return css.slice(start, i + 1);
};

test('on a phone the title bar wraps: the bar grows, the mode buttons stay in a row, the target starts the second row', () => {
  const phone = block('@media (max-width: 560px)');
  assert.match(phone, /#title \{[^}]*flex-wrap: wrap/);
  assert.match(phone, /grid-template-rows: auto /, 'the bar is as tall as its rows, not a fixed 48px');
  assert.match(phone, /#title \.seg[^{]*\{[^}]*white-space: nowrap/, 'Tile, Map and Plates never stack');
  assert.match(phone, /#title \.grow \{[^}]*flex: 0 0 100%/, 'the spacer is the line break');
});
