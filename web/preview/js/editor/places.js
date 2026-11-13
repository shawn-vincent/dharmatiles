// @ts-check
// The folder levels of the panel (docs/preview.html, "Places and folders"): the levels above the
// tile.  Places at the top (My tiles, folders you added, the library, the tests), a place opens to
// a folder level (its subfolders, then its tiles), a subfolder is another level.  Where you are
// is `view.place` ({root, dir}; root null is Places) and shows while the sidebar is on its browser
// page; a tile opened from it shows the editor page, whose crumbs begin with the same path.
//
// This part draws the level (rows: js/view/places-panel.js; what a level lists: js/folders.js), slides
// between levels as the tile's levels do, keeps the address (`#place=…&dir=…`), and does what the
// rows do: make a tile or a folder, rename, move (a drag onto a folder row or a crumb, or the row's
// menu), copy, trash with an Undo, add a folder with the program's own chooser.  Every one is a call
// to the program, which checks it again; the page never sends a path, only a place's number, a folder's
// path below it and a tile's id.  The list is updated from the program's answer, which carries the list.

import { effect } from '@preact/signals-core';
import { fillCrumbs, keepEnd, pinEnd } from '../crumbs.js';
import { html, render } from '../view/html.js';
import { ContextLines, FolderActions, Levels } from '../view/places-panel.js';
import {
  canDrop, contextForFolder, levelOf, levelOfParams, moveTargets, parentLevel, placeParams, placesOf, renamedDir, stemOf, whereIs, writable,
} from '../folders.js';
import { openMenu } from '../menu.js';
import { buildHash, crumbsFor, moveRow, parseHash } from '../nav.js';
import { copiedNote } from '../mine.js';
import { systemName } from '../inventory.js';
import { failed, whyOf } from '../problems.js';
import { $, host, toast } from './app.js';
import { canLeave } from './document.js';
import { applySnapshot } from './live.js';
import { levelSnapshot, restoreLevel } from './levels.js';
import { select } from './open.js';
import { nameField } from './rows.js';
import { openNewTile, renameTile } from './tiles.js';
import { listMenu, newList } from './lists.js';
import { browse } from './model.js';
import { placeAt, server, tile, tileById, view, whenListedWhere } from './stores.js';

const SLIDE_MS = 200;
const EASE = 'cubic-bezier(.2, .7, .2, 1)';
const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

const TOP = { root: null, dir: '' };
const levelNow = () => view.get().place || TOP;
const depthOfLevel = (l) => (l.root == null ? 0 : 1 + (l.dir ? l.dir.split('/').length : 0));
const keyOf = (l) => `${l.root ?? 'places'}|${l.dir || ''}`;
const browsing = () => view.get().page === 'browse';

const scrolls = new Map(); // a level's key -> how far its list was scrolled
let started = false;
let leftFrom = null;       // the folder or place the last Up left, so the keyboard goes back to its row

// ---- what the program says of a refusal ----------------------------------------------------------

/** A reply that was not a success, shown where it was asked for: the program's words in a toast (an internal failure goes to the error panel). */
function refusal(what, r) {
  if (!failed(what, r)) toast(`${what}: ${whyOf(r)}`, null, { ms: 9000 });
}

/** The program's answer to a call that changed the list: its list is taken at once. */
const take = (r) => { if (r.body?.snapshot) applySnapshot(r.body.snapshot); };

// ---- going places ---------------------------------------------------------------------------------

/** Show the folder level `next` (Places for null), on the browser page. */
export function goTo(next, { focus = false } = {}) {
  const to = next || TOP;
  view.set({ placeAt: placeAt(to), query: '', pageAt: 'browse' });
  if (focus) requestAnimationFrame(() => focusRow());
}

/** One level up: a folder to its parent, a place to Places; false at Places. */
export function goUpFolder() {
  const up = parentLevel(levelNow());
  if (!up) return false;
  const l = levelNow();
  leftFrom = l.root == null ? null : { kind: l.dir ? 'folder' : 'place', id: l.dir || String(l.root) };
  goTo(up, { focus: !!$('browser').contains(document.activeElement) });
  return true;
}

