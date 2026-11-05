// The editor page (docs/design/editor.html): the tile menu, the text, the
// view and the stack, linked by one selection.  Talks to the server only
// through `host` (js/host.js), so a desktop app can reuse this page and the
// viewer with a different host.
//
// The selection is a stack entry (an index into the plan's stack, the base
// last), optionally one piece of it (rock 2 of a scatter), or the whole tile.
// Clicking a thing selects it; clicking it again climbs: piece, entry, tile.

import { browserHost } from './host.js';
import { parsePayload, NO_ENTRY } from './payload.js';
import { TextPane } from './text.js';
import { Viewer, VIEWS } from './viewer.js';

const $ = (id) => document.getElementById(id);
const host = browserHost();
const viewer = new Viewer($('canvas'));

const state = {
  tiles: [], roots: [], root: -1, selected: -1, overhangDeg: 55, steepestDeg: 65, system: 'db',
  lastInRoot: new Map(), // root index -> the tile last chosen there
  loaded: new Map(),   // tile index -> mesh_version shown
  cameras: new Map(),  // tile index -> saved camera
  view: 'painted', buildStart: new Map(),
  sel: null,           // {entry, piece} (piece null: the whole entry) | {tile: true} | null
  hov: null,           // the same, under the pointer (view, text or stack)
  pieces: new Map(),   // entry -> how many pieces its geometry has
  textFor: null,       // [tile index, file, mesh_version] the text pane shows
};
window.dtPreview = { state, viewer }; // for tests and the console

const tile = () => state.tiles[state.selected];
const stackItems = () => (tile()?.plan?.stack || []).map((s) => (typeof s === 'string' ? { label: s } : s));
// only a scatter's things are picked one by one (a dirt's clods or a grass's clumps are the entry)
const piecesOf = (e) => (stackItems()[e]?.what === 'scatter' ? state.pieces.get(e) || 0 : 0);
const same = (a, b) => (!a && !b) || (a && b && !!a.tile === !!b.tile && a.entry === b.entry && (a.piece ?? null) === (b.piece ?? null));

// ---- the view's controls: views and the display menu -------------------
const SHOWN_VIEWS = ['painted', 'top', 'low', 'close'];
VIEWS.forEach(([name, label], i) => {
  const mk = () => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'view'; b.dataset.view = name;
    b.textContent = name === 'top' ? 'Map' : label;
    b.title = `${label} (${i + 1})`;
    b.onclick = () => pickView(name);
    return b;
  };
  if (SHOWN_VIEWS.includes(name)) $('views').append(mk());
  $('allviews').append(mk());
});
function pickView(name, animate = true) {
  state.view = name;
  viewer.setView(name, animate);
  document.querySelectorAll('button.view').forEach((b) => {
    b.classList.toggle('on', b.dataset.view === name);
    b.setAttribute('aria-pressed', b.dataset.view === name);
  });
}
const toggle = (id, fn) => { $(id).onchange = (e) => fn(e.target.checked); };
toggle('t-overhang', (on) => viewer.setOverhang(on));
// paint on or off (grey plastic): remembered in the browser
function setPaint(on) {
  $('t-paint').checked = on;
  viewer.setPaint(on);
  try { localStorage.setItem('dt-preview-paint', on ? '1' : '0'); } catch { /* ignore */ }
}
$('t-paint').onchange = (e) => setPaint(e.target.checked);
try { if (localStorage.getItem('dt-preview-paint') === '0') setPaint(false); } catch { /* ignore */ }
if (/paint=off/.test(location.hash)) { $('t-paint').checked = false; viewer.setPaint(false); } // #paint=off previews grey without saving
toggle('t-wire', (on) => viewer.setWireframe(on));
toggle('t-grid', (on) => viewer.setGrid(on));

