// The editor page (docs/design/editor.html): the tile menu, the text, the
// view and the property sheet, linked by one selection. Talks to the server only
// through `host` (js/host.js), so a desktop app can reuse this page and the
// viewer with a different host.
//
// The selection is a stack entry (an index into the plan's stack, the base
// last), optionally one piece of it (rock 2 of a scatter), a region or a
// line of the tile (by name), or the whole tile.  Clicking a thing selects
// it; clicking it again climbs: piece, entry, the region it is in, tile.

import { browserHost } from './host.js';
import { parsePayload, NO_ENTRY } from './payload.js';
import { TextPane, byteToIndex } from './text.js';
import { Lens, valueOf, kdlString } from './lens.js';
import { suggest } from './complete.js';
import { Viewer, VIEWS } from './viewer.js';
import { rememberView, shortcutViews } from './view-presets.js';
import { describeChange } from './history.js';
import { Overlay } from './overlay.js';
import { Tools } from './place.js';
import { Layout } from './layout.js';

const $ = (id) => document.getElementById(id);
const host = browserHost();
const viewer = new Viewer($('canvas'));

const state = {
  tiles: [], roots: [], root: -1, selected: -1, overhangDeg: 55, steepestDeg: 65, system: 'db',
  lastInRoot: new Map(), // root index -> the tile last chosen there
  loaded: new Map(),   // tile index -> mesh_version shown
  cameras: new Map(),  // tile index -> saved camera and active preset
  view: 'painted', activeView: null, buildStart: new Map(),
  sel: null,           // {entry, piece} (piece null: the whole entry) | {region} | {line} | {tile: true} | null
  hov: null,           // the same, under the pointer (view or text)
  pieces: new Map(),   // entry -> how many pieces its geometry has
  textFor: null,       // [tile index, file, mesh_version] the text pane shows
};
let tools = null; // the bar's tools (below)
let toastTimer = 0;
window.dtPreview = { state, viewer, get tools() { return tools; } }; // for tests and the console

const tile = () => state.tiles[state.selected];
// the stack of the text now: the last clean check of it while editing, else the last build's
const stackItems = () => ((doc.stack && doc.tile === state.selected ? doc.stack : tile()?.plan?.stack) || []).map((s) => (typeof s === 'string' ? { label: s } : s));
// only a scatter's things are picked one by one (a dirt's clods or a grass's clumps are the entry)
const piecesOf = (e) => (stackItems()[e]?.what === 'scatter' ? state.pieces.get(e) || 0 : 0);
const keyOf = (w) => (!w ? '' : w.tile ? 'tile' : w.region != null ? `r:${w.region}` : w.line != null ? `l:${w.line}` : `e:${w.entry}:${w.piece ?? ''}`);
const same = (a, b) => keyOf(a) === keyOf(b);
const isEntry = (w) => !!w && w.entry != null;

function toast(message, action = null) {
  const box = $('toast');
  clearTimeout(toastTimer);
  box.querySelector('span').textContent = message;
  const button = box.querySelector('button');
  button.hidden = !action;
  button.onclick = action ? () => { box.hidden = true; action(); } : null;
  box.hidden = false;
  toastTimer = setTimeout(() => { box.hidden = true; }, 5000);
}

// ---- the view's controls: views and the display menu -------------------
const viewIndex = new Map(VIEWS.map((view, i) => [view[0], { view, i }]));
let recentViews = ['low', 'close'];
try {
  const saved = JSON.parse(localStorage.getItem('dt-preview-recent-views'));
  if (Array.isArray(saved)) {
    const valid = saved.filter((name) => viewIndex.has(name) && !['painted', 'top'].includes(name));
    recentViews = [...new Set([...valid, 'low', 'close'])].slice(0, 2);
  }
} catch { /* private mode or an older malformed value */ }

function viewButton(name) {
  const entry = viewIndex.get(name);
  if (!entry) return null;
  const [, label] = entry.view;
  const b = document.createElement('button');
  b.type = 'button'; b.className = 'view'; b.dataset.view = name;
  b.textContent = name === 'top' ? 'Map' : label;
  b.title = `${label} (${entry.i + 1})`;
  b.onclick = () => pickView(name);
  return b;
}

function showActiveView(name) {
  document.querySelectorAll('button.view').forEach((b) => {
    const on = b.dataset.view === name;
    b.classList.toggle('on', on);
    b.setAttribute('aria-pressed', on);
  });
}

function renderViewShortcuts() {
  $('views').replaceChildren(...shortcutViews(recentViews).map(viewButton).filter(Boolean));
  showActiveView(state.activeView);
}

VIEWS.forEach(([name]) => $('allviews').append(viewButton(name)));
renderViewShortcuts();
function pickView(name, animate = true) {
  state.view = name;
  state.activeView = name;
  recentViews = rememberView(recentViews, name);
  try { localStorage.setItem('dt-preview-recent-views', JSON.stringify(recentViews)); } catch { /* private mode */ }
  renderViewShortcuts();
  viewer.setView(name, animate);
  showActiveView(name);
}
viewer.onUserViewChange = () => { state.activeView = null; showActiveView(null); };
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
toggle('t-fade', (on) => { viewer.setFade(on); try { localStorage.setItem('dt-fade', on ? '1' : '0'); } catch { /* ignore */ } });
try { if (localStorage.getItem('dt-fade') === '0') { $('t-fade').checked = false; viewer.setFade(false); } } catch { /* ignore */ }

// popovers: one open at a time; Esc or a click outside closes it
const popovers = [
  ['dispbtn', 'display'], ['gate', 'record-card'], ['export', 'export-card'],
  ['undo-drop', 'undo-history'], ['redo-drop', 'redo-history'],
];
function openPopover(id) {
  for (const [btn, pop] of popovers) {
    const open = pop === id && $(pop).hidden;
    $(pop).hidden = !open;
    $(btn).setAttribute('aria-expanded', open);
  }
  if ((id === 'undo-history' || id === 'redo-history') && !$(id).hidden) renderHistory(true);
}
const closePopovers = () => {
  let any = false;
  for (const [btn, pop] of popovers) { any ||= !$(pop).hidden; $(pop).hidden = true; $(btn).setAttribute('aria-expanded', false); }
  return any;
};
for (const [btn, pop] of popovers) $(btn).onclick = (e) => { e.stopPropagation(); openPopover(pop); };
$('choose').onclick = () => {
  const lib = state.roots.findIndex((r) => r.kind === 'library');
  if (lib >= 0) state.root = lib;
  showBrowser();
};

// ---- the sidebar's two pages: the tile browser, and the editor beside it ----
const browsing = () => !$('side').classList.contains('editing');
function showBrowser() {
  layout.showSide();
  $('side').classList.remove('editing');
  $('browser').inert = false; $('editpage').inert = true;
  const t = tile();
  if (t && t.root !== state.root && state.roots[t.root] && !$('filter').value) state.root = t.root;
  renderList();
  // scroll the list only: scrollIntoView would scroll the sliding sidebar too
  const ul = $('tiles'), li = ul.querySelector('li.sel');
  if (li) ul.scrollTop = li.offsetTop - ul.offsetTop - (ul.clientHeight - li.offsetHeight) / 2;
  $('side').scrollLeft = 0;
  $('filter').focus({ preventScroll: true });
}
function showEditor() {
  if (!tile()) return;
  $('side').classList.add('editing');
  $('browser').inert = true; $('editpage').inert = false;
}
$('back').onclick = showBrowser;
$('forward').onclick = showEditor;
$('tilebtn').onclick = () => setSel({ tile: true });
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
  $('theme').setAttribute('aria-label', isDark ? 'Switch to Lichen & Linen' : 'Switch to Moonpool');
  $('theme').title = isDark ? 'Lichen & Linen light theme (t)' : 'Moonpool dark theme (t)';
  viewer.setAccent(getComputedStyle(document.documentElement).getPropertyValue('--accent').trim());
  viewer.setGridColor(getComputedStyle(document.documentElement).getPropertyValue('--grid-ink').trim());
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
// the panes: Edit (text over properties), Inspect (side by side), Look (the view alone)
const layout = new Layout($('app'), (st) => {
  $('reopen').hidden = !st.hideText && !st.hideProps;
  $('show-text').hidden = !st.hideText;
  $('show-props').hidden = !st.hideProps;
  const preset = st.hideText && st.hideProps ? 'look' : st.hideText || st.hideProps ? null : st.mode;
  for (const b of $('layout-seg').children) b.classList.toggle('on', b.dataset.preset === preset);
  $('t-right').checked = st.side === 'right';
});
$('hide-text').onclick = $('show-text').onclick = () => layout.toggle('text');
$('hide-props').onclick = $('show-props').onclick = () => layout.toggle('props');
for (const b of $('layout-seg').children) b.onclick = () => layout.preset(b.dataset.preset);
$('t-right').onchange = (e) => layout.set({ side: e.target.checked ? 'right' : 'left' });
const showKeys = () => { closePopovers(); $('keys').showModal(); };
$('b-keys').onclick = showKeys;
/** ⌘1/⌘2/⌘3: put the keyboard in the text, the view or the properties (showing the pane first). */
function focusPane(n) {
  const st = layout.state;
  if (n === 1) { if (st.hideText) layout.toggle('text'); texts.focus(); }
  if (n === 2) $('canvas').focus();
  if (n === 3) {
    if (st.hideProps) layout.toggle('props');
    const f = $('lens').querySelector('button, input, select, [tabindex], .val');
    if (f) { if (!f.hasAttribute('tabindex') && f.classList.contains('val')) f.tabIndex = -1; f.focus(); }
    else { $('lens').tabIndex = -1; $('lens').focus(); }
  }
}

