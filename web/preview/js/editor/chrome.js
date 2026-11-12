// The page's frame: the view's controls and display menu, popovers, the
// sidebar's two pages, light and dark, the projection, the panes and the
// keyboard.

import { Layout } from '../layout.js';
import { rememberView, shortcutViews } from '../view-presets.js';
import { VIEWS } from '../viewer.js';
import { legendRows } from '../debug-layer.js';
import { $, host, pickView, viewer } from './app.js';
import { copyButton, versionParts } from '../copy.js';
import { server, tile, view } from './stores.js';

// ---- the view's controls: views and the display menu -------------------

const viewIndex = new Map(VIEWS.map((view, i) => [view[0], { view, i }]));
let recentViews = ['low', 'close'];

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
  showActiveView(view.get().activeView);
}
/** The camera goes to preset `name` (asked for with app.js's pickView). */
function applyPick({ name, animate }) {
  view.set({ mode: name, activeView: name });
  recentViews = rememberView(recentViews, name);
  try { localStorage.setItem('dt-preview-recent-views', JSON.stringify(recentViews)); } catch { /* private mode */ }
  renderViewShortcuts();
  viewer.setView(name, animate);
}
const toggle = (id, fn) => { $(id).onchange = (e) => fn(e.target.checked); };
// paint on or off (grey plastic): remembered in the browser
function setPaint(on) {
  $('t-paint').checked = on;
  viewer.setPaint(on);
  try { localStorage.setItem('dt-preview-paint', on ? '1' : '0'); } catch { /* private mode: the default stands */ }
}

// the debug layer: off at every load; ⌃⌥D or the ⋯ menu
function setDebug(on) {
  $('t-debug').checked = on;
  view.set({ debug: on }); // the program collects the layer's marks for a tile only while it is on (build.js tells it)
  viewer.setDebugShown(on);
}
const cssToken = (t) => getComputedStyle(document.documentElement).getPropertyValue(t).trim();
/** The debug layer's legend, beside the view while the layer is on. */
function renderDebugLegend(d) {
  const box = $('debug-legend');
  box.hidden = !d;
  if (!d) return;
  const rows = legendRows(d).map((r) => {
    const li = document.createElement('li');
    const sw = document.createElement('i');
    sw.className = `sw ${r.kind}`;
    sw.style.background = cssToken(r.token);
    const label = document.createElement('span');
    label.textContent = r.label;
    li.append(sw, label);
    if (r.kind === 'dot' || r.kind === 'line') {
      const n = document.createElement('span');
      n.className = 'n';
      n.textContent = r.n;
      li.append(n);
    }
    return li;
  });
  if (!rows.length) {
    const li = document.createElement('li');
    li.textContent = 'Nothing in this tile shows here yet: grass does.';
    rows.push(li);
  }
  $('debug-rows').replaceChildren(...rows);
  $('debug-thinned').hidden = !d.thinned;
}

// popovers: one open at a time (`view.popover`); Esc or a click outside closes it
const popovers = [
  ['dispbtn', 'display'], ['gate', 'record-card'], ['export', 'export-card'], ['target', 'target-card'],
  ['undo-drop', 'undo-history'], ['redo-drop', 'redo-history'],
];
const openPopover = (id) => view.set({ popover: view.get().popover === id ? null : id });
function renderPopovers() {
  const open = view.get().popover;
  for (const [btn, pop] of popovers) {
    $(pop).hidden = pop !== open;
    $(btn).setAttribute('aria-expanded', pop === open);
  }
}
/** Close the open popover; true if there was one. */
const closePopovers = () => {
  const any = view.get().popover != null;
  view.set({ popover: null });
  return any;
};

// ---- the sidebar's two pages: the tile browser, and the editor beside it ----

const browsing = () => view.get().page === 'browse';
function renderPage() {
  if (!browsing()) {
    $('side').classList.add('editing');
    $('browser').inert = true; $('editpage').inert = false;
    return;
  }
  layout.showSide();
  $('side').classList.remove('editing');
  $('browser').inert = false; $('editpage').inert = true;
  const t = tile(), { roots } = server.get();
  if (t && t.root !== view.get().root && roots[t.root] && !$('filter').value) view.set({ root: t.root });
  // scroll the list only: scrollIntoView would scroll the sliding sidebar too
  const ul = $('tiles'), li = ul.querySelector('li.sel');
  if (li) ul.scrollTop = li.offsetTop - ul.offsetTop - (ul.clientHeight - li.offsetHeight) / 2;
  $('filter').focus({ preventScroll: true });
}