// popovers: one open at a time; Esc or a click outside closes it
const popovers = [['tilebtn', 'menu'], ['dispbtn', 'display'], ['gate', 'record-card']];
function openPopover(id) {
  for (const [btn, pop] of popovers) {
    const open = pop === id && $(pop).hidden;
    $(pop).hidden = !open;
    $(btn).setAttribute('aria-expanded', open);
  }
  if (id === 'menu' && !$('menu').hidden) {
    renderList();
    $('tiles').querySelector('li.sel')?.scrollIntoView({ block: 'center' });
    $('filter').focus();
  }
}
const closePopovers = () => {
  let any = false;
  for (const [btn, pop] of popovers) { any ||= !$(pop).hidden; $(pop).hidden = true; $(btn).setAttribute('aria-expanded', false); }
  return any;
};
for (const [btn, pop] of popovers) $(btn).onclick = (e) => { e.stopPropagation(); openPopover(pop); };
$('choose').onclick = (e) => { e.stopPropagation(); openPopover('menu'); };
addEventListener('pointerdown', (e) => {
  if (popovers.some(([b, p]) => $(p).contains(e.target) || $(b).contains(e.target))) return;
  closePopovers();
});

// light or dark: the system's setting until the button is used; then the choice is remembered
const dark = matchMedia('(prefers-color-scheme: dark)');
function applyTheme() {
  let stored = null;
  try { stored = localStorage.getItem('dt-theme'); } catch { /* ignore */ }
  const hash = (location.hash.match(/theme=(light|dark)/) || [])[1]; // #theme=light previews a theme without saving it
  if (hash) stored = hash;
  if (stored === 'light' || stored === 'dark') document.documentElement.dataset.theme = stored;
  else delete document.documentElement.dataset.theme;
  const isDark = stored ? stored === 'dark' : dark.matches;
  $('theme').textContent = isDark ? '☀' : '☾';
  viewer.setGridDim(Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--grid')) || 1);
}
function toggleTheme() {
  const isDark = document.documentElement.dataset.theme
    ? document.documentElement.dataset.theme === 'dark' : dark.matches;
  try { localStorage.setItem('dt-theme', isDark ? 'light' : 'dark'); } catch { /* ignore */ }
  applyTheme();
}
$('theme').onclick = toggleTheme;
dark.addEventListener('change', applyTheme);
applyTheme();

// perspective or orthographic: remembered across reloads (a per-viewer convenience)
function setProjection(ortho, animated = true) {
  viewer.setOrtho(ortho, animated);
  $('b-proj').firstChild.textContent = ortho ? 'Orthographic ' : 'Perspective ';
  try { localStorage.setItem('dt-preview-ortho', ortho ? '1' : '0'); } catch { /* private mode */ }
}
$('b-proj').onclick = () => setProjection(!viewer.isOrtho);
try { if (localStorage.getItem('dt-preview-ortho') === '1') setProjection(true, false); } catch { /* ignore */ }
addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    if (!closePopovers()) setSel(null);
    return;
  }
  if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable) return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const n = Number(e.key);
  if (n >= 1 && n <= VIEWS.length) pickView(VIEWS[n - 1][0]);
  const flip = (id) => { $(id).checked = !$(id).checked; $(id).onchange({ target: $(id) }); };
  if (e.key === 'c') setPaint(!$('t-paint').checked);
  if (e.key === 'o') flip('t-overhang');
  if (e.key === 'w') flip('t-wire');
  if (e.key === 'g') flip('t-grid');
  if (e.key === 't') toggleTheme();
  if (e.key === 'p') setProjection(!viewer.isOrtho);
  if (e.key === 'f' && state.sel) frameSel();
});

// ---- the tile menu -----------------------------------------------------
function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}
// the file stem (`2x1-cliff`, `1x1-water+grass-tree`): the menu's name for a tile, whatever the document calls it
const stem = (t) => t.path.replace(/^lib:/, '').split(/[\\/]/).pop().replace(/\.(dtile|stl)$/i, '') + (/\.stl$/i.test(t.path) ? '.stl' : '');
const inRoot = (r) => state.tiles.filter((t) => t.root === r && !t.gone);
const rootOf = (r) => state.roots[r] || { kind: 'files', label: 'Files', path: null };
let testsOpen = false; // the library's "Test tiles" starts collapsed

