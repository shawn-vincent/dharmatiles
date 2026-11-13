// @ts-check
// Edits made from the structure (docs/preview.html, "The panel"): what a row of the tile
// level does to the source.  Add an entry at a place in the stack (what can stand there is the
// program's answer: the plan's), hide, delete and rename a row, move a stack row to another
// place (a drag, ⌥↑ / ⌥↓, or the row's menu), add a line or a region.  Each is an edit through
// the server's edit route, one step of the history, so the toast's Undo and ⌘Z take it back.
// The rules (where a node goes, what a name may be) are js/rows.js; the popups js/menu.js.

import { orderStack, ordinal } from '../nav.js';
import { openMenu, openNameField } from '../menu.js';
import { failed } from '../problems.js';
import {
  addPlace, canStep, idFlaw, idOf, landing, movedNote, nameOf, orderRefusal, ownNode, planMove, refusalLine, stepSlot, writtenIn,
} from '../rows.js';
import { byteToIndex } from '../text.js';
import { host, makeMine, toast } from './app.js';
import { askDraft, edited, spansUsable, undo } from './document.js';
import { doc, isEntry, markSel, stackItems, tile, view } from './stores.js';

/** A library tile's own text is shown: its first change copies it into My tiles (tiles.js). */
export const canMine = () => { const t = tile(), d = doc.get(); return !d.writable && !!t?.path.startsWith('lib:') && d.file === t.path; };
/** May a change be made to the document shown (it is the tile's own, and writable or a library tile that is copied first)? */
export const editable = () => doc.get().file === tile()?.path && (doc.get().writable || canMine());

const list = () => orderStack(stackItems());

/** What an edit that was refused says: the program's words, shown where the change was asked for. */
function refused(what, r) {
  const why = typeof r.body?.error === 'string' ? r.body.error : r.body?.error?.message || 'that change could not be made';
  doc.set({ note: why });
  if (!failed(what, r)) toast(`${what}: ${why}`);
}

// ---- edits of one node ---------------------------------------------------------------------

/** A control written on the node at byte `at`, or taken out (`value` null). */
export async function setControl(at, name, value) {
  if (!doc.get().writable) return makeMine({ run: () => setControl(at, name, value) });
  if (!spansUsable()) return;
  const r = await host.edit({ op: 'set', text: doc.get().text, at, name, value });
  if (r.status !== 200) { refused(`Could not change ${name}`, r); return; }
  edited(r.body.text, 'lens', `Changed ${name}`);
  return true;
}

/** A whole-node change to the tile's text (insert, remove, move or rename), as one step of history. */
export async function tileEdit(body, label) {
  if (!doc.get().writable) return makeMine({ run: () => tileEdit(body, label) });
  if (!spansUsable()) return;
  const r = await host.edit({ text: doc.get().text, ...body });
  if (r.status !== 200) { refused(`Could not make the change: ${label}`, r); return; }
  edited(r.body.text, 'lens', label);
  return true;
}

/** Comment a stack entry out (`/-`), with a way back. */
export function muteAt(byte, label = 'Entry') {
  if (!doc.get().writable) return makeMine({ run: () => muteAt(byte, label) });
  if (!spansUsable()) return;
  const t = doc.get().text, i = byteToIndex(t, byte);
  if (isEntry(view.get().sel)) view.set({ sel: null });
  edited(`${t.slice(0, i)}/- ${t.slice(i)}`, 'view', `Hid ${label}`);
  askDraft();
  toast(`${label} hidden`, () => undo(-1));
}

/** Take a stack entry out of the text, with a way back. */
export async function removeAt(byte, label = 'Entry') {
  if (!doc.get().writable) return makeMine({ run: () => removeAt(byte, label) });
  if (!spansUsable()) return;
  const r = await host.edit({ op: 'remove', text: doc.get().text, at: byte });
  if (r.status !== 200) { refused(`Could not delete ${label}`, r); return; }
  view.set({ sel: null });
  edited(r.body.text, 'view', `Deleted ${label}`);
  askDraft();
  toast(`${label} removed`, () => undo(-1));
}

/** Rename the selected line or region, and keep it selected under its new name. */
export async function renameMark(kind, old, name) {
  const next = String(name ?? '').trim();
  if (!next || next === old) return;
  if (await tileEdit({ op: 'rename', mark: kind, old, new: next }, `Renamed ${kind} ${old} to ${next}`)) {
    view.set({ sel: markSel(kind, next) });
  }
}

