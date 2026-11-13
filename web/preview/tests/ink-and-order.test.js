// "Ink and Order": the rules of one visual system for the editor, kept by tests that fail when the old form comes back.
// Four type sizes and no other; no all-caps labels; one set of corners; two control heights; one status mark; one icon set
// with no character standing in for an icon; the panels' showing and hiding animated by one function.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');
const style = read('../style.css');
const theme = read('../../../docs/assets/theme.css');
const sheets = [['style.css', style], ['theme.css', theme]];

const jsFiles = () => {
  const out = [['index.html', read('../index.html')]];
  const walk = (dir, rel) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.name === 'vendor') continue;
      if (e.isDirectory()) walk(new URL(`${e.name}/`, dir), `${rel}${e.name}/`);
      else if (e.name.endsWith('.js')) out.push([`${rel}${e.name}`, readFileSync(new URL(e.name, dir), 'utf8')]);
    }
  };
  walk(new URL('../js/', import.meta.url), 'js/');
  return out;
};

/** Every declaration of `prop` in a sheet with its line, comments removed. */
function declarations(css, prop) {
  const hits = [];
  css.split('\n').forEach((line, i) => {
    const bare = line.replace(/\/\*.*?\*\//g, '');
    for (const m of bare.matchAll(new RegExp(`(?:^|[;{\\s])${prop}\\s*:\\s*([^;}]*)`, 'g'))) hits.push({ line: i + 1, value: m[1].trim(), text: line.trim() });
  });
  return hits;
}

test('the type scale is four tokens, and no font-size or font shorthand of the page names a raw size', () => {
  for (const [name, css] of sheets) {
    const raw = [];
    for (const d of [...declarations(css, 'font-size'), ...declarations(css, 'font')]) {
      if (/^--/.test(d.text)) continue;
      // a size is a number with a unit: `var(--fs-*)` and `inherit` are not raw
      if (/\d+(\.\d+)?(px|rem|em|pt)\b/.test(d.value.replace(/var\([^)]*\)/g, ''))) raw.push(`${name}:${d.line}: ${d.text.slice(0, 110)}`);
    }
    assert.deepEqual(raw, [], `${name}: a raw font size outside the tokens`);
  }
  for (const t of ['--fs-s: 12px', '--fs-m: 14px', '--fs-l: 17px', '--fs-xl: 24px']) assert.ok(theme.includes(t), `theme.css defines ${t}`);
  const defined = [...theme.matchAll(/--fs-[a-z]+:\s*(\d+)px/g)].map((m) => Number(m[1]));
  assert.ok(Math.min(...defined) >= 12, '12 px is the floor');
  assert.equal(defined.length, 4, 'four sizes');
});

test('no all-caps label: sentence case is the one label (.cap), and no rule capitalises or tracks text', () => {
  for (const [name, css] of sheets) {
    assert.doesNotMatch(css, /text-transform\s*:\s*uppercase/, `${name} has an uppercase rule`);
  }
  assert.match(theme, /^\.cap \{[^}]*600 var\(--fs-s\)[^}]*var\(--ink\)/m, 'theme.css has the one .cap');
  // labels never track: no letter-spacing in the editor's sheet
  assert.deepEqual(declarations(style, 'letter-spacing').map((d) => d.text.slice(0, 60)), []);
});

test('one set of corners: --r, --r-lg and the mark corner; no pill, and a round thing says so', () => {
  const bad = [];
  for (const [name, css] of sheets) {
    css.split('\n').forEach((line, i) => {
      for (const m of line.matchAll(/border-radius\s*:\s*([^;}]*)/g)) {
        const round = /\/\*\s*round\s*\*\//.test(line);
        const rest = m[1].replace(/calc\(var\(--r\) - 1px\)/g, '').replace(/var\(--r(-lg|-mark)?\)/g, '').replace(/\b0\b/g, '').replace(/\s/g, '');
        if (rest === '' || (rest === '50%' && round)) continue;
        bad.push(`${name}:${i + 1}: border-radius: ${m[1].trim()}`);
      }
    });
  }
  assert.deepEqual(bad, []);
  assert.match(theme, /--r-mark:\s*2px/);
});

test('two control heights: no height of a control between 17 and 29 px but the tokens (a bar or a swatch says why)', () => {
  const stray = [];
  for (const [name, css] of sheets) {
    css.split('\n').forEach((line, i) => {
      if (/\/\*\s*h:/.test(line)) return; // said: a bar, a swatch, a glyph box, not a control
      for (const d of [...declarations(line, 'height'), ...declarations(line, 'min-height')]) {
        const m = d.value.match(/^(\d+(?:\.\d+)?)px$/);
        if (m && Number(m[1]) >= 17 && Number(m[1]) <= 29 && Number(m[1]) !== 24) stray.push(`${name}:${i + 1}: ${d.text.slice(0, 100)}`);
      }
    });
  }
  assert.deepEqual(stray, []);
  assert.match(theme, /--h-sm:\s*24px/);
  assert.match(theme, /--h-md:\s*30px/);
  assert.match(theme, /\.seg \{[^}]*height: var\(--h-md\)/, 'a segmented control is 30 outside');
  assert.match(theme, /\.seg > button \{[^}]*height: var\(--h-sm\)/, 'and 24 inside');
});