addEventListener('keydown', (e) => {
  if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && ['1', '2', '3'].includes(e.key)) {
    e.preventDefault(); focusPane(Number(e.key)); return;
  }
  if (e.target.tagName !== 'INPUT' && e.target.tagName !== 'TEXTAREA' && !e.target.isContentEditable && tools?.key(e)) { e.preventDefault(); return; }
  if (e.key === 'Escape') {
    if (!$('mine').hidden) { $('mine').hidden = true; return; }
    if (closePopovers()) return;
    if (browsing() && tile() && !$('side').hidden) { showEditor(); return; }
    setSel(null);
    return;
  }
  // one undo for the whole page: typing and changes made in the view
  if ((e.metaKey || e.ctrlKey) && !e.altKey && e.target.tagName !== 'INPUT' && (e.key === 'z' || e.key === 'Z' || e.key === 'y')) {
    e.preventDefault();
    undo(e.key === 'y' || e.shiftKey ? 1 : -1);
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
  if (e.key === 'x') flip('t-fade');
  if (e.key === 't') toggleTheme();
  if (e.key === 'p') setProjection(!viewer.isOrtho);
  if (e.key === 'f' && state.sel) frameSel();
  if (e.key === 'a' || e.key === '/') { e.preventDefault(); $('place-in').focus(); }
  if (e.key === 'F2') { e.preventDefault(); startRename(); }
  if (e.key === '[') layout.toggle('text');
  if (e.key === ']') layout.toggle('props');
  if (e.key === '\\') layout.toggle('both');
  if (e.key === '?') { e.preventDefault(); showKeys(); }
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
    li.tabIndex = 0; li.setAttribute('role', 'option'); li.setAttribute('aria-selected', t.index === state.selected);
    li.title = t.path;
    li.innerHTML = '<span></span><span class="name"></span>';
    li.firstChild.className = dotClass(t);
    li.querySelector('.name').textContent = stem(t);
    li.onclick = async () => {
      if (t.index === state.selected) { showEditor(); return; }
      if ((await select(t.index)) !== false) showEditor();
    };
    li.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); li.click(); } };
    return li;
  };
  const header = (text, onclick, open) => {
    const g = document.createElement('li'); g.className = 'group';
    g.textContent = `${onclick ? (open ? '▾ ' : '▸ ') : ''}${text}`;
    if (onclick) {
      g.classList.add('fold'); g.tabIndex = 0; g.setAttribute('role', 'button'); g.setAttribute('aria-expanded', open);
      g.onclick = onclick;
      g.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onclick(); } };
    }
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
  if (e.key === 'Escape' && tile()) { e.preventDefault(); e.stopPropagation(); showEditor(); return; }
  if (e.key !== 'Enter') return;
  const first = $('tiles').querySelector('li:not(.group):not(.note)');
  first?.click();
};

// A first tile: valid, printable ground in My tiles, ready for the same
// text/lens/direct edits as every other tile.
function openNewTile() {
  closePopovers();
  const dlg = $('new-dialog');
  $('new-note').textContent = 'Starts as printable flat ground. Everything about it is ordinary text you can change.';
  dlg.showModal();
  $('new-name').focus();
  $('new-name').select();
}
$('new').onclick = openNewTile;
$('empty-new').onclick = openNewTile;
$('new-cancel').onclick = () => $('new-dialog').close();
$('new-form').onsubmit = async (e) => {
  e.preventDefault();
  if (!(await canLeave())) return;
  const ok = $('new-ok');
  ok.disabled = true;
  $('new-note').textContent = 'Making the tile…';
  let r;
  try {
    r = await host.post('/api/new', { name: $('new-name').value, size: $('new-size').value });
  } catch {
    r = { status: 0, body: { error: 'the editor is disconnected' } };
  }
  ok.disabled = false;
  if (r.status !== 200 || r.body?.index == null) {
    $('new-note').textContent = r.body?.error || 'The tile could not be made.';
    return;
  }
  $('new-dialog').close();
  const i = r.body.index;
  for (let k = 0; k < 40 && !state.tiles[i]; k++) await new Promise((res) => setTimeout(res, 50));
  if (state.tiles[i]) select(i, true);
};

// ---- renaming: a tile's file name is its name ---------------------------
const renameable = (t) => !!t && !t.path.startsWith('lib:') && /\.dtile$/i.test(t.path) && rootOf(t.root).kind === 'dir';
function startRename() {
  const t = tile();
  if (!renameable(t) || $('rename')) return;
  closePopovers();
  const input = document.createElement('input');
  input.id = 'rename'; input.type = 'text'; input.value = stem(t); input.maxLength = 80;
  input.setAttribute('aria-label', 'Rename the tile'); input.spellcheck = false; input.autocomplete = 'off';
  $('tilebtn').hidden = true;
  $('tilebtn').before(input);
  input.focus(); input.select();
  let done = false;
  const finish = async (commit) => {
    if (done) return;
    const name = input.value.trim();
    if (!commit || !name || name === stem(t)) { done = true; input.remove(); $('tilebtn').hidden = false; if (doc.note) { doc.note = null; renderTextBar(); } return; }
    done = true;
    input.disabled = true;
    if (!(await canLeave())) { input.remove(); $('tilebtn').hidden = false; return; }
    let r;
    try { r = await host.post(`/api/tiles/${t.index}/rename`, { name }); } catch { r = { status: 0, body: { error: 'the editor is disconnected' } }; }
    if (r.status !== 200 || r.body?.index == null) {
      // stay in the field with the reason, so the name can be fixed
      done = false; input.disabled = false; input.classList.add('bad');
      input.title = r.body?.error || 'The tile could not be renamed.';
      doc.note = input.title; renderTextBar(); input.focus();
      return;
    }
    input.remove(); $('tilebtn').hidden = false;
    doc.note = null;
    carried = { history: doc.history, at: doc.at };
    const i = r.body.index;
    for (let k = 0; k < 40 && !state.tiles[i]; k++) await new Promise((res) => setTimeout(res, 50));
    if (state.tiles[i]) await select(i, true);
  };
  input.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') { e.preventDefault(); finish(true); }
    if (e.key === 'Escape') { e.preventDefault(); finish(false); }
  };
  input.oninput = () => { input.classList.remove('bad'); input.title = ''; };
  input.onblur = () => finish(true);
}
$('tilebtn').ondblclick = (e) => { e.preventDefault(); closePopovers(); startRename(); };

// ---- choosing a tile ---------------------------------------------------
// ---- the camera before a tile's mesh is in --------------------------------
// The view starts on a 1×1 tile, and while a tile builds it follows what is
// known of its size: the plan's footprint, then each sketch the build sends.
// The finished mesh frames it; moving the view yourself stops the following.
const ONE_SQUARE = { lo: [0, 0, 0], hi: [35, 35, 8] };
let following = null; // {i, b}: the tile followed and the bounds last framed
function followBuild() {
  const t = tile();
  if (!following || !t || following.i !== t.index) return;
  const [w, h] = t.plan?.size_mm || [35, 35];
  const b = { lo: [0, 0, 0], hi: [w, h, 8] };
  for (const k of viewer.sketchBounds?.() || []) {
    for (let a = 0; a < 3; a++) { b.lo[a] = Math.min(b.lo[a], k.lo[a]); b.hi[a] = Math.max(b.hi[a], k.hi[a]); }
  }
  // a small change is not worth a camera move
  const size = (x) => Math.hypot(...x.hi.map((v, a) => v - x.lo[a]));
  const was = following.b;
  if (was && Math.abs(size(b) - size(was)) < 0.08 * size(was)
    && b.lo.every((v, a) => Math.abs(v - was.lo[a]) < 2) && b.hi.every((v, a) => Math.abs(v - was.hi[a]) < 2)) return;
  following.b = b;
  viewer.setView(state.view, true, b);
}
viewer.setView(state.view, false, ONE_SQUARE);
{
  const user = viewer.onUserViewChange;
  viewer.onUserViewChange = () => { following = null; user?.(); };
}

