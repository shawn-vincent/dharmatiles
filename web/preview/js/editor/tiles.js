// The tile menu: listing and filtering the tiles, a new tile, renaming one,
// and making a library tile your own.

import { failed, showError } from '../problems.js';
import { $, host, onMakeMine, toast } from './app.js';
import { abandonChanges, applyPatchOnCopy, canLeave, carryHistory, whenChecked } from './document.js';
import { copiedNote } from '../mine.js';
import { attachSizeGrid } from '../size-grid.js';
import { levelSnapshot, restoreLevel } from './levels.js';
import { select } from './open.js';
import { doc, inRoot, renameable, rootOf, server, stem, tile, view, whenListed } from './stores.js';

// ---- the tile menu -----------------------------------------------------

function dotClass(t) {
  const over = t.record && (t.record.audit.overhang_mm2 > 0 || t.record.audit.band_mm2 > 0);
  return `dot ${t.status}${over && t.status === 'ready' ? ' over' : ''}`;
}

// the switcher: one button per root (the personal home, the library, the test tiles, each folder named)
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
  const header = (text) => {
    const g = document.createElement('li'); g.className = 'group';
    g.textContent = text;
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
  const shown = all.filter(matches)
    .sort((x, y) => (x.group || '').localeCompare(y.group || '') || x.path.localeCompare(y.path));
  let last = null;
  for (const t of shown) {
    const g = t.group;
    if (g && g !== last) { header(g); last = g; }
    ul.append(row(t));
  }
}

// A first tile: valid, printable ground in My tiles, ready for the same
// text/lens/direct edits as every other tile.
function openNewTile() {
  view.set({ popover: null });
  const dlg = $('new-dialog');
  $('new-note').textContent = 'Starts as printable flat ground. Everything about it is in its source, which you can change.';
  dlg.showModal();
  $('new-size').value = '1x1';
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
      failed(`Could not rename ${stem(t)}`, r);
      return;
    }
    input.remove(); $('tilebtn').hidden = false;
    doc.set({ note: null });
    carryHistory();
    const id = r.body.id, here = levelSnapshot();
    if (await whenListed(id)) { await select(id, true); restoreLevel(here); }
  };
  input.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') { e.preventDefault(); finish(true); }
    if (e.key === 'Escape') { e.preventDefault(); finish(false); }
  };
  input.oninput = () => { input.classList.remove('bad'); input.title = ''; };
  input.onblur = () => finish(true);
}

// ---- the first change to a library tile --------------------------------------
// A library tile is read only until it is first changed.  That change copies it
// into My tiles and is made there; one line over the view says so, with an Undo
// that goes back to the library tile and deletes the copy.

let minting = false;

/** The copy is deleted and the library tile is shown again (the toast's Undo). */
async function undoMine(copy, lib) {
  abandonChanges();
  await select(lib.id, true);
  if (!(await whenChecked(lib.id))) { showError('Could not go back to the library tile', new Error(`${lib.name} did not open again`)); return; }
  const r = await host.deleteTile(copy.id);
  if (r.status !== 200 && r.status !== 0) {
    // a refusal too: the copy was made by this page a moment ago, so the program must take it away
    showError(`Could not delete the copy ${copy.name}`, new Error(`Could not delete the copy ${copy.name}`, { cause: r.error }));
  }
}

/**
 * Copy the library tile shown into My tiles, open the copy, make the change `replay` describes there
 * ({patch}: a typed change over the library tile's text; {run}: a function that writes at the
 * positions of the library tile's text, called only when the copy's text is the same) and say so.
 * False when this tile is not one that is copied (it says why instead).
 */
async function mineIt(replay) {
  const t = tile(), d = doc.get();
  if (!t) return false;
  if (!t.path.startsWith('lib:') || d.file !== t.path) {
    doc.set({ note: d.file !== t.path ? 'this part is read only: change the tile’s own text' : 'this file is read only' });
    return false;
  }
  if (minting) return false;
  minting = true;
  const lib = { id: t.id, name: stem(t), text: d.text };
  try {
    const r = await host.mine(t.id);
    if (r.status !== 200 || r.body?.id == null) {
      doc.set({ note: r.body?.error || 'could not make a copy' });
      failed(`Could not copy ${lib.name} into My tiles`, r);
      return false;
    }
    const copy = { id: r.body.id, name: stem({ path: r.body.path }) };
    if (!(await whenListed(copy.id))) { showError('The copy did not appear', new Error(`${copy.name} was made but is not in the list`)); return false; }
    const here = levelSnapshot(); // the copy opens on the same level (its entries have the same keys)
    await select(copy.id, true);
    restoreLevel(here);
    if (!(await whenChecked(copy.id))) { showError('The copy did not open', new Error(`${copy.name} was made but did not open`)); return false; }
    let applied = false;
    if (replay?.patch) applied = applyPatchOnCopy(replay.patch, lib.text);
    else if (replay?.run && doc.get().text === lib.text) { await replay.run(); applied = true; }
    toast(copiedNote(copy.name, replay ? applied : null), () => undoMine(copy, lib), { ms: 10000 });
    return true;
  } finally {
    minting = false;
  }
}

/** Wire this part of the page (once, at load). */
export function init() {
  server.subscribe(['tiles', 'roots'], () => { if (view.get().page === 'browse') renderList(); });
  view.subscribe(['tileId', 'root', 'page'], () => { if (view.get().page === 'browse') renderList(); });
  onMakeMine(mineIt);
  $('filter').oninput = renderList;
  $('filter').onkeydown = (e) => {
    if (e.key === 'Escape' && tile()) { e.preventDefault(); e.stopPropagation(); view.set({ page: 'edit' }); return; }
    if (e.key !== 'Enter') return;
    const first = $('tiles').querySelector('li:not(.group):not(.note)');
    first?.click();
  };
  $('new').onclick = openNewTile;
  $('empty-new').onclick = openNewTile;
  attachSizeGrid($('new-size'), { parent: $('new-dialog') });
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
    } catch (e) {
      ok.disabled = false;
      $('new-note').textContent = 'The tile could not be made.';
      showError('Could not make the tile', e);
      return;
    }
    ok.disabled = false;
    if (r.status !== 200 || r.body?.id == null) {
      $('new-note').textContent = r.body?.error || 'The tile could not be made.';
      failed('Could not make the tile', r);
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
}