// the switcher: one button per root (the personal home, the library, each folder named)
function renderRoots() {
  const nav = $('roots');
  nav.replaceChildren();
  nav.hidden = state.roots.length < 2;
  state.roots.forEach((r, i) => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'root' + (i === state.root ? ' on' : '');
    b.setAttribute('aria-pressed', i === state.root);
    b.title = r.path || r.label;
    b.innerHTML = '<span></span> <span class="n"></span>';
    b.firstChild.textContent = r.label;
    b.lastChild.textContent = inRoot(i).length;
    b.onclick = () => { state.root = i; renderList(); };
    nav.append(b);
  });
}

function renderList() {
  renderRoots();
  const q = $('filter').value.trim().toLowerCase();
  const ul = $('tiles');
  ul.replaceChildren();
  const matches = (t) => !q || stem(t).toLowerCase().includes(q) || t.path.toLowerCase().includes(q);
  const row = (t) => {
    const li = document.createElement('li');
    li.className = t.index === state.selected ? 'sel' : '';
    li.title = t.name && t.name !== stem(t) ? `${t.path}\ntile “${t.name}”` : t.path;
    li.innerHTML = '<span></span><span class="name"></span>';
    li.firstChild.className = dotClass(t);
    li.querySelector('.name').textContent = stem(t);
    li.onclick = () => { closePopovers(); select(t.index); };
    return li;
  };
  const header = (text, onclick, open) => {
    const g = document.createElement('li'); g.className = 'group';
    g.textContent = `${onclick ? (open ? '▾ ' : '▸ ') : ''}${text}`;
    if (onclick) { g.classList.add('fold'); g.onclick = onclick; }
    ul.append(g);
  };
  const root = rootOf(state.root);
  const all = inRoot(state.root);
  if (!all.length) {
    const li = document.createElement('li'); li.className = 'note';
    li.textContent = root.kind === 'dir'
      ? `No tiles here yet. Save a .dtile in ${root.path} and it shows up.`
      : 'No tiles.';
    ul.append(li);
    return;
  }
  const shown = all.filter(matches);
  // test tiles fold away in the library only
  const isTest = (t) => root.kind === 'library' && t.group === 'test';
  const main = shown.filter((t) => !isTest(t))
    .sort((x, y) => (x.group || '').localeCompare(y.group || '') || x.path.localeCompare(y.path));
  const tests = shown.filter(isTest);
  let last = null;
  for (const t of main) {
    const g = t.group;
    if (g && g !== last) { header(g); last = g; }
    ul.append(row(t));
  }
  if (tests.length) {
    const open = testsOpen || !!q || tests.some((t) => t.index === state.selected);
    header(`Test tiles (${tests.length})`, () => { testsOpen = !open; renderList(); }, open);
    if (open) for (const t of tests) ul.append(row(t));
  }
}
$('filter').oninput = renderList;
$('filter').onkeydown = (e) => {
  if (e.key !== 'Enter') return;
  const first = $('tiles').querySelector('li:not(.group):not(.note)');
  first?.click();
};

// ---- choosing a tile ---------------------------------------------------
function select(i) {
  if (state.selected >= 0 && viewer.payload) state.cameras.set(state.selected, viewer.getCamera());
  if (i !== state.selected) { state.sel = null; state.hov = null; $('tilebtn').classList.remove('sel'); }
  state.selected = i;
  viewer.clear();
  viewer.setPlan(null);
  state.loaded.delete(i);
  const t = state.tiles[i];
  if (!t || t.gone) {
    state.selected = -1;
    history.replaceState(null, '', `#root=${state.root}`);
    texts.clear(); state.textFor = null;
    renderAll();
    return;
  }
  state.root = t.root;
  state.lastInRoot.set(t.root, i);
  history.replaceState(null, '', `#tile=${i}`);
  if (t.plan) viewer.setPlan(t.plan);
  if (t.status === 'idle') host.build(i);
  if (t.mesh_version != null) loadMesh(i, t.mesh_version);
  renderAll();
}