async function select(i, guarded = false) {
  if (!guarded && i !== state.selected && !(await canLeave())) return false;
  closeComp(); lens.hide();
  if (state.selected >= 0 && viewer.payload) {
    state.cameras.set(state.selected, { camera: viewer.getCamera(), activeView: state.activeView });
  }
  if (i !== state.selected) {
    state.sel = null; state.hov = null; state.activeView = null;
    $('tilebtn').classList.remove('sel');
    showActiveView(null);
  }
  state.selected = i;
  viewer.clear();
  viewer.setPlan(null);
  state.loaded.delete(i);
  const t = state.tiles[i];
  if (!t || t.gone) {
    state.selected = -1;
    history.replaceState(null, '', `#root=${state.root}`);
    texts.clear(); state.textFor = null; clearDocumentHistory();
    renderAll();
    return true;
  }
  state.root = t.root;
  state.lastInRoot.set(t.root, i);
  history.replaceState(null, '', `#tile=${i}`);
  // no mesh to frame yet and no view of yours kept: follow the build
  following = t.mesh_version == null && !state.cameras.get(i) ? { i, b: null } : null;
  if (t.plan) viewer.setPlan(t.plan);
  followBuild();
  if (t.status === 'idle') host.build(i);
  if (t.mesh_version != null) loadMesh(i, t.mesh_version);
  renderAll();
  // a build already running: what it has sent so far, not only what comes next
  showBuild(t);
  return true;
}

function countPieces(p) {
  state.pieces = new Map();
  for (let t = 0; t < p.triangles; t++) {
    const e = p.entry[t];
    if (e !== NO_ENTRY) state.pieces.set(e, Math.max(state.pieces.get(e) || 0, p.piece[t] + 1));
  }
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
    countPieces(p);
    surfaceHeights(p);
    drafts.shown = false;
    state.loaded.set(i, version);
    sketchCache.clear(); viewer.setSketches([]);
    showBuild(state.tiles[i]);
    if (first) {
      const saved = state.cameras.get(i);
      if (saved) {
        viewer.setCamera(saved.camera);
        state.activeView = saved.activeView;
        showActiveView(state.activeView);
      } else pickView(state.view, following?.i === i);
    }
    if (following?.i === i) following = null;
    // a selection the new build no longer has goes
    if (isEntry(state.sel) && state.sel.entry >= stackItems().length) state.sel = null;
    renderLens();
    renderAll();
  } catch (e) {
    console.error(e);
  }
}

// ---- the document: the text, one history, checking and saving -----------------
// The text pane edits the file shown (the tile's document, or a file it
// uses).  Every change is checked at once against the server (the errors
// and the stack, without building) and saved soon after when it reads
// cleanly; the worker sees the saved file and rebuilds.  A text with a
// mistake stays here, unsaved, until it is fixed.
let elements = [];
host.elements().then((v) => { elements = v; renderLens(); }).catch(() => {});
const elementNamed = (n) => elements.find((e) => e.name === n);

const doc = {
  tile: -1, file: null, base: null, writable: false,
  history: [], at: -1, lastKind: null, lastTime: 0,
  checkTimer: 0, saveTimer: 0, checkSeq: 0, saving: false,
  checked: null, ok: true, errors: [], stack: null, note: null, conflict: null,
  map: null, selectAt: null,
};
state.doc = doc;
const dirty = () => texts.text != null && doc.base != null && texts.text !== doc.base;

const texts = new TextPane($('code'), {
  onCaretLine: (n) => { closeComp(); setSel(entryAtLine(n), { from: 'text' }); },
  onHoverLine: (n) => setHov(n == null ? null : entryAtLine(n)),
  onEdit: (text, kind) => {
    edited(text, kind);
    const last = texts.caret().before.slice(-1);
    if (/[\w=+\-.]/.test(last)) openComp(false); else closeComp();
  },
  onReadOnly: () => offerMine(),
  onKey: (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { save(true); return true; }
    return compKey(e);
  },
  onHistory: (step) => undo(step),
});

/** The innermost entry written on line `n` of the file shown, else the tile (its own file) or nothing. */
function entryAtLine(line) {
  const n = line;
  let best = null, width = Infinity;
  stackItems().forEach((it, i) => {
    if (!it.lines || it.file !== texts.file) return;
    const [a, b] = it.lines;
    if (n >= a && n <= b && b - a < width) { best = i; width = b - a; }
  });
  if (best != null) return { entry: best, piece: null };
  if (texts.file !== tile()?.path) return null;
  for (const [kind, list] of [['region', doc.map?.regions], ['line', doc.map?.lines]]) {
    for (const node of list || []) {
      const l = node.span && spanLines(node.span);
      if (l && line >= l[0] && line <= l[1]) return { [kind]: node.name };
    }
  }
  return { tile: true };
}

/** The lines [first, last] (from 1) that byte span [offset, length] of the text covers. */
function spanLines([o, len]) {
  const t = texts.text || '';
  const a = byteToIndex(t, o), b = byteToIndex(t, o + Math.max(len, 1) - 1);
  return [t.slice(0, a).split('\n').length, t.slice(0, b).split('\n').length];
}
/** The map's region or line a selection names. */
function mapNode(w) {
  if (!w || !doc.map) return null;
  if (w.region != null) return doc.map.regions.find((r) => r.name === w.region) || null;
  if (w.line != null) return doc.map.lines.find((l) => l.name === w.line) || null;
  return null;
}

// one history for the file shown: typing (a pause or another kind of change
// starts a new step), changes from the lens, and changes on disk
function historyButton(entry, target, distance, direction) {
  const b = document.createElement('button');
  b.type = 'button';
  const action = document.createElement('span'); action.className = 'action'; action.textContent = entry.label;
  const away = document.createElement('span'); away.className = 'distance'; away.textContent = distance === 1 ? '1 step' : `${distance} steps`;
  b.append(action, away);
  if (entry.change?.removed || entry.change?.added) {
    const change = document.createElement('span'); change.className = 'change';
    if (entry.change.removed) {
      const removed = document.createElement('code'); removed.className = 'removed'; removed.textContent = `− ${entry.change.removed}`;
      change.append(removed);
    }
    if (entry.change.added) {
      const added = document.createElement('code'); added.className = 'added'; added.textContent = `+ ${entry.change.added}`;
      change.append(added);
    }
    b.append(change);
  }
  b.title = `${direction} to ${entry.label}`;
  b.onclick = () => { goHistory(target); closePopovers(); };
  return b;
}

function renderHistory(buildMenus = false) {
  const canUndo = doc.writable && doc.at > 0;
  const canRedo = doc.writable && doc.at >= 0 && doc.at < doc.history.length - 1;
  $('undo').disabled = !canUndo; $('undo-drop').disabled = !canUndo;
  $('redo').disabled = !canRedo; $('redo-drop').disabled = !canRedo;
  if (!buildMenus) return;
  const undoMenu = $('undo-history'), redoMenu = $('redo-history');
  undoMenu.replaceChildren(); redoMenu.replaceChildren();
  for (let target = doc.at - 1; target >= 0; target--) {
    undoMenu.append(historyButton(doc.history[target + 1], target, doc.at - target, 'Undo'));
  }
  for (let target = doc.at + 1; target < doc.history.length; target++) {
    redoMenu.append(historyButton(doc.history[target], target, target - doc.at, 'Redo'));
  }
}

function resetHistory(text) {
  doc.history = [{ text, caret: 0, kind: 'open', label: 'Opened document' }];
  doc.at = 0; doc.lastKind = null;
  renderHistory();
}
function clearDocumentHistory() {
  doc.writable = false; doc.base = null; doc.history = []; doc.at = -1; doc.lastKind = null;
  renderHistory();
}
function record(text, kind, label = null) {
  const now = performance.now();
  const join = kind === 'type' && doc.lastKind === 'type' && now - doc.lastTime < 1000 && doc.at > 0;
  doc.history.length = doc.at + 1;
  const before = doc.history[join ? doc.at - 1 : doc.at]?.text || '';
  const change = describeChange(before, text, kind, label);
  const h = { text, caret: texts.selectionStart, kind, label: change.title, change };
  if (join) doc.history[doc.at] = h; else { doc.history.push(h); doc.at++; }
  if (doc.history.length > 400) { doc.history.shift(); doc.at--; }
  doc.lastKind = kind; doc.lastTime = now;
  renderHistory();
}
function goHistory(i) {
  if (!doc.writable || i < 0 || i >= doc.history.length) return;
  doc.at = i; doc.lastKind = null;
  const h = doc.history[i];
  texts.setText(h.text, h.caret);
  closeComp();
  changed();
  renderHistory();
}
function undo(step) { goHistory(doc.at + step); }
$('undo').onclick = () => undo(-1);
$('redo').onclick = () => undo(1);

