// The tile menu: listing and filtering the tiles, a new tile, renaming one,
// and making a library tile your own.

import { $, host, state, tile } from './app.js';
import { closePopovers, showEditor } from './chrome.js';
import { canLeave, carryHistory, doc, renderTextBar } from './document.js';
import { whenTile } from './live.js';
import { select } from './open.js';

// ---- the tile menu -----------------------------------------------------

function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}
// the file stem (`2x1-cliff`, `1x1-water+grass-tree`): the menu's name for a tile, whatever the document calls it
export const stem = (t) => t.path.replace(/^lib:/, '').split(/[\\/]/).pop().replace(/\.(dtile|stl)$/i, '') + (/\.stl$/i.test(t.path) ? '.stl' : '');
export const inRoot = (r) => state.tiles.filter((t) => t.root === r && !t.gone);
export const rootOf = (r) => state.roots[r] || { kind: 'files', label: 'Files', path: null };
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

export function renderList() {
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

// ---- renaming: a tile's file name is its name ---------------------------

export const renameable = (t) => !!t && !t.path.startsWith('lib:') && /\.dtile$/i.test(t.path) && rootOf(t.root).kind === 'dir';
export function startRename() {
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
    carryHistory();
    const i = r.body.index;
    if (await whenTile(i)) await select(i, true);
  };
  input.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') { e.preventDefault(); finish(true); }
    if (e.key === 'Escape') { e.preventDefault(); finish(false); }
  };
  input.oninput = () => { input.classList.remove('bad'); input.title = ''; };
  input.onblur = () => finish(true);
}

// ---- make it mine -----------------------------------------------------------

export function offerMine() {
  const t = tile();
  if (!t || !t.path.startsWith('lib:')) return;
  $('mine').hidden = false;
  $('mine-ok').focus();
}

/** Wire this part of the page (once, at load). */
export function init() {
  $('filter').oninput = renderList;
  $('filter').onkeydown = (e) => {
    if (e.key === 'Escape' && tile()) { e.preventDefault(); e.stopPropagation(); showEditor(); return; }
    if (e.key !== 'Enter') return;
    const first = $('tiles').querySelector('li:not(.group):not(.note)');
    first?.click();
  };
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
    if (await whenTile(i)) select(i, true);
  };
  $('tilebtn').ondblclick = (e) => { e.preventDefault(); closePopovers(); startRename(); };
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
    if (await whenTile(i)) select(i);
  };
}
