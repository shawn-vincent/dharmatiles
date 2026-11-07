// The page's frame: the view's controls and display menu, popovers, the
// sidebar's two pages, light and dark, the projection, the panes and the
// keyboard.

import { Layout } from '../layout.js';
import { rememberView, shortcutViews } from '../view-presets.js';
import { VIEWS } from '../viewer.js';
import { $, state, tile, viewer } from './app.js';
import { renderHistory, texts, undo } from './document.js';
import { tools } from './map.js';
import { frameSel, setSel } from './selection.js';
import { renderList, startRename } from './tiles.js';

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

export function showActiveView(name) {
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
export function pickView(name, animate = true) {
  state.view = name;
  state.activeView = name;
  recentViews = rememberView(recentViews, name);
  try { localStorage.setItem('dt-preview-recent-views', JSON.stringify(recentViews)); } catch { /* private mode */ }
  renderViewShortcuts();
  viewer.setView(name, animate);
  showActiveView(name);
}
const toggle = (id, fn) => { $(id).onchange = (e) => fn(e.target.checked); };
// paint on or off (grey plastic): remembered in the browser
function setPaint(on) {
  $('t-paint').checked = on;
  viewer.setPaint(on);
  try { localStorage.setItem('dt-preview-paint', on ? '1' : '0'); } catch { /* ignore */ }
}

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
export const closePopovers = () => {
  let any = false;
  for (const [btn, pop] of popovers) { any ||= !$(pop).hidden; $(pop).hidden = true; $(btn).setAttribute('aria-expanded', false); }
  return any;
};

// ---- the sidebar's two pages: the tile browser, and the editor beside it ----

export const browsing = () => !$('side').classList.contains('editing');
export function showBrowser() {
  layout.showSide();
  $('side').classList.remove('editing');
  $('browser').inert = false; $('editpage').inert = true;
  const t = tile();
  if (t && t.root !== state.root && state.roots[t.root] && !$('filter').value) state.root = t.root;
  renderList();
  // scroll the list only: scrollIntoView would scroll the sliding sidebar too
  const ul = $('tiles'), li = ul.querySelector('li.sel');
  if (li) ul.scrollTop = li.offsetTop - ul.offsetTop - (ul.clientHeight - li.offsetHeight) / 2;
  $('filter').focus({ preventScroll: true });
}
export function showEditor() {
  if (!tile()) return;
  $('side').classList.add('editing');
  $('browser').inert = true; $('editpage').inert = false;
}

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

// perspective or orthographic: remembered across reloads (a per-viewer convenience)
function setProjection(ortho, animated = true) {
  viewer.setOrtho(ortho, animated);
  $('b-proj').firstChild.textContent = ortho ? 'Orthographic ' : 'Perspective ';
  try { localStorage.setItem('dt-preview-ortho', ortho ? '1' : '0'); } catch { /* private mode */ }
}
// the panes: Edit (text over properties), Inspect (side by side), Look (the view alone)
const layout = new Layout($('app'), (st) => {
  $('reopen').hidden = !st.hideText && !st.hideProps;
  $('show-text').hidden = !st.hideText;
  $('show-props').hidden = !st.hideProps;
  const preset = st.hideText && st.hideProps ? 'look' : st.hideText || st.hideProps ? null : st.mode;
  for (const b of $('layout-seg').children) b.classList.toggle('on', b.dataset.preset === preset);
  $('t-right').checked = st.side === 'right';
});
const showKeys = () => { closePopovers(); $('keys').showModal(); };
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
  viewer.onUserViewChange = () => { state.activeView = null; showActiveView(null); };
  toggle('t-overhang', (on) => viewer.setOverhang(on));
  $('t-paint').onchange = (e) => setPaint(e.target.checked);
  try { if (localStorage.getItem('dt-preview-paint') === '0') setPaint(false); } catch { /* ignore */ }
  if (/paint=off/.test(location.hash)) { $('t-paint').checked = false; viewer.setPaint(false); } // #paint=off previews grey without saving
  toggle('t-wire', (on) => viewer.setWireframe(on));
  toggle('t-grid', (on) => viewer.setGrid(on));
  toggle('t-fade', (on) => { viewer.setFade(on); try { localStorage.setItem('dt-fade', on ? '1' : '0'); } catch { /* ignore */ } });
  try { if (localStorage.getItem('dt-fade') === '0') { $('t-fade').checked = false; viewer.setFade(false); } } catch { /* ignore */ }
  for (const [btn, pop] of popovers) $(btn).onclick = (e) => { e.stopPropagation(); openPopover(pop); };
  $('choose').onclick = () => {
    const lib = state.roots.findIndex((r) => r.kind === 'library');
    if (lib >= 0) state.root = lib;
    showBrowser();
  };
  $('back').onclick = showBrowser;
  $('forward').onclick = showEditor;
  $('tilebtn').onclick = () => setSel({ tile: true });
  addEventListener('pointerdown', (e) => {
    if (popovers.some(([b, p]) => $(p).contains(e.target) || $(b).contains(e.target))) return;
    closePopovers();
  });
  $('theme').onclick = toggleTheme;
  dark.addEventListener('change', applyTheme);
  applyTheme();
  $('b-proj').onclick = () => setProjection(!viewer.isOrtho);
  try { if (localStorage.getItem('dt-preview-ortho') === '1') setProjection(true, false); } catch { /* ignore */ }
  $('hide-text').onclick = $('show-text').onclick = () => layout.toggle('text');
  $('hide-props').onclick = $('show-props').onclick = () => layout.toggle('props');
  for (const b of $('layout-seg').children) b.onclick = () => layout.preset(b.dataset.preset);
  $('t-right').onchange = (e) => layout.set({ side: e.target.checked ? 'right' : 'left' });
  $('b-keys').onclick = showKeys;
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
}