/** The user changed the text: `kind` is type, lens, or drag (a scrub under way, kept out of the history until it ends). */
function edited(text, kind, label = null) {
  if (kind !== 'drag') record(text, kind, label);
  else askDraft();
  changed();
}

// drafts: while a value is dragged the view shows quick builds of the text
// as it is (marked draft), one in flight at a time, always the latest text;
// the full build follows when the hand lets go and the file is saved
const drafts = { sent: 0, waiting: false, again: false, shown: false };
async function askDraft() {
  const t = tile();
  if (!t || !texts.file) return;
  if (drafts.waiting) { drafts.again = true; return; }
  drafts.waiting = true; drafts.again = false;
  drafts.sent += 1;
  await host.post(`/api/tiles/${t.index}/draft`, { text: texts.text, file: texts.file === t.path ? undefined : texts.file, id: drafts.sent });
  setTimeout(() => { if (drafts.waiting) { drafts.waiting = false; if (drafts.again) askDraft(); } }, 4000); // a draft that never came
}
async function draftArrived(t) {
  drafts.waiting = false;
  if (drafts.again) askDraft();
  if (t.draft_id !== drafts.sent || t.index !== state.selected) return; // an older text
  try {
    const p = parsePayload(await host.fetchMesh(t.index, true));
    if (t.index !== state.selected || t.draft_id !== drafts.sent) return;
    viewer.setMesh(p);
    countPieces(p);
    surfaceHeights(p);
    drafts.shown = true;
    showBuild(state.tiles[t.index]);
    renderTitle();
  } catch (e) { console.error(e); }
}

// ---- the build as it goes ----------------------------------------------
// While the tile builds, the mesh shown is the last build's: it fades. Each
// stage's sketch (what it has made, or a rough stand-in) replaces it as the
// build sends them, faded too, until the finished mesh lands. A draft
// being dragged is shown instead of either.
const sketchCache = new Map(); // sketch id -> Promise of its payload, or null
let sketchGen = 0;
function showBuild(t) {
  if (!t || t.index !== state.selected) return;
  const shown = state.loaded.get(t.index);
  const current = t.status === 'ready' && t.mesh_version === shown;
  viewer.setStale(shown != null && !drafts.shown && !current);
  syncSketches(t);
}
async function syncSketches(t) {
  // a finished mesh on its way: the sketches stay until it lands
  if (t.status === 'ready' && t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) return;
  const gen = ++sketchGen;
  const list = drafts.shown ? [] : (t.sketches || []);
  for (const k of list) {
    if (!sketchCache.has(k.id)) {
      sketchCache.set(k.id, host.fetchSketch(t.index, k.id).then(parsePayload).catch(() => null));
    }
  }
  const got = await Promise.all(list.map(async (k) => ({ id: k.id, rough: k.rough, payload: await sketchCache.get(k.id) })));
  if (gen !== sketchGen || t.index !== state.selected) return; // a newer list came
  const ids = new Set(list.map((k) => k.id));
  for (const id of sketchCache.keys()) if (!ids.has(id)) sketchCache.delete(id);
  viewer.setSketches(got.filter((k) => k.payload));
  followBuild();
}
function changed() {
  clearTimeout(doc.checkTimer);
  doc.checkTimer = setTimeout(check, 120);
  clearTimeout(doc.saveTimer);
  if (!lens.dragging) doc.saveTimer = setTimeout(save, 500);
  renderTextBar();
}

async function check() {
  const t = tile();
  if (!t || texts.text == null) return false;
  const text = texts.text, seq = ++doc.checkSeq;
  let r;
  try {
    r = await host.post(`/api/tiles/${t.index}/check`, { text, file: texts.file === t.path ? undefined : texts.file });
  } catch {
    if (seq === doc.checkSeq && text === texts.text && t === tile()) {
      doc.note = 'cannot check while the editor is disconnected';
      renderTextBar();
    }
    return false;
  }
  if (seq !== doc.checkSeq || text !== texts.text || !r.body || t !== tile()) return false;
  doc.note = null;
  doc.checked = text;
  doc.ok = !!r.body.ok;
  doc.errors = r.body.errors || [];
  if (r.body.ok && r.body.stack) doc.stack = r.body.stack; // the last good stack stays while there are mistakes
  if (r.body.ok && r.body.map) doc.map = r.body.map;
  if (doc.selectAt != null && r.body.ok) {
    // the node a gesture just wrote: select it
    const at = doc.selectAt; doc.selectAt = null;
    const i = (doc.stack || []).findIndex((it) => it.span && it.span[0] === at && it.file === texts.file);
    if (i >= 0) { setSel({ entry: i, piece: null }, { from: 'view' }); }
  }
  markText(); renderTextBar(); renderLens(); drawOverlay();
  return true;
}

async function save(now = false) {
  if (!doc.writable || !dirty()) return !dirty();
  if (doc.saving) return false;
  if (doc.checked !== texts.text) {
    if (now) await check();
    else { doc.saveTimer = setTimeout(save, 250); return false; }
  }
  if (doc.checked !== texts.text || !doc.ok) { renderTextBar(); return false; } // a mistake: kept here until it is fixed
  const t = tile(), text = texts.text, file = texts.file;
  if (!t || !file) return false;
  doc.saving = true;
  renderTextBar();
  try {
    const r = await host.post(`/api/tiles/${t.index}/save`, { text, base: doc.base, file: file === t.path ? undefined : file });
    doc.note = null;
    if (r.status === 200) { doc.base = text; doc.conflict = null; }
    else if (r.status === 409) doc.conflict = r.body?.text ?? '';
    else doc.note = r.body?.error || `could not save (${r.status})`;
  } catch {
    doc.note = 'cannot save while the editor is disconnected';
  } finally {
    doc.saving = false;
    renderTextBar();
  }
  if (dirty() && !doc.conflict && !doc.note) changed(); // typed while it saved
  return !dirty();
}
addEventListener('beforeunload', (e) => {
  if (dirty() && doc.writable) { e.preventDefault(); e.returnValue = ''; }
});

let leaving = null;
function confirmDiscard() {
  if (leaving) return leaving;
  const dlg = $('leave-dialog');
  $('leave-reason').textContent = doc.conflict != null
    ? 'The file changed on disk while your version was open. Choose which version to keep before leaving.'
    : !doc.ok ? 'This edit contains a mistake, so it has not been saved.'
      : doc.note || 'This edit has not been saved yet.';
  dlg.returnValue = ''; // showModal keeps the last choice: Esc must mean keep editing
  dlg.showModal();
  leaving = new Promise((resolve) => {
    dlg.addEventListener('close', () => {
      const discard = dlg.returnValue === 'discard';
      if (discard && doc.base != null) {
        texts.setText(doc.base);
        resetHistory(doc.base);
        doc.checked = doc.base; doc.ok = true; doc.errors = []; doc.conflict = null; doc.note = null;
      }
      leaving = null;
      resolve(discard);
    }, { once: true });
  });
  return leaving;
}

/** Save before leaving this file; if it cannot be saved, make discarding explicit. */
async function canLeave() {
  if (!dirty() || !doc.writable) return true;
  clearTimeout(doc.checkTimer); clearTimeout(doc.saveTimer);
  if (await save(true)) return true;
  return confirmDiscard();
}