let loadToken = 0;
async function loadMesh(i, version) {
  const token = ++loadToken;
  try {
    const buf = await host.fetchMesh(i);
    if (token !== loadToken || i !== state.selected) return;
    const first = !viewer.payload;
    const p = parsePayload(buf);
    viewer.setMesh(p);
    state.pieces = new Map();
    for (let t = 0; t < p.triangles; t++) {
      const e = p.entry[t];
      if (e !== NO_ENTRY) state.pieces.set(e, Math.max(state.pieces.get(e) || 0, p.piece[t] + 1));
    }
    state.loaded.set(i, version);
    if (first) {
      const saved = state.cameras.get(i);
      if (saved) viewer.setCamera(saved); else pickView(state.view, false);
    }
    // a selection the new build no longer has goes
    if (state.sel && !state.sel.tile && state.sel.entry >= stackItems().length) state.sel = null;
    renderAll();
  } catch (e) {
    console.error(e);
  }
}

// ---- the text ------------------------------------------------------------
const texts = new TextPane($('code'), {
  onLine: (n) => setSel(entryAtLine(n), { from: 'text' }),
  onHoverLine: (n) => setHov(n == null ? null : entryAtLine(n)),
});

/** The innermost entry written on line `n` of the file shown, else the tile (its own file) or nothing. */
function entryAtLine(n) {
  let best = null, width = Infinity;
  stackItems().forEach((it, i) => {
    if (!it.lines || it.file !== texts.file) return;
    const [a, b] = it.lines;
    if (n >= a && n <= b && b - a < width) { best = i; width = b - a; }
  });
  if (best != null) return { entry: best, piece: null };
  return texts.file === tile()?.path ? { tile: true } : null;
}

let textToken = 0;
async function loadText(file) {
  const t = tile();
  if (!t || /\.stl$/i.test(t.path)) { texts.clear(); state.textFor = null; renderTextBar(); return; }
  const key = [t.index, file || t.path, t.mesh_version, t.status, t.errors.length];
  if (state.textFor && state.textFor.join('|') === key.join('|')) return;
  state.textFor = key;
  const token = ++textToken;
  try {
    const r = await fetch(`/api/tiles/${t.index}/text${file && file !== t.path ? `?file=${encodeURIComponent(file)}` : ''}`, { cache: 'no-store' });
    if (token !== textToken) return;
    if (!r.ok) { state.textFor = null; return; }
    const v = await r.json();
    texts.writable = v.writable;
    texts.show(v.file, v.text);
    renderTextBar();
    markText();
  } catch (e) { console.error(e); state.textFor = null; }
}

function renderTextBar() {
  const t = tile();
  const f = texts.file;
  $('textpath').textContent = f ? `‎${f}` : '';
  $('textpath').title = f || '';
  const st = $('textstate');
  st.replaceChildren();
  if (f && f !== t?.path) {
    // a file the tile uses: the way back to the tile's own
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'back'; b.textContent = 'Back to the tile';
    b.onclick = () => loadText(null).then(() => { markText(); revealSel(); });
    st.append(b);
  } else if (f) st.textContent = f.startsWith('lib:') ? 'library, read only' : 'read only for now';
}

function linesOf(w) {
  if (!w) return null;
  if (w.tile) return texts.file === tile()?.path ? [1, (texts.text || '').split('\n').length] : null;
  const it = stackItems()[w.entry];
  return it?.lines && it.file === texts.file ? it.lines : null;
}

function markText() {
  const t = tile();
  const errs = (t?.errors || []).filter((e) => e.file === texts.file && e.line).map((e) => e.line);
  texts.mark(state.sel?.tile ? null : linesOf(state.sel), linesOf(state.hov), errs);
}

// ---- the selection ----------------------------------------------------------
function describe(w) {
  if (!w) return '';
  if (w.tile) return `tile ${tile()?.name || ''}`.trim();
  const it = stackItems()[w.entry];
  if (!it) return '';
  const n = piecesOf(w.entry);
  if (w.piece != null && n > 1) return `${it.label} · ${w.piece + 1} of ${n}`;
  return it.label;
}