function focusRow(match = null) {
  const list = $('folder')?.querySelector('.flist');
  if (!list) return;
  const all = [...list.querySelectorAll('.orow')];
  const row = (match && all.find((r) => r.dataset.kind === match.kind && r.dataset.id === match.id)) || all.find((r) => r.parentElement.classList.contains('sel')) || all[0];
  (row || $('folder')).focus({ preventScroll: true });
}

// ---- the address -------------------------------------------------------------------------------------

let lastWritten = null;
/** Write the folder level into the URL while the browser page shows: a new level is a step of the browser's history. */
function syncAddress() {
  if (!started || !view.get().addressRead || !browsing() || !server.get().ready) return;
  const { roots } = server.get();
  const cur = parseHash(location.hash);
  const next = { ...cur };
  for (const k of ['tile', 'at', 'piece', 'control', 'group', 'face', 'place', 'dir']) delete next[k];
  Object.assign(next, placeParams(roots, levelNow()));
  const h = buildHash(next);
  if (h === location.hash) { lastWritten = keyOf(levelNow()); return; }
  // a step when the level changed (or the tile's address was showing), else the same step again
  const push = lastWritten == null ? cur.tile != null : lastWritten !== keyOf(levelNow()) || cur.tile != null;
  lastWritten = keyOf(levelNow());
  if (push) history.pushState(null, '', h); else history.replaceState(null, '', h);
}

/** The browser's back or forward onto a folder level's address. */
function fromAddress() {
  const h = parseHash(location.hash);
  const lv = levelOfParams(server.get().roots, h);
  if (!lv) return;
  if (lv.missing) { toast('That place is not on the list now'); goTo(TOP); return; }
  lastWritten = keyOf(lv);
  view.set({ placeAt: placeAt(lv), query: '', pageAt: 'browse' });
}

// ---- drawing ---------------------------------------------------------------------------------------------

const dataNow = () => browse.data.value;

/** The buttons above the list: what can be added at the level. */
function actionButtons(d) {
  const mk = (label, title, run, cls = '') => ({ label, title, run, cls });
  const buttons = [];
  if (d.level.root == null) buttons.push(mk('Add a folder…', 'Choose a folder of tiles on this computer to list here', addPlace));
  else if (d.writable) {
    buttons.push(mk('New tile', 'A new tile in this folder', () => openNewTile({ root: d.level.root, dir: d.level.dir, name: placeName(d) })));
    buttons.push(mk('New folder', 'A new folder in this folder', (b) => newFolder(b)));
    buttons.push(mk('New map', 'A new map (.dmap) in this folder: tiles placed on a grid, to see the scene before anything prints', (b) => newList(d.level.root, d.level.dir, b, placeName(d), true)));
    buttons.push(mk('New print list', 'A new print list (.dmap) in this folder: the tiles to print, laid out for a printer', (b) => newList(d.level.root, d.level.dir, b, placeName(d))));
  }
  return buttons;
}

const placeName = (d) => [d.roots[d.level.root]?.label, ...(d.level.dir ? d.level.dir.split('/') : [])].join(' › ');

const listHandlers = {
  place: (i) => goTo({ root: i, dir: '' }, { focus: !!$('browser').contains(document.activeElement) }),
  folder: (dir) => goTo({ root: levelNow().root, dir }, { focus: !!$('browser').contains(document.activeElement) }),
  tile: (id) => openTile(id),
  menu: (kind, id, anchor) => openRowMenu(kind, id, anchor),
  act: (action, kind, id, anchor) => rowAction(action, kind, id, anchor),
  canDrop: (id, target) => canDrop(server.get().roots, tileById(id), target),
  drop: (id, target) => { const t = tileById(id); if (t) moveTile(t, target); },
};