let textToken = 0;
let carried = null; // the undo history a rename brings to the renamed file
async function loadText(file) {
  const t = tile();
  if (!t || /\.stl$/i.test(t.path)) { texts.clear(); state.textFor = null; clearDocumentHistory(); renderTextBar(); return; }
  const want = file || t.path;
  if (dirty() && texts.file !== want && !(await canLeave())) return false;
  // unsaved changes to this file stay; a new build of the same file brings nothing new
  if (dirty() && texts.file === want && doc.tile === t.index) return true;
  const key = [t.index, want, t.mesh_version, t.status, t.errors.length];
  if (state.textFor && state.textFor.join('|') === key.join('|')) return;
  state.textFor = key;
  const token = ++textToken;
  try {
    const v = await host.text(t.index, want === t.path ? null : want);
    if (token !== textToken || !v) { if (!v) state.textFor = null; return false; }
    if (dirty() && texts.file === v.file && doc.tile === t.index) return true;
    const fresh = doc.tile !== t.index || texts.file !== v.file;
    doc.tile = t.index;
    doc.writable = v.writable;
    doc.base = v.text;
    doc.conflict = null; doc.note = null;
    texts.show(v.file, v.text, v.writable);
    if (fresh && carried && carried.history[carried.at]?.text === v.text) {
      doc.history = carried.history; doc.at = carried.at; renderHistory(); // a rename keeps its undo
    } else if (fresh) resetHistory(v.text);
    else if (doc.history[doc.at]?.text !== v.text) record(v.text, 'disk'); // changed on disk: one more step
    doc.checked = v.text; doc.ok = !t.errors.length; doc.errors = []; doc.stack = null; // the build's plan is this text's
    if (fresh) { doc.map = null; carried = null; }
    if (v.file === t.path && !doc.map) check(); // its map, for the overlay
    renderTextBar();
    markText();
    renderLens();
    return true;
  } catch (e) { console.error(e); state.textFor = null; return false; }
}

function renderTextBar() {
  const t = tile();
  const f = texts.file;
  // the tile's own file is named in the crumb above; another file (a part) by its path
  $('textpath').textContent = !f || f === t?.path ? '' : `‎${f}`;
  $('textpath').title = f || '';
  const st = $('textstate');
  st.replaceChildren();
  const button = (label, fn, cls = 'back') => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = cls; b.textContent = label; b.onclick = fn;
    return b;
  };
  if (f && f !== t?.path) st.append(button('Back to the tile', () => loadText(null).then(() => { markText(); revealSel(); })));
  else if (f && !doc.writable) {
    st.append(f.startsWith('lib:') ? 'library · ' : 'read only · ');
    if (f.startsWith('lib:')) st.append(button('Make it mine', () => offerMine()));
  } else if (doc.conflict != null) {
    st.append('changed on disk: ');
    st.append(button('take the disk’s', () => { const d = doc.conflict; doc.conflict = null; doc.base = d; texts.setText(d); record(d, 'disk'); changed(); }), ' or ',
      button('keep mine', () => { doc.base = doc.conflict; doc.conflict = null; changed(); }));
  } else if (doc.note) st.textContent = doc.note;
  else if (f) st.textContent = doc.saving ? 'saving…' : !dirty() ? 'saved' : doc.checked === texts.text && !doc.ok ? 'not saved: a mistake' : 'editing…';
  // the first mistake, in words, under the text
  const hint = $('codehint');
  const e = doc.checked === texts.text && !doc.ok ? doc.errors.find((x) => !x.file || x.file === texts.file) || doc.errors[0] : null;
  hint.classList.toggle('bad', !!e);
  // the hint shows until the first edit; a mistake always shows, on one line (all of it on hover)
  if (dirty()) { try { localStorage.setItem('dt-hint-seen', '1'); } catch { /* ignore */ } }
  let seen = false;
  try { seen = localStorage.getItem('dt-hint-seen') === '1'; } catch { /* ignore */ }
  hint.textContent = e ? `${e.line ? `Line ${e.line}: ` : ''}${e.message}.${e.help ? ` ${e.help[0].toUpperCase()}${e.help.slice(1)}` : ''}`
    : doc.writable ? 'Type to change the tile; it saves as you go. ⌃Space: what can go here; ? for keys.' : 'Click a line to select it in the view.';
  hint.title = hint.textContent;
  hint.hidden = !e && doc.writable && seen;
}

function linesOf(w) {
  if (!w) return null;
  if (w.tile) return texts.file === tile()?.path ? [1, (texts.text || '').split('\n').length] : null;
  if (w.region != null || w.line != null) {
    const n = mapNode(w);
    return n?.span && texts.file === tile()?.path ? spanLines(n.span) : null;
  }
  const it = stackItems()[w.entry];
  return it?.lines && it.file === texts.file ? it.lines : null;
}

function markText() {
  const t = tile();
  const live = doc.checked === texts.text && doc.tile === state.selected;
  const errs = (live ? doc.errors : t?.errors || []).filter((e) => (!e.file || e.file === texts.file) && e.line)
    .map((e) => ({ line: e.line, message: `${e.message}${e.help ? `. ${e.help}` : ''}` }));
  texts.mark(state.sel?.tile ? null : linesOf(state.sel), linesOf(state.hov), errs);
}

// ---- completion -------------------------------------------------------------
const comp = { s: null, i: 0 };
function openComp(force) {
  if (!doc.writable || !elements.length) return closeComp();
  const s = suggest(texts.caret(), texts.text, elements, force);
  if (!s) return closeComp();
  comp.s = s; comp.i = 0;
  renderComp();
}
function renderComp() {
  const box = $('comp');
  box.replaceChildren();
  comp.s.items.forEach((it, i) => {
    const d = document.createElement('div');
    d.className = `it fam-${FAMILY[it.label] || (it.kind === 'definition' ? 'plant' : 'doc')}${i === comp.i ? ' on' : ''}`;
    d.setAttribute('role', 'option');
    d.setAttribute('aria-selected', i === comp.i);
    d.innerHTML = '<i class="sw"></i><span class="nm"></span><span class="ds"></span>';
    d.querySelector('.nm').textContent = it.label;
    d.querySelector('.ds').textContent = it.about || it.kind;
    d.onpointerdown = (e) => { e.preventDefault(); comp.i = i; acceptComp(); };
    box.append(d);
  });
  const xy = texts.caretXY();
  box.hidden = false;
  if (xy) {
    const h = box.offsetHeight, w = box.offsetWidth;
    box.style.left = `${Math.max(8, Math.min(xy.x - 6, innerWidth - w - 8))}px`;
    box.style.top = `${xy.y + h + 4 > innerHeight ? xy.y - h - 26 : xy.y + 2}px`;
  }
  box.children[comp.i]?.scrollIntoView({ block: 'nearest' });
}
function closeComp() { comp.s = null; $('comp').hidden = true; }
function acceptComp() {
  const it = comp.s?.items[comp.i];
  if (!it) return closeComp();
  const s = comp.s;
  closeComp();
  texts.insert(it.insert, s.from, s.to);
}
function compKey(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === ' ') { openComp(true); return true; }
  if (!comp.s) return false;
  const n = comp.s.items.length;
  if (e.key === 'ArrowDown') { comp.i = (comp.i + 1) % n; renderComp(); return true; }
  if (e.key === 'ArrowUp') { comp.i = (comp.i + n - 1) % n; renderComp(); return true; }
  if (e.key === 'Enter' || e.key === 'Tab') { acceptComp(); return true; }
  if (e.key === 'Escape') { closeComp(); return true; }
  return false;
}

// ---- make it mine -----------------------------------------------------------
function offerMine() {
  const t = tile();
  if (!t || !t.path.startsWith('lib:')) return;
  $('mine').hidden = false;
  $('mine-ok').focus();
}
$('mine-cancel').onclick = () => { $('mine').hidden = true; };
$('mine-ok').onclick = async () => {
  const t = tile();
  $('mine-ok').disabled = true;
  const r = await host.post(`/api/tiles/${t.index}/mine`, {});
  $('mine-ok').disabled = false;
  $('mine').hidden = true;
  if (r.status !== 200 || r.body?.index == null) { doc.note = r.body?.error || 'could not make a copy'; renderTextBar(); return; }
  // the copy joins the list with the next snapshot
  const i = r.body.index;
  for (let k = 0; k < 40 && !state.tiles[i]; k++) await new Promise((res) => setTimeout(res, 50));
  if (state.tiles[i]) select(i);
};

// ---- the selection ----------------------------------------------------------
function describe(w) {
  if (!w) return '';
  if (w.tile) return tile() ? `tile ${stem(tile())}` : 'tile';
  if (w.region != null) return `region ${w.region}`;
  if (w.line != null) return `line ${w.line}`;
  const it = stackItems()[w.entry];
  if (!it) return '';
  const n = piecesOf(w.entry);
  if (w.piece != null && n > 1) return `${it.label} · ${w.piece + 1} of ${n}`;
  return it.label;
}