function setSel(w, { from } = {}) {
  if (same(w, state.sel)) return;
  state.sel = w;
  $('tilebtn').classList.toggle('sel', !!w?.tile);
  viewer.setHighlight(w && !w.tile ? w : null, hovForView());
  renderStack();
  // the text follows: the selection's own file, scrolled to it
  if (w && !w.tile) {
    const it = stackItems()[w.entry];
    if (it?.file && it.file !== texts.file && from !== 'text') loadText(it.file).then(() => revealSel());
  } else if (w?.tile && texts.file !== tile()?.path) loadText(null);
  markText();
  if (from !== 'text') revealSel();
}
function revealSel() { const l = linesOf(state.sel); if (l) texts.reveal(l[0]); }

function setHov(w) {
  if (same(w, state.hov)) return;
  state.hov = w;
  viewer.setHighlight(state.sel && !state.sel.tile ? state.sel : null, hovForView());
  document.querySelectorAll('.srow').forEach((r) => r.classList.toggle('hov', !!w && !w.tile && Number(r.dataset.entry) === w.entry));
  markText();
}
const hovForView = () => (state.hov && !state.hov.tile && !same(state.hov, state.sel) ? state.hov : null);

/** A click in the view on `hit`: select it, or climb when it is already selected. */
function clickHit(hit) {
  if (!hit) return setSel(null);
  const s = state.sel;
  const many = piecesOf(hit.entry) > 1;
  if (!s || s.tile || s.entry !== hit.entry) return setSel({ entry: hit.entry, piece: many ? hit.piece : null }, { from: 'view' });
  if (s.piece != null) return setSel({ entry: hit.entry, piece: null }, { from: 'view' }); // the piece -> its entry
  return setSel({ tile: true }, { from: 'view' }); // the entry -> the tile
}

/** Fly to the selection (f). */
function frameSel() {
  const s = state.sel;
  if (!s || s.tile) return pickView(state.view);
  const b = viewer.centreOf(s.entry, s.piece);
  if (!b) return;
  const pose = viewer.pose();
  const size = Math.max(8, Math.hypot(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]));
  pose.target.set((b.lo[0] + b.hi[0]) / 2, (b.lo[1] + b.hi[1]) / 2, (b.lo[2] + b.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 2.4 });
}

// hover and click in the view: a click is a press and release that hardly moved
const canvas = $('canvas');
let down = null, hoverRaf = 0, lastMove = null;
canvas.addEventListener('pointerdown', (e) => { down = { x: e.clientX, y: e.clientY, t: performance.now(), b: e.button }; $('hint').style.display = 'none'; });
canvas.addEventListener('pointerup', (e) => {
  const d = down; down = null;
  if (!d || d.b !== 0 || Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4 || performance.now() - d.t > 600) return;
  const r = canvas.getBoundingClientRect();
  clickHit(viewer.pick(e.clientX - r.left, e.clientY - r.top));
});
canvas.addEventListener('pointermove', (e) => {
  if (e.buttons || e.pointerType === 'touch') return;
  lastMove = e;
  if (hoverRaf) return;
  hoverRaf = requestAnimationFrame(() => {
    hoverRaf = 0;
    const r = canvas.getBoundingClientRect();
    const x = lastMove.clientX - r.left, y = lastMove.clientY - r.top;
    const hit = viewer.pick(x, y);
    const many = hit && piecesOf(hit.entry) > 1;
    // hover shows what a click would select
    let w = hit && { entry: hit.entry, piece: many ? hit.piece : null };
    if (w && state.sel && !state.sel.tile && state.sel.entry === w.entry) w = state.sel;
    setHov(w);
    const hint = $('hint');
    if (w) {
      hint.textContent = describe(w);
      hint.style.display = 'block';
      const left = Math.min(x + 14, r.width - hint.offsetWidth - 8);
      hint.style.left = `${Math.max(8, left)}px`;
      hint.style.top = `${Math.max(8, y - 30)}px`;
    } else hint.style.display = 'none';
  });
});
canvas.addEventListener('pointerleave', () => { setHov(null); $('hint').style.display = 'none'; });
canvas.addEventListener('dblclick', () => frameSel());

