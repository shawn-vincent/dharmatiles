// The page's frame: the view's controls and display menu, popovers, the
// sidebar's two pages, light and dark, the projection, the panes and the
// keyboard.

import { effect, signal } from '@preact/signals-core';
import { Layout } from '../layout.js';
import { CopyButton } from '../view/copy-button.js';
import { html, render } from '../view/html.js';
import { Backdrops, DebugRows, ViewButtons } from '../view/view-controls.js';
import { rememberView, shortcutViews } from '../view-presets.js';
import { VIEWS } from '../viewer.js';
import { KINDS as BACKDROPS } from '../viewer/backdrop.js';
import { legendRows } from '../debug-layer.js';
import { $, host, pickView, viewer } from './app.js';
import { versionParts } from '../copy.js';
import { goUp, setFace } from './levels.js';
import { isInventory, isPlatesView, placeAt, server, tile, view } from './stores.js';

// ---- the view's controls: views and the display menu -------------------

const viewEntries = new Map(VIEWS.map(([name, label], i) => [name, { label, n: i + 1 }]));
/** The two views beside the view besides Iso and Top: the ones used last (remembered in the browser). */
const recentViews = signal(['low', 'close']);

/** The camera goes to preset `name` (asked for with app.js's pickView). */
function applyPick({ name, animate }) {
  view.set({ mode: name, activeView: name });
  recentViews.value = rememberView(recentViews.value, name);
  try { localStorage.setItem('dt-preview-recent-views', JSON.stringify(recentViews.value)); } catch { /* private mode */ }
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
/** What the debug layer holds now (null: the layer is off): the legend follows it. */
const debugNow = signal(/** @type {any} */ (null));
function bindDebugLegend() {
  viewer.onDebugChange = (d) => { debugNow.value = d; };
  effect(() => {
    const d = debugNow.value;
    $('debug-legend').hidden = !d;
    render(d ? html`<${DebugRows} rows=${legendRows(d)} colour=${cssToken} />` : null, $('debug-rows'));
    $('debug-thinned').hidden = !d?.thinned;
  });
}

// popovers: one open at a time (`view.popover`); Esc or a click outside closes it
const popovers = [
  ['dispbtn', 'display'], ['export', 'export-card'], ['target', 'target-card'],
  ['undo-drop', 'undo-history'], ['redo-drop', 'redo-history'],
];
const openPopover = (id) => view.set({ popover: view.get().popover === id ? null : id });
function bindPopovers() {
  effect(() => {
    const open = view.get().popover;
    for (const [btn, pop] of popovers) {
      $(pop).hidden = pop !== open;
      $(btn).setAttribute('aria-expanded', pop === open);
    }
  });
}
/** Close the open popover; true if there was one. */
const closePopovers = () => {
  const any = view.get().popover != null;
  view.set({ popover: null });
  return any;
};

// ---- the sidebar's two pages: the folder levels (places.js), and the editor beside them ----

const browsing = () => view.get().page === 'browse';
function bindPage() {
  effect(() => {
    if (!browsing()) {
      $('side').classList.add('editing');
      $('browser').inert = true; $('editpage').inert = false;
      return;
    }
    layout.showSide();
    $('side').classList.remove('editing');
    $('browser').inert = false; $('editpage').inert = true;
  });
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
  viewer.setBackdropNight(isDark ? 1 : 0); // the dark theme's backdrop is evening
}

// the backdrop: the place the tiles stand in.  A per-user choice, remembered in the browser (never in a document: it
// cannot change what prints), used for the map and for the tile editor alike; the plates keep the printer's bed.
const backdrop = signal('wilderness');
function bindBackdrop() {
  render(html`<${Backdrops} kinds=${BACKDROPS} current=${backdrop.value} onPick=${chooseBackdrop} />`, $('backdrops'));
  effect(() => {
    const kind = isPlatesView() || isInventory(tile()) ? 'plain' : backdrop.value;
    viewer.setBackdropKind(kind);
    render(html`<${Backdrops} kinds=${BACKDROPS} current=${backdrop.value} onPick=${chooseBackdrop} />`, $('backdrops'));
    $('backdrops').title = isPlatesView() ? 'The plates are always drawn on the printer’s bed' : '';
  });
}
function chooseBackdrop(kind) {
  backdrop.value = kind;
  try { localStorage.setItem('dt-backdrop', kind); } catch { /* private mode: the choice lasts until the page is closed */ }
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
// the panel: beside the view (one level at a time, or the source), or hidden; the title bar's one panel button (or \\) shows or hides it
const layout = new Layout($('app'), (st) => {
  $('panelbtn').setAttribute('aria-pressed', String(!st.hidePanel));
  $('panelbtn').classList.toggle('on', !st.hidePanel);
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
    let slot = $('about-version').querySelector('.copy-slot');
    if (!slot) { slot = document.createElement('span'); slot.className = 'copy-slot'; slot.style.display = 'contents'; $('about-version').append(slot); }
    render(html`<${CopyButton} text=${a.version} title="Copy the version" />`, slot);
    // Formats are major versions, written "0.x": any version starting with 0.
    $('about-formats').textContent = a.formats;
  } catch (e) {
    err.textContent = `Could not read the version: ${e.message}`;
    err.hidden = false;
    throw e;
  }
}
const showKeys = () => { closePopovers(); $('keys').showModal(); };
/** ⌘1/⌘2/⌘3: put the keyboard in the source, the view or the tile's levels (showing the panel first). */
function focusPane(n) {
  if (n === 2) { $('canvas').focus(); return; }
  layout.showSide();
  view.set({ pageAt: 'edit' });
  setFace(n === 1 ? 'source' : 'tile', { focus: true });
}

/** Wire this part of the page (once, at load). */
export function init() {
  try {
    const saved = JSON.parse(localStorage.getItem('dt-preview-recent-views'));
    if (Array.isArray(saved)) {
      const valid = saved.filter((name) => viewEntries.has(name) && !['painted', 'top'].includes(name));
      recentViews.value = [...new Set([...valid, 'low', 'close'])].slice(0, 2);
    }
  } catch { /* private mode or an older malformed value */ }
  try {
    const saved = localStorage.getItem('dt-backdrop');
    if (BACKDROPS.some(([k]) => k === saved)) backdrop.value = saved;
  } catch { /* private mode: the default stands */ }
  bindBackdrop();
  // the view buttons: the shortcuts (Iso, Top and the two used last) and the whole list in the display menu, the active one pressed
  effect(() => {
    const active = view.get().activeView;
    render(html`<${ViewButtons} names=${shortcutViews(recentViews.value)} entries=${viewEntries} active=${active} onPick=${pickView} />`, $('views'));
    render(html`<${ViewButtons} names=${VIEWS.map((v) => v[0])} entries=${viewEntries} active=${active} onPick=${pickView} />`, $('allviews'));
  });
  viewer.onUserViewChangeHook(() => view.set({ activeView: null }));
  view.subscribe(['pick'], ({ pick }) => applyPick(pick));
  bindPopovers();
  // the page the sidebar starts on: the tile's when the address names one (so it does not slide in at load), else the folder level
  view.set({ pageAt: /(^|[#&])tile=/.test(location.hash) ? 'edit' : 'browse' });
  bindPage();
  // the gate's legend names the angles the program checks against
  effect(() => {
    const { overhangDeg, steepestDeg } = server.get();
    $('lg-orange').textContent = `${overhangDeg}–${steepestDeg}°: allowed within budget`;
    $('lg-red').textContent = `Over ${steepestDeg}°: small patches allowed, more fails`;
  });
  toggle('t-overhang', (on) => viewer.setOverhang(on));
  $('t-paint').onchange = (e) => setPaint(e.target.checked);
  try { if (localStorage.getItem('dt-preview-paint') === '0') setPaint(false); } catch { /* private mode: the default stands */ }
  if (/paint=off/.test(location.hash)) { $('t-paint').checked = false; viewer.setPaint(false); } // #paint=off previews grey without saving
  toggle('t-wire', (on) => viewer.setWireframe(on));
  bindDebugLegend();
  viewer.setDebugColours(cssToken);
  toggle('t-debug', setDebug);
  toggle('t-grid', (on) => viewer.setGrid(on));
  toggle('t-fade', (on) => { viewer.setFade(on); try { localStorage.setItem('dt-fade', on ? '1' : '0'); } catch { /* private mode: the default stands */ } });
  try { if (localStorage.getItem('dt-fade') === '0') { $('t-fade').checked = false; viewer.setFade(false); } } catch { /* private mode: the default stands */ }
  for (const [btn, pop] of popovers) $(btn).onclick = (e) => { e.stopPropagation(); openPopover(pop); };
  // Choose from the library: the library's folder level
  $('choose').onclick = () => {
    const lib = server.get().roots.findIndex((r) => r.role === 'library');
    view.set({ placeAt: placeAt(lib >= 0 ? { root: lib, dir: '' } : { root: null, dir: '' }), query: '', pageAt: 'browse' });
  };
  // Up from the tile: the folder it is in (its level was set when it opened)
  $('back').onclick = () => { view.set({ pageAt: 'browse' }); };
  addEventListener('pointerdown', (e) => {
    if (popovers.some(([b, p]) => $(p).contains(e.target) || $(b).contains(e.target))) return;
    closePopovers();
  });
  $('theme').onclick = toggleTheme;
  dark.addEventListener('change', applyTheme);
  applyTheme();
  $('b-proj').onclick = () => setProjection(!viewer.isOrtho);
  try { if (localStorage.getItem('dt-preview-ortho') === '1') setProjection(true, false); } catch { /* private mode: the default stands */ }
  $('panelbtn').onclick = () => layout.toggle();
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
      if (closePopovers()) return;
      if (browsing() && tile() && !$('side').hidden) { view.set({ pageAt: 'edit' }); return; }
      if (browsing()) { $('bback').click(); return; }
      goUp(); // up a level of the panel (levels.js); at the tile there is nowhere to go
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
    if (e.key === '\\') layout.toggle();
    if (e.key === '?') { e.preventDefault(); showKeys(); }
  });
}
