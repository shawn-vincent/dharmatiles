// @ts-check
// What is done to a tile from the page: a new tile (the size picker), renaming one, and
// making a library tile your own.  The list of tiles, and the folders they are in, are the
// panel's folder levels (places.js).

import { failed, showError } from '../problems.js';
import { $, host, onMakeMine, toast } from './app.js';
import { abandonChanges, applyPatchOnCopy, canLeave, carryHistory, whenChecked } from './document.js';
import { copiedNote } from '../mine.js';
import { attachSizeGrid } from '../view/size-grid.js';
import { levelSnapshot, restoreLevel } from './levels.js';
import { select } from './open.js';
import { doc, renameable, server, stem, tile, view, whenListed } from './stores.js';

// A first tile: valid, printable ground in My tiles (or in the folder the browser is at, `into`:
// {root, dir, name}), ready for the same text/lens/direct edits as every other tile.
let makingIn = null;
export function openNewTile(into = null) {
  view.set({ popover: null });
  makingIn = into && into.root != null ? { root: into.root, dir: into.dir || '' } : null;
  const dlg = $('new-dialog');
  $('new-where').textContent = into?.name || 'My tiles';
  $('new-note').textContent = 'Starts as printable flat ground. Everything about it is in its source, which you can change.';
  dlg.showModal();
  $('new-size').value = '1x1';
  $('new-name').focus();
  $('new-name').select();
}

// ---- renaming: a tile's file name is its name ---------------------------

/**
 * Rename tile `t`'s file (the open one is saved or kept first, keeps its history and its level, and is
 * opened again under its new name): {ok: true, id}, or {ok: false} with `r` the reply that refused it
 * (`kept` when unsaved changes were kept instead).
 */
export async function renameTile(t, name) {
  const open = t.id === view.get().tileId;
  if (open && !(await canLeave())) return { ok: false, kept: true };
  const editing = view.get().page === 'edit';
  const r = await host.rename(t.id, name);
  if (r.status !== 200 || r.body?.id == null) return { ok: false, r };
  if (!open) return { ok: true, id: r.body.id };
  carryHistory();
  const id = r.body.id, here = levelSnapshot();
  // the old name leaves the list as the new one joins: for a moment no tile is shown, and the panel went to its folder
  if (await whenListed(id)) { await select(id, true); restoreLevel(here); if (editing) view.set({ page: 'edit' }); }
  return { ok: true, id };
}

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
    const out = await renameTile(t, name);
    if (out.kept) { input.remove(); $('tilebtn').hidden = false; return; }
    if (!out.ok) {
      // stay in the field with the reason, so the name can be fixed
      const r = out.r;
      done = false; input.disabled = false; input.classList.add('bad');
      input.title = r.body?.error || 'The tile could not be renamed.';
      doc.set({ note: input.title }); input.focus();
      failed(`Could not rename ${stem(t)}`, r);
      return;
    }
    input.remove(); $('tilebtn').hidden = false;
    doc.set({ note: null });
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
  onMakeMine(mineIt);
  // the empty page's New tile: into the folder the browser is at, when it is one of yours
  $('empty-new').onclick = () => {
    const l = view.get().place, r = server.get().roots[l.root];
    openNewTile(r?.writable ? { root: l.root, dir: l.dir, name: [r.label, ...(l.dir ? l.dir.split('/') : [])].join(' › ') } : null);
  };
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
      r = await host.newTile($('new-name').value, $('new-size').value, makingIn);
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
    if (await whenListed(r.body.id) && (await select(r.body.id, true)) !== false) view.set({ page: 'edit' });
  };
  $('tilebtn').ondblclick = (e) => { e.preventDefault(); startRename(); };
  addEventListener('keydown', (e) => {
    const at = /** @type {HTMLElement} */ (e.target);
    const typing = at.tagName === 'INPUT' || at.tagName === 'TEXTAREA' || at.isContentEditable;
    if (e.key === 'F2' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) { e.preventDefault(); startRename(); }
  });
}