/** Draw what the browser page shows from the model: the level, the crumbs, the strip, the buttons; all of it follows the level and the program's list. */
function bind() {
  // the list, sliding between levels
  render(html`<${BrowserLevels} />`, $('folder-vp'));
  effect(() => {
    if (!browse.ready.value) return;
    const d = browse.data.value;
    const r = d.roots[d.level.root];
    // Places has nothing to filter
    $('filter').hidden = d.level.root == null;
    if ($('filter').value !== d.query) $('filter').value = d.query;
    const crumbs = crumbsFor({ place: r ? { root: d.level.root, label: r.label, dir: d.level.dir } : null });
    fillCrumbs($('bcrumbs'), crumbs, (c) => goTo(c.place));
    pinEnd($('bcrumb-scroll'));
    $('bback').disabled = d.level.root == null;
    $('bback').title = d.level.root == null ? 'Places is the top' : 'Up a level (⌘[)';
    const lines = contextForFolder({ roots: d.roots, level: d.level, view: d.view, home: d.home });
    // the pills are for one system, the title bar's chip: said once, where they are
    if (d.level.root != null && d.inventory?.exists && d.view?.items.length && lines.length) lines[0] = `${lines[0]} · have and need for ${systemName(d.system)}`;
    render(html`<${ContextLines} lines=${lines} />`, $('bctx-lines'));
    render(html`<${FolderActions} buttons=${actionButtons(d)} />`, $('folder-actions'));
  });
  // the address follows the level (a new level is a step of the browser's history)
  effect(() => { if (browse.ready.value && view.get().page === 'browse') { void view.get().place; syncAddress(); } });
}

/** The level in the viewport: the component reads the model itself, so it is drawn again by itself. */
function BrowserLevels() {
  if (!browse.ready.value) return null;
  // once a level has come in, the keyboard goes to the row it came from (or the first), if it was in the list
  const arrived = () => {
    if (!$('browser').contains(document.activeElement)) return;
    const goBack = leftFrom;
    leftFrom = null;
    setTimeout(() => focusRow(goBack), reduced() ? 0 : SLIDE_MS + 20);
  };
  return html`<${Levels} d=${browse.data.value} on=${listHandlers} scrolls=${scrolls} arrived=${arrived} />`;
}

// ---- opening a tile -------------------------------------------------------------------------------------------

async function openTile(id) {
  if (id === view.get().tileId) { view.set({ pageAt: 'edit' }); return; }
  if ((await select(id)) !== false) view.set({ pageAt: 'edit' });
}

// ---- the actions ------------------------------------------------------------------------------------------------

const folderOpenIn = (root, dir) => {
  const t = tile();
  const at = t && whereIs(server.get().roots, t);
  return !!at && at.root === root && (at.dir === dir || at.dir.startsWith(`${dir}/`));
};

/**
 * After the open tile's file moved: the tile at its new path is opened again, on the level it was at (and the
 * page it was on: its old path left the list as the new one joined, so for a moment no tile was shown).
 */
async function followOpen(path, here, editing) {
  const t = await whenListedWhere((x) => x.path === path);
  if (!t) return;
  await select(t.id, true);
  restoreLevel(here);
  if (editing) view.set({ pageAt: 'edit' });
}

/** Rename tile `t` (the open one keeps its level). */
async function renameFromList(t, name) {
  const r = await renameTile(t, name);
  if (!r.ok && r.r) refusal(`Could not rename ${stemOf(t.path)}`, r.r);
  return r.ok;
}

async function duplicate(t) {
  const r = await host.duplicateTile(t.id);
  if (r.status !== 200) { refusal(`Could not copy ${stemOf(t.path)}`, r); return; }
  take(r);
  const name = stemOf(r.body.path);
  toast(`Made ${name}`, () => host.deleteTile(r.body.id).then((d) => { if (d.status !== 200) refusal(`Could not take ${name} away again`, d); }), { label: 'Undo' });
}

/** Move tile `t` into `target` ({root, dir}); the Undo moves it back. */
async function moveTile(t, target) {
  const roots = server.get().roots;
  const from = whereIs(roots, t);
  const check = canDrop(roots, t, target);
  if (!check.ok) { toast(`${stemOf(t.path)} stays: ${check.why}`); return; }
  const open = t.id === view.get().tileId;
  if (open && !(await canLeave())) return;
  const here = open ? levelSnapshot() : null;
  const editing = view.get().page === 'edit';
  const r = await host.moveTile(t.id, target);
  if (r.status !== 200) { refusal(`Could not move ${stemOf(t.path)}`, r); return; }
  take(r);
  if (open) await followOpen(r.body.path, here, editing);
  const to = roots[target.root];
  const where = target.dir ? target.dir.split('/').pop() : to?.label;
  toast(`Moved ${stemOf(t.path)} into ${where}`, () => undoMove(r.body.id, from, stemOf(t.path)), { ms: 8000 });
}