// ---- the stack ----------------------------------------------------------------------------------------

let wanted = null; // {index, text}: the row the keyboard goes to once the moved stack is on screen

/** The row to put the keyboard on after a move, if the stack shown is the moved one: its list position (once). */
export function takeFocusIndex() {
  if (!wanted) return null;
  const d = doc.get();
  if (d.text !== wanted.text) { wanted = null; return null; }
  if (d.checked !== wanted.text) return null;
  const { index } = wanted;
  wanted = null;
  return index;
}

/** After a change to the order, say so when the plan refuses it: in its own words, with the Undo that goes back to the order before. */
function watchOrder(text) {
  let off = () => {};
  const stop = setTimeout(() => off(), 20000);
  off = doc.subscribe(['checked', 'text'], () => {
    const d = doc.get();
    if (d.text !== text) { clearTimeout(stop); off(); return; }
    if (d.checked !== text) return;
    clearTimeout(stop);
    off();
    const e = orderRefusal(d.errors);
    if (e) toast(`That order does not build: ${refusalLine(e).replace(/^./, (c) => c.toLowerCase())}`, () => undo(-1), { ms: 15000 });
  });
}

/**
 * Move stack row `from` to stand before list position `slot` (the list length is the end).  The check
 * says whether the plan accepts the new order: a refused order is allowed and reported in the checker's
 * own words (with an Undo), never blocked on the way: a stack is often put right in two moves.
 */
export async function moveEntry(from, slot) {
  const rows = list();
  const it = rows[from];
  if (!it) return;
  const file = doc.get().file;
  const to = Math.max(0, Math.min(rows.length, slot));
  if (!writtenIn(it, file)) {
    toast(`${nameOf(it)} is written in another file: change it there`);
    return;
  }
  const plan = planMove(rows, from, to, file);
  if (!plan) return; // where it already is
  if (!doc.get().writable) return makeMine({ run: () => moveEntry(from, slot) });
  if (!spansUsable()) return;
  const r = await host.edit({ op: 'move', text: doc.get().text, at: plan.at, before: plan.before });
  if (r.status !== 200) { refused(`Could not move ${nameOf(it)}`, r); return; }
  const note = movedNote(rows, from, to, nameOf(it));
  wanted = { index: landing(rows, from, to, file), text: r.body.text };
  edited(r.body.text, 'lens', `Moved ${nameOf(it)}`);
  askDraft();
  toast(note, () => undo(-1), { ms: note.length > 70 ? 9000 : 5000 });
  watchOrder(r.body.text);
}

/** One place earlier (`dir` -1) or later (+1) in the written stack, which is down or up the tile as drawn: ⌥↓ and ⌥↑ on a row, and the row menu's Move down and Move up. */
export function stepEntry(from, dir) {
  const rows = list();
  const slot = stepSlot(rows, from, dir, doc.get().file);
  if (slot == null) return;
  return moveEntry(from, slot);
}

/** Write `offer`'s node at `place` ({place, at}) and open the new entry's level. */
async function insertEntry(place, offer) {
  if (!doc.get().writable) return makeMine({ run: () => insertEntry(place, offer) });
  if (!spansUsable()) return;
  /** @type {import('../api').EditReq} */
  const body = place.place === 'before'
    ? { op: 'insert', place: 'before', at: place.at, node: offer.node, text: doc.get().text }
    : { op: 'insert', place: 'stack', node: offer.node, text: doc.get().text };
  const r = await host.edit(body);
  if (r.status !== 200) { refused(`Could not add ${offer.name}`, r); return; }
  doc.set({ selectAt: r.body.at });
  edited(r.body.text, 'lens', `Added ${offer.name}`, byteToIndex(r.body.text, r.body.at));
  askDraft();
  toast(`Added ${offer.name}`, () => undo(-1));
}

/**
 * The Add menu for list position `slot` (the end when left out): every element, those the plan lets stand
 * there first-class, the rest listed with the plan's reason.  Choosing one writes it and opens its level.
 */