// ---- the stack -----------------------------------------------------------
// each element's family, for its colour (the docs site uses the same)
const FAMILY = {
  ground: 'surface', dirt: 'surface', floor: 'surface', water: 'water',
  rock: 'thing', stone: 'thing', boulder: 'thing', pebble: 'thing', scatter: 'thing', cliff: 'thing', wall: 'thing',
  grass: 'plant', reeds: 'plant', flowers: 'plant', tree: 'plant',
  dungeonblocks: 'base', openlock: 'base', connector: 'base', stl: 'base',
};
function renderStack() {
  const ol = $('stack');
  ol.replaceChildren();
  const items = stackItems();
  items.forEach((it, i) => {
    const li = document.createElement('li');
    const base = i === items.length - 1 && it.element === 'connector';
    li.className = `srow fam-${FAMILY[it.element] || 'doc'}${base ? ' base' : ''}`;
    li.dataset.entry = i;
    const sel = state.sel && !state.sel.tile && state.sel.entry === i;
    li.classList.toggle('sel', !!sel);
    if (viewer.payload && !state.pieces.has(i)) li.classList.add('hidden-geo');
    const n = piecesOf(i);
    const sub = base ? 'added to every tile'
      : `${it.what || it.element || ''}${n > 1 ? ` · ${n} pieces` : ''}${it.lines ? ` · ${it.file && it.file !== tile()?.path ? `${it.file.replace(/^lib:/, '').replace(/\.dtile$/, '')} ` : ''}line ${it.lines[0]}` : ''}`;
    li.innerHTML = '<i class="sw"></i><div><span class="nm"></span><span class="pl"></span></div>';
    li.querySelector('.nm').textContent = it.label;
    li.querySelector('.pl').textContent = sel && state.sel.piece != null ? `${describe(state.sel)}` : sub;
    if (it.href) {
      const a = document.createElement('a');
      a.className = 'doc'; a.href = it.href; a.target = '_blank'; a.rel = 'noopener';
      a.textContent = '?'; a.title = `${it.element}: its page in the docs`;
      a.onclick = (e) => e.stopPropagation();
      li.append(a);
    }
    li.onclick = () => setSel(same(state.sel, { entry: i, piece: null }) ? null : { entry: i, piece: null }, { from: 'stack' });
    li.ondblclick = () => { setSel({ entry: i, piece: null }, { from: 'stack' }); frameSel(); };
    li.onpointerenter = () => setHov({ entry: i, piece: null });
    li.onpointerleave = () => setHov(null);
    ol.append(li);
  });
}

// ---- the title, the gate and the record ---------------------------------------
const secs = (s) => (s >= 60 ? `${Math.floor(s / 60)}m${String(Math.round(s % 60)).padStart(2, '0')}s` : `${s.toFixed(1)} s`);
/** The bar: percentage, the stage (with an element's own count), elapsed against the estimate. */
function showProgress(p) {
  const pct = Math.floor(p.fraction * 100);
  $('bar-fill').style.width = `${pct}%`;
  $('progress').style.width = `${pct}%`;
  const stage = !p.count ? p.label
    : p.count[2] === 0 ? `${p.label}: ${p.count[0]}`
    : `${p.label}: ${p.count[0]} ${p.count[1]}/${p.count[2]}`;
  $('status-text').textContent = `${pct}% · ${stage} · ${secs(p.elapsed_s)} of ~${secs(p.estimate_s)}`;
}

const fmt = (x, d = 2) => Number(x).toFixed(d);
function row(dl, k, v, cls = '') {
  const dt = document.createElement('dt'); dt.textContent = k;
  const dd = document.createElement('dd'); dd.textContent = v; if (cls) dd.className = cls;
  dl.append(dt, dd);
}