async function undoMove(id, from, name) {
  const open = id === view.get().tileId;
  const here = open ? levelSnapshot() : null;
  const editing = view.get().page === 'edit';
  const r = await host.moveTile(id, from);
  if (r.status !== 200) { refusal(`Could not move ${name} back`, r); return; }
  take(r);
  if (open) await followOpen(r.body.path, here, editing);
}

/** To the Trash, never a delete; the toast's Undo puts it back. */
async function trashTile(t) {
  const open = t.id === view.get().tileId;
  if (open && !(await canLeave())) return;
  const name = stemOf(t.path);
  const r = await host.trashTile(t.id);
  if (r.status !== 200) { refusal(`Could not move ${name} to the Trash`, r); return; }
  take(r);
  toast(`Moved ${name} to the Trash`, () => undoTrash(r.body.undo, name, open), { ms: 9000 });
}

async function undoTrash(n, name, reopen) {
  const r = await host.undoTrash(n);
  if (r.status !== 200) { refusal(`Could not put ${name} back`, r); return; }
  take(r);
  if (reopen && r.body.id && (await whenListedWhere((x) => x.id === r.body.id))) { await select(r.body.id, true); view.set({ pageAt: 'edit' }); }
}

async function trashFolder(root, dir) {
  if (folderOpenIn(root, dir) && !(await canLeave())) return;
  const name = dir.split('/').pop();
  const r = await host.trashFolder(root, dir);
  if (r.status !== 200) { refusal(`Could not move ${name} to the Trash`, r); return; }
  take(r);
  toast(`Moved the folder ${name} to the Trash`, () => undoTrash(r.body.undo, name, false), { ms: 9000 });
}

async function renameFolder(root, dir, name) {
  const inside = folderOpenIn(root, dir);
  if (inside && !(await canLeave())) return;
  const open = tile();
  const here = inside ? levelSnapshot() : null;
  const editing = view.get().page === 'edit';
  const roots = server.get().roots;
  const oldPrefix = `${roots[root].path}/${dir}/`;
  const r = await host.renameFolder(root, dir, name);
  if (r.status !== 200) { refusal(`Could not rename ${dir.split('/').pop()}`, r); return; }
  take(r);
  const nowDir = r.body.dir;
  const l = levelNow();
  if (l.root === root) view.set({ placeAt: placeAt({ root, dir: renamedDir(l.dir, dir, nowDir) }) });
  if (inside && open) await followOpen(`${roots[root].path}/${nowDir}/${open.path.slice(oldPrefix.length)}`, here, editing);
}

async function newFolder(anchor) {
  const l = levelNow();
  const have = new Set((dataNow().view?.folders || []).map((f) => f.name.toLowerCase()));
  nameField(anchor, {
    title: 'New folder',
    value: 'New folder',
    hint: `Made in ${placeName(dataNow())}.`,
    flaw: (v) => {
      const s = v.trim();
      if (!s) return 'A name is needed';
      if (s.length > 80) return 'At most 80 characters';
      return have.has(s.toLowerCase()) ? `A folder called ${s} is already here` : null;
    },
    save: async (v) => {
      const r = await host.newFolder(l.root, l.dir, v);
      if (r.status !== 200) { refusal('Could not make the folder', r); return; }
      take(r);
      goTo({ root: l.root, dir: r.body.dir }, { focus: true });
    },
  });
}