export async function openAdd(anchor, slot = null, { phrase = null, align = 'left' } = {}) {
  const rows = list();
  const at = slot == null ? rows.length : slot;
  if (!editable()) { toast('This part is read only: change the tile’s own text'); return; }
  if (!spansUsable()) return;
  const place = addPlace(rows, at, doc.get().file);
  const t = tile(), d = doc.get();
  anchor.setAttribute('aria-busy', 'true');
  let r;
  try {
    r = await host.offers(t.id, { text: d.text, at: place.index, file: d.file === t.path ? undefined : d.file });
  } finally { anchor.removeAttribute('aria-busy'); }
  if (failed('Could not ask what can be added', r)) return;
  if (r.status !== 200) { refused('Could not ask what can be added', r); return; }
  const where = phrase || (place.place === 'stack' ? 'on top' : at === 0 ? 'at the bottom' : `as ${ordinal(place.index + 1)}`);
  if (!r.body.ok) {
    openMenu(anchor, [
      { head: 'Add to the stack' },
      { label: 'The tile has a mistake', sub: `${r.body.why || 'fix it first'}: what can stand here is asked of a tile that checks`, disabled: true },
    ], { label: 'Add to the stack', cls: 'addmenu', align });
    return;
  }
  const ok = r.body.offers.filter((o) => o.ok), no = r.body.offers.filter((o) => !o.ok);
  const item = (o) => ({ label: o.name, sub: o.about, swatch: o.family, disabled: !o.ok, why: o.why, run: () => insertEntry(place, o) });
  openMenu(anchor, [
    { head: `Add ${where}` },
    ...ok.map(item),
    ...(no.length ? [{ sep: true }, { head: 'Cannot stand here' }, ...no.map(item)] : []),
  ], { label: `Add ${where}`, cls: 'addmenu', align });
}

// ---- lines and regions ---------------------------------------------------------------------------------

/** A name no line, region or point of the tile has yet: `line1`, `line2`… */
function freeName(base) {
  const n = doc.get().names || {};
  const taken = new Set([...(n.lines || []), ...(n.regions || []), ...(n.points || []), ...(n.defs || [])]);
  for (let i = 1; ; i++) if (!taken.has(`${base}${i}`)) return `${base}${i}`;
}

/** Write a plain line across the middle, or a region along the north edge, into the tile's body and open its level. */
export async function addMark(kind) {
  if (!editable()) { toast('This part is read only: change the tile’s own text'); return; }
  const name = freeName(kind);
  const node = kind === 'line'
    ? `line ${name} { through "(0sq, 0.5sq)" "(1sq, 0.5sq)" }`
    : `region ${name} { within "8dmm" of=northEdge }`;
  if (await tileEdit({ op: 'insert', place: 'tile', node }, `Added ${kind} ${name}`)) {
    view.set({ sel: markSel(kind, name) });
    toast(`Added ${kind} ${name}`, () => undo(-1));
  }
}

/** Delete a line or a region from its row, with a way back. */
async function deleteMark(kind, m) {
  const at = m?.sheet?.at;
  if (at == null) return;
  if (await tileEdit({ op: 'remove', at }, `Deleted ${kind} ${m.name}`)) {
    if (view.get().sel?.kind === kind && view.get().sel.id === m.name) view.set({ sel: null });
    toast(`Deleted ${kind} ${m.name}`, () => undo(-1));
  }
}

// ---- rename ----------------------------------------------------------------------------------------------

/** The popover with one field: `flaw(value)` says why a name will not do; `save(value)` writes it. */
export const nameField = openNameField;

/** Name an entry: `id=` on its node.  A name is part of its seed path, so the look is drawn again once and then kept. */
function renameEntry(it, anchor) {
  const rows = list();
  const cur = idOf(it.key) ?? '';
  nameField(anchor, {
    title: `Name this ${it.what}`,
    value: cur,
    hint: 'Writes id= on the entry. Its seed is keyed by the name, so it looks different once and then keeps that look when entries are added, taken out or moved. Empty takes the name off.',
    flaw: (v) => idFlaw(v, rows, it),
    save: async (v) => {
      if (v === cur) return;
      if (!v && !cur) return;
      const ok = await setControl(ownNode(it, doc.get().file), 'id', v || null);
      if (ok) toast(v ? `Named ${it.what} ${v}: it looks different once, and keeps that look` : `Took the name off ${cur}`, () => undo(-1), { ms: 7000 });
    },
  });
}