let timer = 0;
function renderAll() {
  renderList();
  renderTitle();
  renderStack();
  viewer.setHighlight(state.sel && !state.sel.tile ? state.sel : null, hovForView());
  const t = tile();
  if (t) loadText(state.textFor && state.textFor[0] === t.index && state.textFor[1] !== t.path ? state.textFor[1] : null);
  markText();
}

function renderTitle() {
  const t = tile();
  $('empty').hidden = !!t;
  const status = $('status'), errs = $('errors'), dl = $('record'), gate = $('gate');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  $('verdict').hidden = true;
  if (!t) {
    status.hidden = true; errs.hidden = true; gate.hidden = true;
    $('tilename').textContent = 'Choose a tile'; $('tilewhere').textContent = ''; $('tileplan').textContent = '';
    $('progress').style.width = '0';
    document.title = 'dharmatiles';
    return;
  }
  $('tilename').textContent = stem(t);
  $('tilewhere').textContent = rootOf(t.root).label || '';
  $('tilebtn').title = t.name && t.name !== stem(t) ? `${t.path}\nthe document calls it “${t.name}”` : t.path;
  document.title = `${stem(t)} · dharmatiles`;
  const p = t.plan;
  $('tileplan').textContent = p ? `${p.cols}×${p.rows} · ${p.square_mm} mm squares · ${state.system === 'ol' ? 'OpenLOCK' : 'DungeonBlocks'}` : '';
  // building…
  const building = t.status === 'building' || t.status === 'idle';
  if (building) {
    if (!state.buildStart.has(t.index)) state.buildStart.set(t.index, performance.now());
    // before the first progress event there is only the clock
    const tick = () => {
      if (t.progress) return showProgress(t.progress);
      $('bar-fill').style.width = '0%';
      $('status-text').textContent = t.status === 'idle'
        ? 'Waiting to build…'
        : `Building… ${((performance.now() - state.buildStart.get(t.index)) / 1000).toFixed(1)} s`;
    };
    tick(); timer = setInterval(tick, 100);
    status.hidden = false;
  } else {
    status.hidden = true;
    $('progress').style.width = '0';
    state.buildStart.delete(t.index);
  }
  // errors: file:line:col, the old mesh stays behind them; a click shows the line
  errs.hidden = !t.errors.length;
  errs.replaceChildren();
  for (const e of t.errors) {
    const d = document.createElement('div'); d.className = 'e';
    const w = document.createElement('div'); w.className = 'where';
    w.textContent = e.file ? `${e.file}:${e.line}:${e.column}  ${e.code}` : e.code;
    const m = document.createElement('div'); m.textContent = e.message;
    d.append(w, m);
    if (e.help) { const h = document.createElement('div'); h.className = 'help'; h.textContent = e.help; d.append(h); }
    if (e.file && e.line) d.onclick = () => loadText(e.file === t.path ? null : e.file).then(() => texts.reveal(e.line));
    errs.append(d);
  }
  const r = t.record;
  gate.hidden = false;
  gate.className = building ? 'busy' : '';
  if (!r) {
    $('gatetext').textContent = building ? 'building' : t.status === 'failed' ? 'no build' : '–';
    if (t.status === 'failed') gate.className = 'fail';
    row(dl, 'State', { idle: 'Not built yet', building: 'Building', failed: 'Failed', ready: 'Ready' }[t.status] || t.status);
    return;
  }
  const a = r.audit;
  const stale = t.status === 'failed' ? ' (last good build)' : '';
  const fails = a.failures.length > 0;
  if (!building) gate.className = fails ? 'fail' : (a.band_mm2 > 0 || a.overhang_mm2 > 0) ? 'warn' : '';
  $('gatetext').textContent = building ? 'building' : fails ? 'fails the gate' : 'passes';
  const v = $('verdict');
  v.hidden = false;
  v.className = fails ? 'fail' : 'pass';
  v.textContent = fails ? `Fails the printability gate${stale}` : `Passes the printability gate${stale}`;
  row(dl, 'Triangles', a.triangles.toLocaleString());
  row(dl, 'Watertight', a.watertight ? 'Yes' : 'No', a.watertight ? 'ok' : 'bad');
  const lo = state.overhangDeg, hi = state.steepestDeg;
  const used = (area, budget) => budget == null ? `${fmt(area)} mm²` : `${fmt(area)} / ${fmt(budget)} mm²`;
  row(dl, `Band ${lo}–${hi}° (orange)`, used(a.band_mm2, a.band_budget_mm2),
      a.band_mm2 > (a.band_budget_mm2 ?? 0) ? 'bad' : a.band_mm2 > 0 ? 'warn' : 'ok');
  row(dl, `Past ${hi}° (red)`, used(a.overhang_mm2, a.overhang_budget_mm2),
      a.overhang_mm2 > (a.overhang_budget_mm2 ?? 0) ? 'bad' : a.overhang_mm2 > 0 ? 'warn' : 'ok');
  const dropped = Array.isArray(a.dropped) ? a.dropped.length : a.dropped;
  row(dl, 'Dropped bodies', dropped, dropped ? 'bad' : '');
  row(dl, 'Voids filled', a.voids_filled);
  row(dl, 'Holes', a.holes, a.holes ? 'bad' : '');
  row(dl, 'Build time', `${fmt(r.build_s)} s`);
  if (r.record.seed != null) row(dl, 'Seed', r.record.seed);
  for (const f of a.failures) {
    const d = document.createElement('div'); d.className = 'f'; d.textContent = f;
    $('failures').append(d);
  }
}