// light or dark: the system's setting until the button is used; then the choice is remembered
const dark = matchMedia('(prefers-color-scheme: dark)');
function applyTheme() {
  let stored = null;
  try { stored = localStorage.getItem('dt-theme'); } catch { /* private mode: the default stands */ }
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
  try { localStorage.setItem('dt-theme', isDark ? 'light' : 'dark'); } catch { /* private mode: the default stands */ }
  applyTheme();
}

// perspective or orthographic: remembered across reloads (a per-viewer convenience)
function setProjection(ortho, animated = true) {
  viewer.setOrtho(ortho, animated);
  $('b-proj').firstChild.textContent = ortho ? 'Orthographic ' : 'Perspective ';
  try { localStorage.setItem('dt-preview-ortho', ortho ? '1' : '0'); } catch { /* private mode */ }
}
// the panes: Edit (source over properties), Inspect (side by side), Look (the view alone)
const layout = new Layout($('app'), (st) => {
  $('reopen').hidden = !st.hideSource && !st.hideProps;
  $('show-source').hidden = !st.hideSource;
  $('show-props').hidden = !st.hideProps;
  const preset = st.hideSource && st.hideProps ? 'look' : st.hideSource || st.hideProps ? null : st.mode;
  for (const b of $('layout-seg').children) b.classList.toggle('on', b.dataset.preset === preset);
  $('t-right').checked = st.side === 'right';
});
/** The About box: the application version and the tile formats it reads, asked of the program. */
async function showAbout() {
  closePopovers();
  const err = $('about-error');
  err.hidden = true;
  $('about').showModal();
  try {
    const a = await host.about();
    // 0.9.0+123.gfd383a7.dirty: the semantic version large, the whole build part after it (dirty included), and a copy button for all of it.
    const v = versionParts(a.version);
    $('about-semver').textContent = v.semver;
    $('about-build').textContent = v.build;
    $('about-dirty').hidden = !v.dirty;
    $('about-version').querySelector('.copy')?.remove();
    $('about-version').append(copyButton(a.version, { title: 'Copy the version' }));
    // Formats are major versions, written "0.x": any version starting with 0.
    $('about-formats').textContent = a.formats;
  } catch (e) {
    err.textContent = `Could not read the version: ${e.message}`;
    err.hidden = false;
    throw e;
  }
}
const showKeys = () => { closePopovers(); $('keys').showModal(); };
/** ⌘1/⌘2/⌘3: put the keyboard in the source, the view or the properties (showing the pane first). */
function focusPane(n) {
  const st = layout.state;
  if (n === 1) { if (st.hideSource) layout.toggle('source'); $('code').querySelector('.cm-content')?.focus(); }
  if (n === 2) $('canvas').focus();
  if (n === 3) {
    if (st.hideProps) layout.toggle('props');
    const f = $('lens').querySelector('button, input, select, [tabindex], .val');
    if (f) { if (!f.hasAttribute('tabindex') && f.classList.contains('val')) f.tabIndex = -1; f.focus(); }
    else { $('lens').tabIndex = -1; $('lens').focus(); }
  }
}

