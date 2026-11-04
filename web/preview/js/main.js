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
  tiles: [], selected: -1, overhangDeg: 55, system: 'db',
  loaded: new Map(),   // tile index -> mesh_version shown
  cameras: new Map(),  // tile index -> saved camera
  view: 'painted', buildStart: new Map(),
};
window.dtPreview = { state, viewer }; // for tests and the console

// ---- toolbar ---------------------------------------------------------
VIEWS.forEach(([name, label], i) => {
  const b = document.createElement('button');
  b.className = 'view'; b.dataset.view = name; b.textContent = label;
  b.title = `${label} (${i + 1})`;
  b.onclick = () => pickView(name);
  $('views').append(b);
});
function pickView(name, animate = true) {
  state.view = name;
  viewer.setView(name, animate);
  document.querySelectorAll('button.view').forEach((b) => b.classList.toggle('on', b.dataset.view === name));
}
const toggle = (id, fn) => { $(id).onchange = (e) => fn(e.target.checked); };
toggle('t-overhang', (on) => viewer.setOverhang(on));
toggle('t-wire', (on) => viewer.setWireframe(on));
toggle('t-grid', (on) => viewer.setGrid(on));
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
  if (e.key === 'o') flip('t-overhang');
  if (e.key === 'w') flip('t-wire');
  if (e.key === 'g') flip('t-grid');
  if (e.key === 'p') setProjection(!viewer.isOrtho);
});

// ---- tile list -------------------------------------------------------
function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}
function renderList() {
  const q = $('filter').value.trim().toLowerCase();
  const ul = $('tiles');
  ul.replaceChildren();
  let lastGroup = null;
  for (const t of state.tiles) {
    if (q && !t.name.toLowerCase().includes(q) && !t.path.toLowerCase().includes(q)) continue;
    const m = t.path.match(/(?:^|[:/])tiles\/([^/]+)\//);
    const group = m ? m[1] : null;
    if (group && group !== lastGroup) {
      const g = document.createElement('li'); g.className = 'group'; g.textContent = group; ul.append(g);
      lastGroup = group;
    }
    const li = document.createElement('li');
    li.className = t.index === state.selected ? 'sel' : '';
    li.title = t.path;
    li.innerHTML = '<span></span><span class="name"></span>';
    li.firstChild.className = dotClass(t);
    li.querySelector('.name').textContent = t.name;
    li.onclick = () => select(t.index);
    ul.append(li);
  }
}
$('filter').oninput = renderList;

// ---- selection and the side panel ------------------------------------
function select(i) {
  if (state.selected >= 0 && viewer.payload) state.cameras.set(state.selected, viewer.getCamera());
  state.selected = i;
  history.replaceState(null, '', `#tile=${i}`);
  viewer.clear();
  viewer.setPlan(null);
  state.loaded.delete(i);
  const t = state.tiles[i];
  if (!t) return;
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

const fmt = (x, d = 2) => Number(x).toFixed(d);
function row(dl, k, v, cls = '') {
  const dt = document.createElement('dt'); dt.textContent = k;
  const dd = document.createElement('dd'); dd.textContent = v; if (cls) dd.className = cls;
  dl.append(dt, dd);
}
let timer = 0;
function renderPanel() {
  const t = state.tiles[state.selected];
  $('empty').hidden = !!t;
  const status = $('status'), errs = $('errors'), dl = $('record');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  if (!t) return;
  $('tile-name').textContent = t.name;
  const p = t.plan;
  $('tile-plan').textContent = p
    ? `${p.cols}×${p.rows} squares of ${p.square_mm} mm · ${p.stack.join(', ')}`
    : t.path;
  // building…
  if (t.status === 'building' || t.status === 'idle') {
    if (!state.buildStart.has(t.index)) state.buildStart.set(t.index, performance.now());
    const tick = () => {
      status.textContent = `building… ${((performance.now() - state.buildStart.get(t.index)) / 1000).toFixed(1)} s`;
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
    row(dl, 'state', t.status);
    return;
  }
  const a = r.audit;
  const stale = t.status === 'failed' ? ' (last good build)' : '';
  row(dl, 'triangles', a.triangles.toLocaleString() + stale);
  row(dl, 'watertight', a.watertight ? 'yes' : 'NO', a.watertight ? 'ok' : 'bad');
  row(dl, `overhang (> ${state.overhangDeg}°)`, `${fmt(a.overhang_mm2)} mm²`, a.overhang_mm2 > 0 ? 'bad' : 'ok');
  row(dl, 'band', `${fmt(a.band_mm2)} / ${fmt(a.band_budget_mm2)} mm²`);
  const dropped = Array.isArray(a.dropped) ? a.dropped.length : a.dropped;
  row(dl, 'dropped bodies', dropped, dropped ? 'bad' : '');
  row(dl, 'voids filled', a.voids_filled);
  row(dl, 'holes', a.holes, a.holes ? 'bad' : '');
  row(dl, 'build time', `${fmt(r.build_s)} s`);
  if (r.record.seed != null) row(dl, 'seed', r.record.seed);
  row(dl, 'system', r.record.system);
  for (const f of a.failures) {
    const d = document.createElement('div'); d.className = 'f'; d.textContent = `gate: ${f}`;
    $('failures').append(d);
  }
}

// ---- live updates ----------------------------------------------------
host.connect({
  onState: (s) => { $('conn').textContent = s; },
  onSnapshot: (m) => {
    state.tiles = m.tiles; state.overhangDeg = m.overhang_deg; state.system = m.system;
    $('system').textContent = m.system;
    $('lg-red').textContent = `overhang past ${m.overhang_deg}°`;
    const want = Number((location.hash.match(/tile=(\d+)/) || [])[1] ?? 0);
    if (state.selected < 0 || !state.tiles[state.selected]) select(Math.min(want, m.tiles.length - 1));
    else { renderList(); select(state.selected); }
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