// ---- live updates ----------------------------------------------------
host.connect({
  onState: (s) => { $('conn').textContent = s === 'live' ? '' : s; },
  onSnapshot: (m) => {
    state.tiles = m.tiles; state.roots = m.roots || []; state.overhangDeg = m.overhang_deg; state.steepestDeg = m.steepest_deg ?? m.overhang_deg + 10; state.system = m.system;
    $('lg-orange').textContent = `${state.overhangDeg}–${state.steepestDeg}°: allowed within budget`;
    $('lg-red').textContent = `Over ${state.steepestDeg}°: small patches allowed, more fails`;
    const cur = state.tiles[state.selected];
    if (cur && !cur.gone) {
      // a rescan or a reconnect: keep the view, catch up on a newer mesh
      renderAll();
      if (cur.plan && JSON.stringify(cur.plan) !== JSON.stringify(viewer.plan)) viewer.setPlan(cur.plan);
      if (cur.mesh_version != null && state.loaded.get(cur.index) !== cur.mesh_version) loadMesh(cur.index, cur.mesh_version);
      return;
    }
    if (state.root >= 0) {
      // the tile shown was deleted, or none was: the first one left in this root
      const first = inRoot(state.root)[0];
      select(first ? first.index : -1);
      return;
    }
    // the first snapshot: the tile in the address, else the first root with tiles
    const want = (location.hash.match(/tile=(\d+)/) || [])[1];
    const wantRoot = (location.hash.match(/root=(\d+)/) || [])[1];
    if (want != null && state.tiles[want] && !state.tiles[want].gone) return select(Number(want));
    const roots = state.roots.length ? state.roots.map((_, i) => i) : [0];
    state.root = wantRoot != null && state.roots[wantRoot] ? Number(wantRoot)
      : (roots.find((r) => inRoot(r).length) ?? roots[0]);
    const first = inRoot(state.root)[0];
    select(first ? first.index : -1);
  },
  onProgress: (i, p) => {
    const t = state.tiles[i];
    if (!t) return;
    t.progress = p;
    if (i === state.selected && t.status === 'building') showProgress(p);
  },
  onTile: (t) => {
    state.tiles[t.index] = t;
    if (t.index !== state.selected) { if (!$('menu').hidden) renderList(); return; }
    if (t.plan && JSON.stringify(t.plan) !== JSON.stringify(viewer.plan)) viewer.setPlan(t.plan);
    renderAll();
    // the new mesh swaps in place: the camera is kept
    if (t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) loadMesh(t.index, t.mesh_version);
  },
});
