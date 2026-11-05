// The preview page: tile list, toolbar, build record, live updates.
// Talks to the server only through `host` (js/host.js), so the Tauri app can
// reuse this page and the viewer with a different host.

import { browserHost } from './host.js';
import { parsePayload } from './payload.js';
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
};
window.dtPreview = { state, viewer }; // for tests and the console

// ---- toolbar ---------------------------------------------------------
VIEWS.forEach(([name, label], i) => {
  const b = document.createElement('button');
  b.type = 'button'; b.className = 'view'; b.dataset.view = name; b.textContent = label;
  b.title = `${label} (${i + 1})`;
  b.onclick = () => pickView(name);
  $('views').append(b);
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
  if (e.target.tagName === 'INPUT' && e.target.type === 'search') return;
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
});

// ---- tile list -------------------------------------------------------
function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}
// the file stem (`2x1-cliff`, `1x1-water+grass-tree`): the list's name for a tile, whatever the document calls it
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
    b.onclick = () => switchRoot(i);
    nav.append(b);
  });
}

function switchRoot(r) {
  if (r === state.root) return;
  state.root = r;
  const last = state.lastInRoot.get(r);
  const keep = last != null && state.tiles[last] && !state.tiles[last].gone;
  const first = inRoot(r)[0];
  if (keep) select(last);
  else if (first) select(first.index);
  else { select(-1); }
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
    li.onclick = () => select(t.index);
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

// ---- selection and the side panel ------------------------------------
function select(i) {
  if (state.selected >= 0 && viewer.payload) state.cameras.set(state.selected, viewer.getCamera());
  state.selected = i;
  viewer.clear();
  viewer.setPlan(null);
  state.loaded.delete(i);
  const t = state.tiles[i];
  if (!t || t.gone) {
    state.selected = -1;
    history.replaceState(null, '', `#root=${state.root}`);
    renderList(); renderPanel();
    return;
  }
  state.root = t.root;
  state.lastInRoot.set(t.root, i);
  history.replaceState(null, '', `#tile=${i}`);
  if (t.plan) viewer.setPlan(t.plan);
  if (t.status === 'idle') host.build(i);
  if (t.mesh_version != null) loadMesh(i, t.mesh_version);
  renderList();
  renderPanel();
}

let loadToken = 0;
async function loadMesh(i, version) {
  const token = ++loadToken;
  try {
    const buf = await host.fetchMesh(i);
    if (token !== loadToken || i !== state.selected) return;
    const first = !viewer.payload;
    viewer.setMesh(parsePayload(buf));
    state.loaded.set(i, version);
    if (first) {
      const saved = state.cameras.get(i);
      if (saved) viewer.setCamera(saved); else pickView(state.view, false);
    }
    renderPanel();
  } catch (e) {
    console.error(e);
  }
}

const secs = (s) => (s >= 60 ? `${Math.floor(s / 60)}m${String(Math.round(s % 60)).padStart(2, '0')}s` : `${s.toFixed(1)} s`);
/** The bar: percentage, the stage (with a look's own count), elapsed against the estimate. */
function showProgress(p) {
  const pct = Math.floor(p.fraction * 100);
  $('bar-fill').style.width = `${pct}%`;
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
// each element's family, for its colour (the docs site uses the same)
const FAMILY = {
  ground: 'surface', dirt: 'surface', floor: 'surface', water: 'water',
  rock: 'thing', stone: 'thing', boulder: 'thing', pebble: 'thing', scatter: 'thing', cliff: 'thing', wall: 'thing',
  grass: 'plant', reeds: 'plant', flowers: 'plant', tree: 'plant',
  dungeonblocks: 'base', openlock: 'base', connector: 'base', stl: 'base',
};
// the size line, and the stack in build order, each entry named by what it
// places and linked to its page on the documentation site
function renderPlan(el, p, path) {
  const ol = $('stack');
  ol.replaceChildren();
  $('stack-sec').hidden = !p;
  if (!p) { el.textContent = path; return; }
  el.textContent = `${p.cols}×${p.rows} · ${p.square_mm} mm squares · ${state.system === 'ol' ? 'OpenLOCK' : 'DungeonBlocks'}`;
  for (const s of p.stack) {
    const item = typeof s === 'string' ? { label: s } : s;
    const li = document.createElement('li');
    li.className = `fam-${FAMILY[item.look] || 'doc'}`;
    if (item.href) {
      const a = document.createElement('a');
      a.href = item.href; a.target = '_blank'; a.rel = 'noopener';
      a.textContent = item.label;
      a.title = `${item.look}: open its page in the docs`;
      li.append(a);
    } else {
      li.textContent = item.label;
    }
    ol.append(li);
  }
}

let timer = 0;
function renderPanel() {
  const t = state.tiles[state.selected];
  $('empty').hidden = !!t;
  const status = $('status'), errs = $('errors'), dl = $('record');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  $('verdict').hidden = true;
  if (!t) {
    status.hidden = true; errs.hidden = true;
    $('tile-name').textContent = '–'; $('tile-plan').replaceChildren();
    $('stack').replaceChildren(); $('stack-sec').hidden = true;
    return;
  }
  $('tile-name').textContent = stem(t);
  $('tile-name').title = t.name && t.name !== stem(t) ? `the document calls it “${t.name}”` : '';
  const p = t.plan;
  renderPlan($('tile-plan'), p, t.path);
  // building…
  if (t.status === 'building' || t.status === 'idle') {
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
    state.buildStart.delete(t.index);
  }
  // errors: file:line:col, the old mesh stays behind them
  errs.hidden = !t.errors.length;
  errs.replaceChildren();
  for (const e of t.errors) {
    const d = document.createElement('div'); d.className = 'e';
    const w = document.createElement('div'); w.className = 'where';
    w.textContent = e.file ? `${e.file}:${e.line}:${e.column}  ${e.code}` : e.code;
    const m = document.createElement('div'); m.textContent = e.message;
    d.append(w, m);
    if (e.help) { const h = document.createElement('div'); h.className = 'help'; h.textContent = e.help; d.append(h); }
    errs.append(d);
  }
  const r = t.record;
  if (!r) {
    row(dl, 'State', { idle: 'Not built yet', building: 'Building', failed: 'Failed', ready: 'Ready' }[t.status] || t.status);
    return;
  }
  const a = r.audit;
  const stale = t.status === 'failed' ? ' (last good build)' : '';
  const v = $('verdict');
  v.hidden = false;
  v.className = a.failures.length ? 'fail' : 'pass';
  v.textContent = a.failures.length
    ? `Fails the printability gate${stale}`
    : `Passes the printability gate${stale}`;
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
  onState: (s) => { $('conn').textContent = s; },
  onSnapshot: (m) => {
    state.tiles = m.tiles; state.roots = m.roots || []; state.overhangDeg = m.overhang_deg; state.steepestDeg = m.steepest_deg ?? m.overhang_deg + 10; state.system = m.system;
    $('lg-orange').textContent = `${state.overhangDeg}–${state.steepestDeg}°: allowed within budget`;
    $('lg-red').textContent = `Over ${state.steepestDeg}°: small patches allowed, more fails`;
    const cur = state.tiles[state.selected];
    if (cur && !cur.gone) {
      // a rescan or a reconnect: keep the view, catch up on a newer mesh
      renderList(); renderPanel();
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
    const tile = (location.hash.match(/tile=(\d+)/) || [])[1];
    const wantRoot = (location.hash.match(/root=(\d+)/) || [])[1];
    if (tile != null && state.tiles[tile] && !state.tiles[tile].gone) return select(Number(tile));
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
    renderList();
    if (t.index !== state.selected) return;
    if (t.plan && JSON.stringify(t.plan) !== JSON.stringify(viewer.plan)) viewer.setPlan(t.plan);
    renderPanel();
    // the new mesh swaps in place: the camera is kept
    if (t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) loadMesh(t.index, t.mesh_version);
  },
});
