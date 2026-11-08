// The tile menu: listing and filtering the tiles, a new tile, renaming one,
// and making a library tile your own.

import { $, host } from './app.js';
import { canLeave, carryHistory } from './document.js';
import { select } from './open.js';
import { doc, inRoot, renameable, rootOf, server, stem, tile, view, whenListed } from './stores.js';

// ---- the tile menu -----------------------------------------------------

function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}
let testsOpen = false; // the library's "Test tiles" starts collapsed

// the switcher: one button per root (the personal home, the library, each folder named)
function renderRoots() {
  const nav = $('roots'), { roots } = server.get(), current = view.get().root;
  nav.replaceChildren();
  nav.hidden = roots.length < 2;
  roots.forEach((r, i) => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'root' + (i === current ? ' on' : '');
    b.setAttribute('aria-pressed', i === current);
    b.title = r.path || r.label;
    b.innerHTML = '<span></span> <span class="n"></span>';
    b.firstChild.textContent = r.label;
    b.lastChild.textContent = inRoot(i).length;
    b.onclick = () => view.set({ root: i });
    nav.append(b);
  });
}

function renderList() {
  renderRoots();
  const { tileId, root: current } = view.get();
  const q = $('filter').value.trim().toLowerCase();
  const ul = $('tiles');
  ul.replaceChildren();
  const matches = (t) => !q || stem(t).toLowerCase().includes(q) || t.path.toLowerCase().includes(q);
  const row = (t) => {
    const li = document.createElement('li');
    li.className = t.id === tileId ? 'sel' : '';
    li.tabIndex = 0; li.setAttribute('role', 'option'); li.setAttribute('aria-selected', t.id === tileId);
    li.title = t.path;
    li.innerHTML = '<span></span><span class="name"></span>';
    li.firstChild.className = dotClass(t);
    li.querySelector('.name').textContent = stem(t);
    li.onclick = async () => {
      if (t.id === view.get().tileId) { view.set({ page: 'edit' }); return; }
      if ((await select(t.id)) !== false) view.set({ page: 'edit' });
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
  const root = rootOf(current);
  const all = inRoot(current);
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
    const open = testsOpen || !!q || tests.some((t) => t.id === tileId);
    header(`Test tiles (${tests.length})`, () => { testsOpen = !open; renderList(); }, open);
    if (open) for (const t of tests) ul.append(row(t));
  }
}

// A first tile: valid, printable ground in My tiles, ready for the same
// text/lens/direct edits as every other tile.
function openNewTile() {
  view.set({ popover: null });
  const dlg = $('new-dialog');
  $('new-note').textContent = 'Starts as printable flat ground. Everything about it is ordinary text you can change.';
  dlg.showModal();
  $('new-name').focus();
  $('new-name').select();
}

// ---- renaming: a tile's file name is its name ---------------------------

export function startRename() {
  const t = tile();
  if (!renameable(t) || $('rename')) return;
  view.set({ popover: null });
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
    if (!commit || !name || name === stem(t)) { done = true; input.remove(); $('tilebtn').hidden = false; doc.set({ note: null }); return; }
    done = true;
    input.disabled = true;
    if (!(await canLeave())) { input.remove(); $('tilebtn').hidden = false; return; }
    const r = await host.rename(t.id, name);
    if (r.status !== 200 || r.body?.id == null) {
      // stay in the field with the reason, so the name can be fixed
      done = false; input.disabled = false; input.classList.add('bad');
      input.title = r.body?.error || 'The tile could not be renamed.';
      doc.set({ note: input.title }); input.focus();
      return;
    }
    input.remove(); $('tilebtn').hidden = false;
    doc.set({ note: null });
    carryHistory();
    const id = r.body.id;
    if (await whenListed(id)) await select(id, true);
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

/** The make-it-mine question follows `view.mineOpen`; only a library tile has one. */
function showMine() {
  const open = view.get().mineOpen, t = tile();
  if (open && !t?.path.startsWith('lib:')) { view.set({ mineOpen: false }); return; }
  $('mine').hidden = !open;
  if (open) $('mine-ok').focus();
}

/** Wire this part of the page (once, at load). */
export function init() {
  server.subscribe(['tiles', 'roots'], () => { if (view.get().page === 'browse') renderList(); });
  view.subscribe(['tileId', 'root', 'page'], () => { if (view.get().page === 'browse') renderList(); });
  view.subscribe(['mineOpen'], showMine);
  $('filter').oninput = renderList;
  $('filter').onkeydown = (e) => {
    if (e.key === 'Escape' && tile()) { e.preventDefault(); e.stopPropagation(); view.set({ page: 'edit' }); return; }
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
      r = await host.newTile($('new-name').value, $('new-size').value);
    } catch {
      r = { status: 0, body: { error: 'the editor is disconnected' } };
    }
    ok.disabled = false;
    if (r.status !== 200 || r.body?.id == null) {
      $('new-note').textContent = r.body?.error || 'The tile could not be made.';
      return;
    }
    $('new-dialog').close();
    if (await whenListed(r.body.id)) select(r.body.id, true);
  };
  $('tilebtn').ondblclick = (e) => { e.preventDefault(); startRename(); };
  addEventListener('keydown', (e) => {
    const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable;
    if (e.key === 'F2' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) { e.preventDefault(); startRename(); }
  });
  $('mine-cancel').onclick = () => view.set({ mineOpen: false });
  $('mine-ok').onclick = async () => {
    const t = tile();
    $('mine-ok').disabled = true;
    const r = await host.mine(t.id);
    $('mine-ok').disabled = false;
    view.set({ mineOpen: false });
    if (r.status !== 200 || r.body?.id == null) { doc.set({ note: r.body?.error || 'could not make a copy' }); return; }
    // the copy joins the list with the next event
    if (await whenListed(r.body.id)) select(r.body.id);
  };
}