/** Wire this part of the page (once, at load). */
export function init() {
  try {
    const saved = JSON.parse(localStorage.getItem('dt-preview-recent-views'));
    if (Array.isArray(saved)) {
      const valid = saved.filter((name) => viewIndex.has(name) && !['painted', 'top'].includes(name));
      recentViews = [...new Set([...valid, 'low', 'close'])].slice(0, 2);
    }
  } catch { /* private mode or an older malformed value */ }
  VIEWS.forEach(([name]) => $('allviews').append(viewButton(name)));
  renderViewShortcuts();
  viewer.onUserViewChange = () => view.set({ activeView: null });
  view.subscribe(['pick'], ({ pick }) => applyPick(pick));
  view.subscribe(['activeView'], ({ activeView }) => showActiveView(activeView));
  view.subscribe(['popover'], renderPopovers);
  view.subscribe(['page'], renderPage);
  // no tile shown: the browser is the page
  view.subscribe(['tileId'], () => { if (view.get().tileId == null) view.set({ page: 'browse' }); });
  // the gate's legend names the angles the program checks against
  server.subscribe(['overhangDeg', 'steepestDeg'], ({ overhangDeg, steepestDeg }) => {
    $('lg-orange').textContent = `${overhangDeg}–${steepestDeg}°: allowed within budget`;
    $('lg-red').textContent = `Over ${steepestDeg}°: small patches allowed, more fails`;
  });
  toggle('t-overhang', (on) => viewer.setOverhang(on));
  $('t-paint').onchange = (e) => setPaint(e.target.checked);
  try { if (localStorage.getItem('dt-preview-paint') === '0') setPaint(false); } catch { /* private mode: the default stands */ }
  if (/paint=off/.test(location.hash)) { $('t-paint').checked = false; viewer.setPaint(false); } // #paint=off previews grey without saving
  toggle('t-wire', (on) => viewer.setWireframe(on));
  viewer.onDebugChange = renderDebugLegend;
  viewer.setDebugColours(cssToken);
  toggle('t-debug', setDebug);
  toggle('t-grid', (on) => viewer.setGrid(on));
  toggle('t-fade', (on) => { viewer.setFade(on); try { localStorage.setItem('dt-fade', on ? '1' : '0'); } catch { /* private mode: the default stands */ } });
  try { if (localStorage.getItem('dt-fade') === '0') { $('t-fade').checked = false; viewer.setFade(false); } } catch { /* private mode: the default stands */ }
  for (const [btn, pop] of popovers) $(btn).onclick = (e) => { e.stopPropagation(); openPopover(pop); };
  $('choose').onclick = () => {
    const lib = server.get().roots.findIndex((r) => r.kind === 'library');
    if (lib >= 0) view.set({ root: lib });
    view.set({ page: 'browse' });
    renderPage();
  };
  $('back').onclick = () => { view.set({ page: 'browse' }); renderPage(); };
  $('forward').onclick = () => { if (tile()) view.set({ page: 'edit' }); };
  addEventListener('pointerdown', (e) => {
    if (popovers.some(([b, p]) => $(p).contains(e.target) || $(b).contains(e.target))) return;
    closePopovers();
  });
  $('theme').onclick = toggleTheme;
  dark.addEventListener('change', applyTheme);
  applyTheme();
  $('b-proj').onclick = () => setProjection(!viewer.isOrtho);
  try { if (localStorage.getItem('dt-preview-ortho') === '1') setProjection(true, false); } catch { /* private mode: the default stands */ }
  $('hide-source').onclick = $('show-source').onclick = () => layout.toggle('source');
  $('hide-props').onclick = $('show-props').onclick = () => layout.toggle('props');
  for (const b of $('layout-seg').children) b.onclick = () => layout.preset(b.dataset.preset);
  $('t-right').onchange = (e) => layout.set({ side: e.target.checked ? 'right' : 'left' });
  $('b-keys').onclick = showKeys;
  $('b-about').onclick = showAbout;
  addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && ['1', '2', '3'].includes(e.key)) {
      e.preventDefault(); focusPane(Number(e.key)); return;
    }
    // ⌃⌥D, by the key's place (⌥ changes the character on a Mac)
    if (e.ctrlKey && e.altKey && !e.metaKey && !e.shiftKey && e.code === 'KeyD') {
      e.preventDefault(); setDebug(!$('t-debug').checked); return;
    }
    if (e.defaultPrevented) return; // a tool (map.js) or the undo (document.js) took it
    if (e.key === 'Escape') {
      if (view.get().mineOpen) { view.set({ mineOpen: false }); return; }
      if (closePopovers()) return;
      if (browsing() && tile() && !$('side').hidden) { view.set({ page: 'edit' }); return; }
      view.set({ sel: null }); // selection.js reflects it
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
    if (e.key === 'a' || e.key === '/') { e.preventDefault(); $('place-in').focus(); }
    if (e.key === '[') layout.toggle('source');
    if (e.key === ']') layout.toggle('props');
    if (e.key === '\\') layout.toggle('both');
    if (e.key === '?') { e.preventDefault(); showKeys(); }
  });
}