/** Rename a line or region from its row, with every use of its name. */
function renameShape(kind, name, anchor) {
  const map = doc.get().map;
  const taken = new Set([...(map?.lines || []), ...(map?.regions || [])].map((m) => m.name));
  nameField(anchor, {
    title: `Rename this ${kind}`,
    value: name,
    hint: `Every place the tile names it (${kind === 'line' ? 'boundedBy and at=' : 'at='}) changes with it.`,
    flaw: (v) => {
      const s = v.trim();
      if (!s) return 'A name is needed';
      if (!/^[A-Za-z][A-Za-z0-9_-]*$/.test(s)) return 'A letter, then letters, digits, - and _';
      return s !== name && taken.has(s) ? `Another ${kind === 'line' ? 'line' : 'region'} is called ${s}` : null;
    },
    save: (v) => renameMark(kind, name, v),
  });
}

// ---- the row's menu ---------------------------------------------------------------------------------------------

const labelOf = (it) => nameOf(it);

/** The menu of a row of the tile level (its ⋯, the menu key). */
export function openRowMenu(kind, id, anchor) {
  if (kind === 'add') return;
  if (kind !== 'entry') {
    const map = doc.get().map;
    const m = (kind === 'line' ? map?.lines : map?.regions)?.find((x) => x.name === id);
    if (!m) return;
    openMenu(anchor, [
      { label: 'Rename…', hint: 'F2', run: () => renameShape(kind, id, anchor), keepFocus: true },
      { label: 'Delete', hint: '⌫', danger: true, disabled: m.sheet?.at == null, why: 'not written in this document', run: () => deleteMark(kind, m) },
    ], { label: `${kind} ${id}` });
    return;
  }
  const rows = list();
  const i = rows.findIndex((x) => x.key === id);
  const it = rows[i];
  if (!it) return;
  const file = doc.get().file;
  const mine = writtenIn(it, file) && editable();
  const base = it.what === 'base';
  const elsewhere = 'written in another file: change it there';
  const why = mine ? null : elsewhere;
  // a call of a definition makes the entry: it is renamed, hidden and deleted as the call
  const called = !!it.call;
  const step = (dir) => canStep(rows, i, dir, file);
  openMenu(anchor, [
    { label: 'Rename…', hint: 'F2', disabled: !mine || base || called, why: base ? 'the base has no name' : called ? 'made by a definition call: name the call' : why, run: () => renameEntry(it, anchor), keepFocus: true },
    { label: 'Hide', sub: 'comment it out, keep it', disabled: !mine, why, run: () => muteAt(ownNode(it, file), labelOf(it)) },
    { label: 'Delete', hint: '⌫', danger: true, disabled: !mine, why, run: () => removeAt(ownNode(it, file), labelOf(it)) },
    { sep: true },
    // the rows are drawn top of the tile first: up the list is later in the written stack
    { label: 'Move up', hint: '⌥↑', disabled: !mine || !step(1), why: !mine ? why : 'already at the top', run: () => stepEntry(i, 1) },
    { label: 'Move down', hint: '⌥↓', disabled: !mine || !step(-1), why: !mine ? why : 'already at the bottom', run: () => stepEntry(i, -1) },
    { sep: true },
    { label: 'Add above…', run: () => openAdd(anchor, i + 1, { phrase: `above ${labelOf(it)}`, align: 'right' }), keepFocus: true },
    { label: 'Add below…', run: () => openAdd(anchor, i, { phrase: `below ${labelOf(it)}`, align: 'right' }), keepFocus: true },
  ], { label: `${labelOf(it)}` });
}

/** A row's key that is an action of its own: F2 (rename) or Delete. */
export function rowAction(action, kind, id, anchor) {
  if (kind === 'add') return;
  if (kind !== 'entry') {
    const map = doc.get().map;
    const m = (kind === 'line' ? map?.lines : map?.regions)?.find((x) => x.name === id);
    if (!m) return;
    if (action === 'rename') renameShape(kind, id, anchor);
    else if (m.sheet?.at != null) deleteMark(kind, m);
    return;
  }
  const rows = list();
  const it = rows.find((x) => x.key === id);
  if (!it || !writtenIn(it, doc.get().file) || !editable()) return;
  if (action === 'rename') { if (it.what !== 'base' && !it.call) renameEntry(it, anchor); } else removeAt(ownNode(it, doc.get().file), labelOf(it));
}

/** The Add row of the stack (`entry`), or of the Shape (`line`, `region`). */
export function addFrom(kind, anchor) {
  if (kind === 'entry') openAdd(anchor);
  else addMark(kind);
}