/** Add a folder with the program's own chooser; a cancel does nothing. */
async function addPlace(anchor) {
  anchor.setAttribute('aria-busy', 'true');
  let r;
  try { r = await host.addPlace(); } finally { anchor.removeAttribute('aria-busy'); }
  if (r.status !== 200) { refusal('Could not add the folder', r); return; }
  // the user closed the chooser: nothing was asked for, so nothing is said
  if ('cancelled' in r.body) return;
  take(r);
  goTo({ root: r.body.root, dir: '' });
  toast(`Added ${r.body.label} to Places. The folder itself is untouched.`, null, { ms: 6000 });
}

async function removePlace(i) {
  const r0 = server.get().roots[i];
  if (!r0) return;
  const r = await host.removePlace(i);
  if (r.status !== 200) { refusal(`Could not take ${r0.label} off the list`, r); return; }
  // the places after it are numbered one less now
  const l = levelNow();
  if (l.root === i) view.set({ placeAt: placeAt(TOP) });
  else if (l.root != null && l.root > i) view.set({ placeAt: placeAt({ root: l.root - 1, dir: l.dir }) });
  take(r);
  toast(`Took ${r0.label} off the list. The folder and its tiles are still on disk.`, null, { ms: 7000 });
}

/** A library tile is copied into My tiles: the row's way to make it yours without opening it. */
async function copyToMine(t) {
  const r = await host.mine(t.id);
  if (r.status !== 200 || r.body?.id == null) { refusal(`Could not copy ${stemOf(t.path)} into My tiles`, r); return; }
  const name = stemOf(r.body.path);
  toast(copiedNote(name, null), () => host.deleteTile(r.body.id).then((d) => { if (d.status !== 200) refusal(`Could not take the copy ${name} away`, d); }), { ms: 8000 });
}

// ---- row menus and keys ----------------------------------------------------------------------------------------------

function nameFolderRow(root, dir, anchor) {
  const cur = dir.split('/').pop();
  const parent = dir.includes('/') ? dir.slice(0, dir.lastIndexOf('/')) : '';
  const siblings = new Set((levelOf({ ...server.get(), root, dir: parent })?.folders || []).map((f) => f.name.toLowerCase()));
  nameField(anchor, {
    title: 'Rename this folder',
    value: cur,
    hint: 'The tiles in it keep their names; the folder is renamed on disk.',
    flaw: (v) => {
      const s = v.trim();
      if (!s) return 'A name is needed';
      if (s.length > 80) return 'At most 80 characters';
      return s.toLowerCase() !== cur.toLowerCase() && siblings.has(s.toLowerCase()) ? `A folder called ${s} is already here` : null;
    },
    save: (v) => { if (v !== cur) renameFolder(root, dir, v); },
  });
}

function nameTileRow(t, anchor) {
  const cur = stemOf(t.path);
  nameField(anchor, {
    title: 'Rename this tile',
    value: cur,
    hint: 'A tile’s file name is its name. Nothing else changes.',
    flaw: (v) => (v.trim() ? (v.trim().length > 80 ? 'At most 80 characters' : null) : 'A name is needed'),
    save: (v) => { if (v !== cur) renameFromList(t, v); },
  });
}

function moveMenu(t, anchor) {
  const { roots, folders } = server.get();
  const targets = moveTargets(roots, folders, t);
  const items = targets.length
    ? targets.map((x) => ({ label: x.label, sub: x.dir ? x.place : 'the top of the place', run: () => moveTile(t, x) }))
    : [{ label: 'No other folder', sub: 'make one with New folder', disabled: true }];
  openMenu(anchor, [{ head: `Move ${stemOf(t.path)} to` }, ...items], { label: `Move ${stemOf(t.path)} to`, cls: 'addmenu', align: 'right' });
}