function setSel(w, { from, guarded = false } = {}) {
  if (same(w, state.sel)) return;
  const targetFile = isEntry(w) ? stackItems()[w.entry]?.file
    : w && (w.tile || w.region != null || w.line != null) ? tile()?.path : null;
  if (!guarded && targetFile && targetFile !== texts.file && dirty()) {
    canLeave().then((ok) => { if (ok) setSel(w, { from, guarded: true }); });
    return;
  }
  state.sel = w;
  $('tilebtn').classList.toggle('sel', !!w?.tile);
  viewer.setHighlight(isEntry(w) ? w : null, hovForView());
  // the text follows: the selection's own file, scrolled to it
  if (isEntry(w)) {
    const it = stackItems()[w.entry];
    if (it?.file && it.file !== texts.file && from !== 'text') loadText(it.file).then(() => { revealSel(); renderLens(); });
  } else if (w && texts.file !== tile()?.path) loadText(null); // the tile, a region, a line: the tile's own text
  markText();
  tools?.paint();
  if (from !== 'text') revealSel();
  renderLens();
}
function revealSel() { const l = linesOf(state.sel); if (l) texts.reveal(l[0]); }

function setHov(w) {
  if (same(w, state.hov)) return;
  state.hov = w;
  viewer.setHighlight(isEntry(state.sel) ? state.sel : null, hovForView());
  markText();
}
const hovForView = () => (isEntry(state.hov) && !same(state.hov, state.sel) ? state.hov : null);

/** A click in the view on `hit`: select it, or climb when it is already selected. */
function clickHit(hit) {
  if (!hit) return setSel(null);
  const s = state.sel;
  const many = piecesOf(hit.entry) > 1;
  const inOf = (e) => (stackItems()[e] ? controlText(stackItems()[e], 'in') : null);
  if (s?.region != null && inOf(hit.entry) === s.region) return setSel({ tile: true }, { from: 'view' }); // the region -> the tile
  if (!isEntry(s) || s.entry !== hit.entry) return setSel({ entry: hit.entry, piece: many ? hit.piece : null }, { from: 'view' });
  if (s.piece != null) return setSel({ entry: hit.entry, piece: null }, { from: 'view' }); // the piece -> its entry
  const inr = inOf(s.entry);
  if (inr && doc.map?.regions.some((r) => r.name === inr)) return setSel({ region: inr }, { from: 'view' }); // the entry -> its region
  return setSel({ tile: true }, { from: 'view' }); // the entry -> the tile
}

/** Fly to the selection (f). */
function frameSel() {
  const s = state.sel;
  if (!isEntry(s)) return pickView(state.view);
  const b = viewer.centreOf(s.entry, s.piece);
  if (!b) return;
  const pose = viewer.pose();
  const size = Math.max(8, Math.hypot(b.hi[0] - b.lo[0], b.hi[1] - b.lo[1], b.hi[2] - b.lo[2]));
  pose.target.set((b.lo[0] + b.hi[0]) / 2, (b.lo[1] + b.hi[1]) / 2, (b.lo[2] + b.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 2.4 });
}

/** Explain the gate's paint by taking the camera to its most important face. */
function showWorst() {
  const f = viewer.worstFlagged();
  if (!f) return;
  if (f.entry != null) setSel({ entry: f.entry, piece: f.piece }, { from: 'view' });
  const pose = viewer.pose();
  const size = Math.max(4, Math.hypot(f.hi[0] - f.lo[0], f.hi[1] - f.lo[1], f.hi[2] - f.lo[2]));
  pose.target.set((f.lo[0] + f.hi[0]) / 2, (f.lo[1] + f.hi[1]) / 2, (f.lo[2] + f.hi[2]) / 2);
  viewer.flyTo({ target: pose.target, dir: pose.dir, dist: size * 3.2 });
  closePopovers();
  toast(f.kind === 'red'
    ? `Showing the largest face past ${state.steepestDeg}°`
    : `Showing the largest face in the ${state.overhangDeg}–${state.steepestDeg}° band`);
}
$('show-worst').onclick = showWorst;

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
    if (w && isEntry(state.sel) && state.sel.entry === w.entry) w = state.sel;
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

// ---- the lens ---------------------------------------------------------------
const lens = new Lens($('lens'), {
  writable: () => doc.writable,
  replace(start, len, text, end) {
    const t = texts.text;
    const i = byteToIndex(t, start), j = byteToIndex(t, start + len);
    texts.setText(t.slice(0, i) + text + t.slice(j));
    edited(texts.text, end ? 'lens' : 'drag');
  },
  async set(at, name, value) {
    if (!doc.writable) return offerMine();
    const r = await host.post('/api/edit', { text: texts.text, at, name, value });
    if (r.status !== 200) { doc.note = r.body?.error || 'that change could not be made'; renderTextBar(); return; }
    texts.setText(r.body.text);
    edited(texts.text, 'lens', `Changed ${name}`);
  },
  makeMine: () => offerMine(),
});
/** The lens for the selection: an entry's controls (a scatter's and each of its things'). */
// the tile node's own controls: shown when nothing is selected, so the sheet is never empty
const TILE_SCHEMA = {
  name: 'tile',
  controls: [
    { name: 'size', about: 'squares across, then up: "2x1"; click to type or pick it from a grid, or drag a number', default: '"1x1"', picker: 'size' },
    { name: 'seed', unit: 'number', about: 'the layout: another seed, another arrangement of the same tile', default: '1' },
  ],
};
// a written property as the lens reads it: `name=value` and its span
const asWritten = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: [c.span[0] - c.name.length - 1, c.span[1] + c.name.length + 1] });
/** A whole-node change to the tile's text (insert or remove), as one step of history. */
async function tileEdit(body, label) {
  if (!doc.writable) return offerMine();
  const r = await host.post('/api/edit', { text: texts.text, ...body });
  if (r.status !== 200) { doc.note = r.body?.error || 'that change could not be made'; renderTextBar(); return; }
  texts.setText(r.body.text);
  edited(texts.text, 'lens', label);
  return true;
}
const LEVEL_ABOUT = {
  edge: 'where the ground meets the tile edge; neighbouring tiles line up on it',
  ground: 'grass and soil regions',
  water: 'the calm water surface',
  'wall-top': 'the top of a wall',
};
function renderTileLens() {
  const t = tile(), own = doc.map?.tile;
  if (!t || !own || texts.file !== t.path) { lens.hide(); return; }
  const groups = [{ at: own.at, schema: TILE_SCHEMA, written: own.controls.map(asWritten) }];
  // the description: `about "…"` in the tile's body
  const about = own.about, av = about?.value;
  groups.push({
    heading: 'Description',
    at: own.at,
    schema: { controls: [{ name: 'about', unit: 'text', about: 'a sentence on what the tile is; shown in the library' }] },
    written: av ? [asWritten({ name: 'about', text: av.text, span: av.span })] : [],
    set: (_, value) => (value == null
      ? about && tileEdit({ op: 'remove', at: about.at }, 'Took out the description')
      : about ? null : tileEdit({ op: 'insert', place: 'tile', node: `about ${kdlString(String(value))}` }, 'Described the tile')),
  });
  // the tile's levels: the standard ones it can override, and its own
  const lv = own.levels;
  if (!lv?.block) {
    groups.push({
      heading: 'Levels (mm)',
      at: lv?.at ?? own.at,
      schema: {
        controls: (own.standard_levels || []).map((l) => ({
          name: l.name, unit: 'mm', default: String(l.value),
          about: `${LEVEL_ABOUT[l.name] || 'a standard level'}; ${l.value} mm unless this tile says otherwise`,
        })),
      },
      written: (lv?.controls || []).map(asWritten),
      set: (name, value) => {
        if (lv) {
          if (value == null && lv.controls.length === 1 && lv.controls[0].name === name) return tileEdit({ op: 'remove', at: lv.at }, `Took out level ${name}`);
          return lens.api.set(lv.at, name, value);
        }
        if (value == null) return null;
        return tileEdit({ op: 'insert', place: 'tile', node: `levels ${name}=${typeof value === 'number' ? value : kdlString(String(value))}` }, `Set level ${name}`);
      },
    });
  }
  const actions = doc.writable ? [
    { label: 'Rename', title: 'Rename the tile’s file (F2)', run: () => startRename() },
  ] : [];
  const note = lv?.block ? 'The tile. Its levels are written as a block: change them in the text.' : 'The tile. Select something on it to change that instead.';
  lens.show({ title: stem(t), kind: 'tile', href: 'https://shawn-vincent.github.io/dharmatiles/language.html', note, groups, actions });
}
/** A run of written text (a line's points, a region's lines) as a lens setting. */
const runWritten = (name, run) => (run?.span ? asWritten({ name, text: run.text, span: run.span }) : null);
/** Rename the selected line or region, and keep it selected under its new name. */
async function renameMark(kind, old, name) {
  const next = String(name ?? '').trim();
  if (!next || next === old) return;
  if (await tileEdit({ op: 'rename', mark: kind, old, new: next }, `Renamed ${kind} ${old} to ${next}`)) {
    state.sel = kind === 'line' ? { line: next } : { region: next };
  }
}
/** The sheet of a drawn line or a region: its name, its controls, its points. */
function renderMarkLens(kind, m) {
  const t = tile(), sh = m?.sheet;
  if (!t || !sh || texts.file !== t.path) { lens.hide(); return; }
  const nameRow = { name: 'name', unit: 'word', required: true, viaSet: true, about: `what the tile calls this ${kind}; renaming it renames every ${kind === 'line' ? '`bounded-by` and `along=`' : '`in=`'} that names it` };
  const written = [{ name: 'name', text: `name=${m.name}`, span: [sh.at, 0] }];
  let controls, set;
  if (kind === 'line') {
    controls = [
      nameRow,
      { name: 'wobble', unit: 'mm', default: '0', about: 'how far the line may wander off its points' },
      { name: 'width', unit: 'mm', about: 'makes the line a strip; an element along= it covers the strip' },
      { name: 'through', unit: 'raw', required: true, about: 'the points it runs through: "x,y" in squares, corners, or edge middles (south, east, north, west); drag a number or click to type' },
    ];
    written.push(...sh.controls.map(asWritten));
    const th = runWritten('through', sh.through);
    if (th) written.push(th);
    set = (name, value) => (name === 'name' ? renameMark('line', m.name, value) : lens.api.set(sh.at, name, value));
  } else {
    const lines = (doc.map?.lines || []).map((l) => ({ word: l.name, about: `bounded by the line ${l.name}` }));
    controls = [
      nameRow,
      { name: 'bounded-by', unit: 'several', words: lines, required: true, about: 'the lines that divide the tile into this region and the rest' },
      { name: 'fill', unit: 'raw', required: true, about: 'a point inside the region, "x,y" in squares: the region is the area around it that the lines enclose' },
    ];
    for (const [n, run] of [['bounded-by', sh.bounded_by], ['fill', sh.fill]]) { const w = runWritten(n, run); if (w) written.push(w); }
    set = (name, value) => {
      if (name === 'name') return renameMark('region', m.name, value);
      if (name === 'bounded-by') {
        const run = sh.bounded_by;
        if (!value.length) { doc.note = 'a region needs at least one line'; renderTextBar(); return null; }
        if (!run?.span) return null;
        return lens.api.replace(run.span[0], run.span[1], value.join(' '), true);
      }
      return null;
    };
  }
  const actions = doc.writable ? [{ label: 'Delete', title: `Delete this ${kind} from the text`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && setSel(null)) }] : [];
  lens.show({
    title: m.name, kind, href: `https://shawn-vincent.github.io/dharmatiles/language.html#${kind === 'line' ? 'lines' : 'regions'}`,
    groups: [{ at: sh.at, schema: { controls }, written, set }], actions,
  });
}
function renderLens() {
  const s = state.sel;
  if (s && (s.line != null || s.region != null)) { renderMarkLens(s.line != null ? 'line' : 'region', mapNode(s)); return; }
  const it = isEntry(s) ? stackItems()[s.entry] : null;
  if (!s || s.tile) { renderTileLens(); return; }
  if (!it || it.element === 'connector' || !it.span || it.file !== texts.file || !elements.length) { lens.hide(); return; }
  const groups = [{ at: it.span[0], schema: elementNamed(it.what), written: it.controls || [] }];
  for (const th of it.things || []) groups.push({ heading: th.what, at: th.at, schema: elementNamed(th.what), written: th.controls || [] });
  const actions = [];
  if (it.file === texts.file && doc.writable) {
    actions.push(
      { label: 'Hide', title: `Hide ${it.label} without deleting it`, run: () => muteAt(it.span[0], it.label) },
      { label: 'Delete', title: `Delete ${it.label} from the document`, run: () => removeAt(it.span[0], it.label) },
    );
  }
  lens.show({ title: it.label, kind: it.what || it.element, href: it.href, groups, actions });
}
viewer.onRender = () => drawOverlay();