test('one status mark: an 8 px square with the mark corner, and no dot of another shape', () => {
  assert.match(theme, /--mark:\s*8px/);
  assert.match(theme, /^\.dot \{[^}]*width: var\(--mark\); height: var\(--mark\); border-radius: var\(--r-mark\)/m);
  assert.doesNotMatch(style, /^\.dot \{/m, 'style.css has no second .dot');
  assert.doesNotMatch(style, /#sb-dot\b[^{]*\{/, 'the status line draws its mark with .dot');
  const html = read('../index.html');
  assert.match(html, /<i id="sb-dot" class="dot"/);
});

test('one icon set: every icon is drawn by js/view/icons.js, and no character stands in for an icon', () => {
  const GLYPHS = '›‹▾⋯✕⟲⟳☾☀↗';
  const seen = [];
  for (const [file, src] of jsFiles()) {
    if (file === 'js/view/icons.js') continue;
    src.split('\n').forEach((line, i) => {
      const code = line.replace(/^\s*(\/\/|\*|\/\*).*$/, '');
      const hit =
        new RegExp(`>\\s*[${GLYPHS}×−]\\s*<`).test(code) // an element holding only the glyph
        || new RegExp(`["'\`][${GLYPHS}]["'\`]`).test(code) // a string that is the glyph
        || new RegExp(`[${GLYPHS}]\\s*</(button|span|a)>`).test(code) // a glyph ending a button or a link
        || new RegExp(`<(button|a)[^>]*>\\s*[${GLYPHS}]`).test(code)
        || /Help ↗/.test(code);
      if (hit) seen.push(`${file}:${i + 1}: ${line.trim().slice(0, 120)}`);
    });
  }
  assert.deepEqual(seen, []);
  assert.doesNotMatch(style, /\.history-icon|copy-glyph|url\("\/icons\//, 'no icon drawn by a mask of a file of its own');
  assert.doesNotMatch(theme, /--copy-glyph/);
  assert.match(read('../vendor/LICENSE-lucide'), /ISC License/, 'the Lucide icons carry their licence note');
});

test('the nine cameras have nine different icons, and the view bar pins Iso, Top, Front and Close-up', async () => {
  const { ICONS, cameraIcon } = await import('../js/view/icons.js');
  const names = ['painted', 'top', 'front', 'right', 'back', 'left', 'low', 'close', 'under'].map(cameraIcon);
  assert.equal(new Set(names.map((n) => ICONS[n])).size, 9, 'no two cameras share a picture');
  for (const n of names) assert.match(ICONS[n], /currentColor/), assert.match(ICONS[n], /var\(--accent\)/);
});

test('the panels show and hide through slide.js: one reveal, and no transition or second animation of its own', () => {
  const slide = read('../js/slide.js');
  assert.match(slide, /export function reveal\(/);
  for (const f of ['../js/layout.js', '../js/editor/panel.js']) {
    const src = read(f);
    assert.match(src, /import \{[^}]*\breveal\b[^}]*\} from '\.\.?\/slide\.js'/, `${f} reveals through slide.js`);
    assert.doesNotMatch(src, /\.animate\(|transition|requestAnimationFrame/, `${f} animates nothing itself`);
  }
  assert.doesNotMatch(style, /#(side|msgpanel|app)\b[^{]*\{[^}]*transition/, 'the stylesheet gives the panels no transition of its own');
  // the one reveal uses the level slide's own duration and easing, and the reduced-motion swap
  const body = slide.slice(slide.indexOf('export function reveal('));
  assert.match(body, /duration: SLIDE_MS, easing: EASE/);
  assert.match(body, /reduced\(\)/);
});

test('the display menu takes its limit from the box the view resizes to', () => {
  assert.match(style, /#viewbar \{[^}]*inset: 10px 10px calc\(26px \+ var\(--msgh\)\) 10px/, 'the view bar is the canvas\'s box less its top gap');
  assert.match(style, /#display \{[^}]*max-height: calc\(100% - 40px\)[^}]*overflow-y: auto/, 'the menu is never taller than that box, and scrolls inside itself');
  const js = jsFiles().map(([, s]) => s).join('\n');
  assert.doesNotMatch(js, /#display|\$\('display'\)\.style/, 'no measurement of the menu in script');
});

test('the view bar follows the camera: it is redrawn when the derived activeView changes, and lit on opening', () => {
  const chrome = read('../js/editor/chrome.js');
  // activeView is derived in editor/framing.js, after this module starts: an effect reading it first would never follow it
  assert.match(chrome, /view\.subscribe\(\['activeView'\], drawViews\)/);
  assert.doesNotMatch(chrome, /effect\(\(\) => \{\s*const active = view\.get\(\)\.activeView/);
  assert.match(read('../js/editor/framing.js'), /presetAt\(VIEWS, p\.elev, p\.azim/);
});