function openRowMenu(kind, id, anchor) {
  const { roots } = server.get();
  const l = levelNow();
  if (kind === 'place') {
    const r = roots[Number(id)];
    if (r?.role !== 'added') return;
    openMenu(anchor, [{ label: 'Take off the list', sub: 'the folder and its tiles stay on disk', danger: true, run: () => removePlace(Number(id)) }], { label: r.label });
    return;
  }
  if (kind === 'folder') {
    if (!writable(roots[l.root])) return;
    openMenu(anchor, [
      { label: 'Rename…', hint: 'F2', run: () => nameFolderRow(l.root, id, anchor), keepFocus: true },
      { sep: true },
      { label: 'Move to the Trash', hint: '⌫', danger: true, run: () => trashFolder(l.root, id) },
    ], { label: id.split('/').pop() });
    return;
  }
  const t = tileById(id);
  if (!t) return;
  if (!writable(roots[t.root])) {
    openMenu(anchor, [
      { label: 'Add to print list…', disabled: !kindActions(t), why: 'only a tile goes in a print list', run: () => listMenu(t, anchor), keepFocus: true },
      { label: 'Copy to My tiles', sub: 'the library is read only; the copy is yours', disabled: !/\.dtile$/i.test(t.path), why: 'only a tile document is copied', run: () => copyToMine(t) },
    ], { label: stemOf(t.path) });
    return;
  }
  const can = kindActions(t);
  openMenu(anchor, [
    { label: 'Rename…', hint: 'F2', disabled: !can, why: 'only a tile document is renamed here', run: () => nameTileRow(t, anchor), keepFocus: true },
    { label: 'Add to print list…', disabled: !can, why: 'only a tile goes in a print list', run: () => listMenu(t, anchor), keepFocus: true },
    { label: 'Duplicate', disabled: !can, why: 'only a tile document is copied here', run: () => duplicate(t) },
    { label: 'Move to…', disabled: !can, why: 'only a tile document is moved here', run: () => moveMenu(t, anchor), keepFocus: true },
    { sep: true },
    { label: 'Move to the Trash', hint: '⌫', danger: true, disabled: !can, why: 'only a tile document is trashed here', run: () => trashTile(t) },
  ], { label: stemOf(t.path) });
}
const kindActions = (t) => /\.dtile$/i.test(t.path);

function rowAction(action, kind, id, anchor) {
  const { roots } = server.get();
  const l = levelNow();
  if (kind === 'place') { if (action === 'trash' && roots[Number(id)]?.role === 'added') removePlace(Number(id)); return; }
  if (kind === 'folder') {
    if (!writable(roots[l.root])) return;
    if (action === 'rename') nameFolderRow(l.root, id, anchor); else trashFolder(l.root, id);
    return;
  }
  const t = tileById(id);
  if (!t || !writable(roots[t.root]) || !kindActions(t)) return;
  if (action === 'rename') nameTileRow(t, anchor); else trashTile(t);
}

/** The arrows move between rows; → opens a folder or place, ← goes up. */
function onListKey(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const row = e.target.closest?.('.orow');
  const all = [...$('folder').querySelectorAll('.orow')];
  if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) {
    e.preventDefault();
    all[moveRow(row ? all.indexOf(row) : -1, e.key, all.length)]?.focus();
  } else if (e.key === 'ArrowRight' && row && row.dataset.kind !== 'tile') {
    e.preventDefault(); row.click();
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault(); goUpFolder();
  }
}

// ---- wiring ----------------------------------------------------------------------------------------------------------------

export function init() {
  started = true;
  keepEnd($('bcrumb-scroll'));
  keepEnd($('crumb-scroll'));
  $('bback').onclick = () => { if (!goUpFolder()) $('folder').focus(); };
  $('folder-vp').addEventListener('keydown', (e) => { if (e.target.closest?.('#folder')) onListKey(e); });
  $('filter').oninput = () => view.set({ query: $('filter').value });
  $('filter').onkeydown = (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); focusRow(); return; }
    if (e.key === 'Escape') {
      e.preventDefault(); e.stopPropagation();
      if (view.get().query) { view.set({ query: '' }); return; }
      if (tile()) view.set({ pageAt: 'edit' }); else goUpFolder();
      return;
    }
    if (e.key === 'Enter') $('folder').querySelector('.orow[data-kind="tile"], .orow')?.click();
  };
  $('forward').onclick = () => { if (tile()) view.set({ pageAt: 'edit' }); };
  bind();
  view.subscribe(['page'], () => {
    // coming back to the browser, the filter has the keyboard
    if (browsing()) requestAnimationFrame(() => { if (browsing()) $('filter').focus({ preventScroll: true }); });
  });
  addEventListener('popstate', fromAddress);
}