// ---- the map, the bar and its tools ----------------------------------------
const overlay = new Overlay($('over'), viewer);
const SURFACES = new Set(['ground', 'dirt', 'floor', 'water']);
const COLOUR = { water: '--water', dirt: '--earth', ground: '--earth', floor: '--stone', grass: '--moss', reeds: '--moss', flowers: '--bloom' };
const cssVar = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
const controlText = (it, name) => [...(it.place || []), ...(it.controls || [])].find((c) => c.name === name)?.text.replace(new RegExp(`^${name}=`), '').replace(/"/g, '');

/** The ground's heights, from the surfaces the build made. */
function surfaceHeights(p) {
  const items = stackItems();
  overlay.setHeights(p, (e) => SURFACES.has(items[e]?.element), tile()?.plan?.size_mm || p.meta.footprint);
}

/** Each region's tint: the colour of the last thing laid in it. */
function tints() {
  const out = {};
  for (const it of stackItems()) {
    const r = controlText(it, 'in');
    if (r && COLOUR[it.element]) out[r] = cssVar(COLOUR[it.element]);
  }
  return out;
}

/**
 * The selection's handles: a line's points, a region's fill points, a
 * placed thing's at=.  Each {q (squares), span (bytes of the written
 * value), snap (edge words allowed), move(q) (the overlay follows)}.
 */
function handles() {
  const s = state.sel;
  if (!s || !doc.writable || texts.file !== tile()?.path || !doc.map) return [];
  if (s.line != null) {
    const l = mapNode(s);
    return (l?.points || []).filter((p) => p.at).map((p, k) => ({
      q: p.at, span: p.span, snap: true,
      move(q) { if (!l.wobble && l.pts.length === l.points.length) l.pts[k] = q; },
    }));
  }
  if (s.region != null) return (mapNode(s)?.fills || []).filter((f) => f.at).map((f) => ({ q: f.at, span: f.span, snap: false, move(q) { f.at = q; } }));
  if (isEntry(s)) {
    const it = stackItems()[s.entry];
    const c = (it?.place || []).find((x) => x.name === 'at');
    if (!c || it.file !== texts.file) return [];
    const v = valueOf(c.text, 'at');
    const m = v.text.match(/^"(-?[\d.]+),(-?[\d.]+)"$/);
    if (!m) return [];
    const start = c.span[0] + new TextEncoder().encode(c.text.slice(0, v.at)).length;
    return [{ q: [Number(m[1]), Number(m[2])], span: [start, new TextEncoder().encode(v.text).length], snap: false }];
  }
  return [];
}

function drawOverlay() {
  const t = tile();
  overlay.setMap(doc.tile === state.selected ? doc.map : null, t?.plan?.square_mm);
  const s = state.sel;
  overlay.draw({
    tints: tints(),
    selRegion: s?.region ?? null,
    selLine: s?.line ?? null,
    all: !!tools?.tool,
    ...(tools?.scene() || {}),
  });
}

/** Write a new node from a gesture in the view, then select it. */
async function insertNode(place, node, { select: sel } = {}) {
  const t = tile();
  if (!t) return;
  if (!doc.writable) return offerMine();
  if (texts.file !== t.path && !(await loadText(null))) return;
  const r = await host.post('/api/edit', { op: 'insert', text: texts.text, place, node });
  if (r.status !== 200) { doc.note = r.body?.error || 'that could not be written'; renderTextBar(); return; }
  texts.setText(r.body.text, byteToIndex(r.body.text, r.body.at));
  if (sel) { state.sel = null; setSel(sel, { from: 'view' }); } else doc.selectAt = r.body.at;
  edited(texts.text, 'view', `Added ${node.trim().split(/\s+/)[0]}`);
  askDraft();
  const l = texts.text.slice(0, byteToIndex(texts.text, r.body.at)).split('\n').length;
  texts.reveal(l);
}

tools = new Tools(canvas, viewer, overlay, { bar: $('place'), input: $('place-in'), list: $('place-list'), chip: $('tool') }, {
  elements: () => elements,
  defs: () => [...(texts.text || '').matchAll(/^\s*define\s+([\w+\-.]+)/gm)].map((m) => m[1]),
  sq: () => tile()?.plan?.square_mm || 35,
  size: () => [tile()?.plan?.cols || 1, tile()?.plan?.rows || 1],
  map: () => doc.map,
  writable: () => doc.writable,
  offerMine: () => offerMine(),
  insert: (place, node, opts) => insertNode(place, node, opts),
  replace: (start, len, text, end) => lens.api.replace(start, len, text, end),
  handles,
  redraw: () => drawOverlay(),
  note: (text) => { const h = $('hint'); h.textContent = text; h.style.display = 'block'; h.style.left = '12px'; h.style.top = '56px'; setTimeout(() => { h.style.display = 'none'; }, 2600); },
});
$('tool').querySelector('button').onclick = () => tools.stop();

// each element's family, for its colour (the docs site uses the same)
const FAMILY = {
  ground: 'surface', dirt: 'surface', floor: 'surface', water: 'water',
  rock: 'thing', stone: 'thing', boulder: 'thing', pebble: 'thing', scatter: 'thing', cliff: 'thing', wall: 'thing',
  grass: 'plant', reeds: 'plant', flowers: 'plant', tree: 'plant',
  dungeonblocks: 'base', openlock: 'base', connector: 'base', stl: 'base',
};

function muteAt(byte, label = 'Entry') {
  if (!doc.writable) return offerMine();
  const t = texts.text, i = byteToIndex(t, byte);
  if (isEntry(state.sel)) setSel(null);
  texts.setText(`${t.slice(0, i)}/- ${t.slice(i)}`);
  edited(texts.text, 'view', `Hid ${label}`);
  askDraft();
  toast(`${label} hidden`, () => undo(-1));
}
async function removeAt(byte, label = 'Entry') {
  if (!doc.writable) return offerMine();
  const r = await host.post('/api/edit', { op: 'remove', text: texts.text, at: byte });
  if (r.status !== 200) { doc.note = r.body?.error || 'that could not be taken out'; renderTextBar(); return; }
  setSel(null);
  texts.setText(r.body.text);
  edited(texts.text, 'view', `Deleted ${label}`);
  askDraft();
  toast(`${label} removed`, () => undo(-1));
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
  viewer.setHighlight(isEntry(state.sel) ? state.sel : null, hovForView());
  const t = tile();
  if (t) loadText(state.textFor && state.textFor[0] === t.index && state.textFor[1] !== t.path ? state.textFor[1] : null);
  markText();
}

// ---- export: the same build as `dharmatiles build`, written to out/ ----------
const exporting = { system: null, busy: false, files: null, error: null, out: null };
try { exporting.system = localStorage.getItem('dt-export-system'); } catch { /* ignore */ }
try { $('export-3mf').checked = localStorage.getItem('dt-export-3mf') === '1'; } catch { /* ignore */ }
/** Why export is not possible now, or null. */
function exportBlock() {
  const t = tile();
  if (!t) return 'Choose a tile first.';
  if (t.status === 'building' || t.status === 'idle') return 'Wait for the build to finish.';
  if (doc.writable && dirty()) return 'Save your changes first: export builds the file as saved.';
  if (!t.record) return 'The tile has no good build yet: fix the mistakes first.';
  if (t.record.audit.failures.length) return `It fails the printability gate: ${t.record.audit.failures[0]}. Fix the geometry first.`;
  return null;
}
function renderExport() {
  const t = tile();
  $('export').hidden = !t;
  const why = exporting.busy ? null : exportBlock();
  const sys = exporting.system || state.system || 'db';
  for (const b of $('export-system').children) b.classList.toggle('on', b.dataset.sys === sys);
  $('export-go').disabled = !!why || exporting.busy;
  $('export-go').textContent = exporting.busy ? 'Exporting…' : $('export-3mf').checked ? 'Export STL and 3MF' : 'Export STL';
  const p = $('export-why');
  p.classList.toggle('bad', !!(why || exporting.error));
  p.replaceChildren();
  if (exporting.busy) p.textContent = 'Building through the gate…';
  else if (exporting.error) p.textContent = exporting.error;
  else if (exporting.files) {
    p.append('Written: ');
    exporting.files.filter((f) => /\.(stl|3mf)$/.test(f)).forEach((f, i) => {
      if (i) p.append(', ');
      const c = document.createElement('code');
      c.textContent = f.split(/[\\/]/).slice(-3).join('/'); c.title = f; // out/db/name-db.stl; the whole path on hover
      p.append(c);
    });
  } else {
    p.textContent = why || (exporting.out ? `Writes to ${exporting.out.shown}` : 'Writes to out/, as dharmatiles build does.');
    p.title = exporting.out?.out || '';
  }
  $('export-reveal').hidden = !exporting.files;
}
for (const b of $('export-system').children) {
  b.onclick = () => { exporting.system = b.dataset.sys; exporting.files = null; try { localStorage.setItem('dt-export-system', b.dataset.sys); } catch { /* ignore */ } renderExport(); };
}
$('export-3mf').onchange = (e) => { try { localStorage.setItem('dt-export-3mf', e.target.checked ? '1' : '0'); } catch { /* ignore */ } exporting.files = null; renderExport(); };
$('export').addEventListener('click', async () => {
  exporting.files = null; exporting.error = null; exporting.out = null; renderExport();
  const t = tile();
  if (!t) return;
  try {
    const r = await host.post(`/api/tiles/${t.index}/export`, { where: true });
    if (r.status === 200 && t === tile()) { exporting.out = r.body; renderExport(); }
  } catch { /* said when exporting */ }
});
$('export-go').onclick = async () => {
  const t = tile();
  if (!t || exportBlock()) return renderExport();
  exporting.busy = true; exporting.error = null; exporting.files = null; renderExport();
  try {
    const r = await host.post(`/api/tiles/${t.index}/export`, { system: exporting.system || state.system || 'db', threemf: $('export-3mf').checked });
    if (r.status !== 200) exporting.error = r.body?.error || `could not export (${r.status})`;
    else if (!r.body.ok) exporting.error = r.body.errors?.[0]?.message || (r.body.failures?.length ? `Fails the gate: ${r.body.failures[0]}` : 'the build failed');
    else exporting.files = r.body.files || [];
    if (r.status === 200 && r.body.files && !r.body.ok) exporting.files = r.body.files;
  } catch { exporting.error = 'cannot export while the editor is disconnected'; }
  exporting.busy = false;
  renderExport();
};
$('export-reveal').onclick = () => { const f = exporting.files?.find((x) => /\.(stl|3mf)$/.test(x)); if (f) host.post('/api/reveal', { path: f }); };

function renderTitle() {
  const t = tile();
  $('empty').hidden = !!t;
  const status = $('status'), errs = $('errors'), dl = $('record'), gate = $('gate');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  $('verdict').hidden = true;
  $('show-worst').hidden = true;
  if (!t) {
    status.hidden = true; errs.hidden = true; gate.hidden = true; $('export').hidden = true;
    $('tilename').textContent = ''; $('tilewhere').textContent = 'Tiles'; $('tileplan').textContent = '';
    $('forward').hidden = true;
    if (!browsing()) showBrowser();
    $('progress').style.width = '0';
    document.title = 'dharmatiles';
    return;
  }
  $('tilename').textContent = stem(t);
  $('tilewhere').textContent = rootOf(t.root).label || 'Tiles';
  $('back').title = `All of ${rootOf(t.root).label || 'the tiles'}`;
  $('forward').hidden = false; $('forward-name').textContent = stem(t);
  $('tilebtn').title = renameable(t) ? `${t.path}\nClick for the tile's properties; double-click or F2 to rename` : `${t.path}\nClick for the tile's properties`;
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
  renderExport();
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
  if (drafts.shown) {
    const tag = document.createElement('span');
    tag.className = 'tag'; tag.textContent = 'draft';
    tag.title = 'The view shows a quick draft; the verdict is the last full build\u2019s';
    $('gatetext').append(' ', tag);
  }
  const v = $('verdict');
  v.hidden = false;
  v.className = fails ? 'fail' : 'pass';
  v.textContent = fails ? `Fails the printability gate${stale}` : `Passes the printability gate${stale}`;
  const worst = viewer.worstFlagged();
  if (worst) {
    const show = $('show-worst');
    show.hidden = false;
    show.textContent = worst.kind === 'red'
      ? `Inspect largest face past ${state.steepestDeg}°`
      : `Inspect largest face at ${state.overhangDeg}–${state.steepestDeg}°`;
  }
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
      showBuild(cur);
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
    const before = state.tiles[t.index];
    state.tiles[t.index] = t;
    if (t.draft_id != null && t.draft_id !== before?.draft_id) draftArrived(t);
    if (t.index !== state.selected) { if (browsing()) renderList(); return; }
    if (t.plan && JSON.stringify(t.plan) !== JSON.stringify(viewer.plan)) { viewer.setPlan(t.plan); followBuild(); }
    renderAll();
    showBuild(t);
    // the new mesh swaps in place: the camera is kept
    if (t.mesh_version != null && state.loaded.get(t.index) !== t.mesh_version) loadMesh(t.index, t.mesh_version);
  },
});
