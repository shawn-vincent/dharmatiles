// @ts-check
// The property sheet (the lens): the controls of the selection, and edits
// made there.

import { Lens, kdlString } from '../lens.js';
import { buildOutline } from '../outline.js';
import {
  addFrom, canMine, editable, moveEntry, muteAt, stepEntry, openRowMenu, removeAt, renameMark, rowAction, setControl, takeFocusIndex, tileEdit,
} from './rows.js';
import { orderStack } from '../nav.js';
import { showProperty } from '../property.js';
import { failed, showError } from '../problems.js';
import { $, host, makeMine } from './app.js';
import { edited, replaceBytes, spansUsable } from './document.js';
import { doc, elementNamed, entrySel, isEntry, isMark, itemOf, log, mapNode, markSel, nodeControl, nodeNamed, server, stackItems, stem, tile, view } from './stores.js';
import { closeControl, openControl, showSourceAt } from './levels.js';
import { setSel } from './selection.js';
import { clauseGroups, clauseMenu, combineNote, regionKind } from './region-sheet.js';
import { startRename } from './tiles.js';
import { haveBlock, isTileDoc } from './inventory.js';
import { listBlock } from './lists.js';


// ---- the lens ---------------------------------------------------------------

const sheetApi = {
  writable: () => doc.get().writable,
  replace: replaceBytes,
  set: setControl,
  canMine: () => canMine(),
  // the top field in focus (typed in or scrubbed) selects the entry's cloud; hovered, it shows quietly
  onField: (name, on) => { if (name === 'top') view.set({ cloudSheet: on && isEntry(view.get().sel) ? view.get().sel.id : null }); },
  onFieldHover: (name, on) => { if (name === 'top') view.set({ cloudSheetHov: on && isEntry(view.get().sel) ? view.get().sel.id : null }); },
  checkValue: async (element, control, typed) => {
    const r = await host.checkValue(element, control, typed);
    if (failed(`Could not check what was typed for ${control}`, r)) return null;
    return r.body?.message ?? null; // offline: nothing to say yet; the edit is checked again when it is sent
  },
  // the gallery's curves are the program's: a failure goes to the error panel,
  // a value the program refuses ({ok: false, message}) is for the gallery to show
  weighting: async (query) => {
    const r = await host.weightings(query);
    // the program refuses a value with 200 and {ok: false}; any error status is the page asking wrongly
    if (failed('Could not read the weighting curves', r)) return null;
    if (r.status >= 400) { showError('Could not read the weighting curves', new Error('Could not read the weighting curves', { cause: r.error })); return null; }
    return r.status === 0 ? null : r.body;
  },
};
// the three levels draw with the same api: the thing's sheet, the tile's (with what it holds under it) and one control's page
const lens = new Lens($('lens'), {
  ...sheetApi,
  // a control with more to show opens its own level (a stack entry's: ranges and heights)
  detail: (c) => isEntry(view.get().sel) && (c.ranges || c.unit === 'height'),
  openControl: (group, name) => openControl(group, name),
});
/** @type {any} */
const tileLens = new Lens($('lens-tile'), sheetApi);
tileLens.emptyTitle = 'No tile'; tileLens.emptyText = 'Choose a tile to see what it holds.';
// the tile node's own controls: shown when nothing is selected, so the sheet is never empty.
// What each is (its unit, help, default) is the program's; this only picks the rows
const tileRows = (names) => ({ controls: names.map((n) => nodeControl('tile', n)).filter(Boolean) });
// a written property as the lens reads it: `name=value` and its span
const asWritten = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: [c.span[0] - c.name.length - 1, c.span[1] + c.name.length + 1] });
/** The tile level's rows: what the tile holds (lines, regions, the stack in build order) and the messages each stack row wears. */
function tileContents() {
  const t = tile(), map = doc.get().map;
  const stack = orderStack(stackItems()).map((it) => ({ ...it, family: elementNamed(it.what)?.family }));
  const hover = (w) => view.set({ hov: w && w.kind === 'entry' ? entrySel(w.id) : null });
  return buildOutline(
    {
      lines: map?.lines, regions: map?.regions, stack, badges: t ? log.byNode(t.id) : new Map(),
      cols: map?.cols ?? t?.plan?.cols ?? 1, rows: map?.rows ?? t?.plan?.rows ?? 1,
      // rows can be changed when the document is the tile's own and has been checked (an unchecked one has no spans to address)
      editable: editable() && !!map && doc.get().tileId === view.get().tileId,
    },
    {
      open: (kind, id) => setSel(kind === 'entry' ? entrySel(id) : markSel(kind, id), /** @type {any} */ ({ from: 'view' })),
      hover,
      menu: openRowMenu,
      act: rowAction,
      add: addFrom,
      move: moveEntry,
      step: stepEntry,
    },
  );
}
function tileModel() {
  const t = tile(), own = doc.get().map?.tile;
  if (!t || !own || doc.get().file !== t.path) return null;
  const node = nodeNamed('tile');
  if (!node) return null; // the program's description of the tile is not here yet
  const groups = [{ at: own.at, schema: tileRows(['size', 'seed']), written: own.controls.map(asWritten) }];
  // the description: `about "…"` in the tile's body
  const about = own.about, av = about?.value;
  groups.push({
    heading: 'Description',
    at: own.at,
    schema: tileRows(['about']),
    written: av ? [asWritten({ name: 'about', text: av.text, span: av.span })] : [],
    set: (_, value) => (value == null
      ? about && tileEdit({ op: 'remove', at: about.at }, 'Took out the description')
      : about ? null : tileEdit({ op: 'insert', place: 'tile', node: `about ${kdlString(String(value))}` }, 'Described the tile')),
  });
  const actions = doc.get().writable ? [
    { label: 'Rename', title: 'Rename the tile’s file (F2)', run: () => startRename() },
  ] : [];
  // under the tile's own controls: what the inventory says of it and "I have this", then what it holds
  const tail = document.createElement('div');
  if (isTileDoc(t)) tail.append(haveBlock(t), listBlock(t));
  tail.append(tileContents());
  return { title: stem(t), kind: 'tile', href: node.href, groups, actions, tail, slim: true };
}
/** A run of written text (a line's points, a region's lines) as a lens setting. */
const runWritten = (name, run) => (run?.span ? asWritten({ name, text: run.text, span: run.span }) : null);
/** Change one clause of a region (a `within`'s distance or place, an `except`'s regions), through the server's edit route like the other controls. */
async function setClause(clause, name, value) {
  if (!doc.get().writable) return makeMine({ run: () => setClause(clause, name, value) });
  if (!spansUsable()) return;
  if (name === 'except') {
    const run = clause.regions;
    if (!value.length) { doc.set({ note: 'an except names at least one region; take the clause out instead' }); return null; }
    if (!run?.span) return null;
    return replaceBytes(run.span[0], run.span[1], value.join(' '), true);
  }
  if (name === 'of') return setControl(clause.at, 'of', value);
  if (name === 'distance') {
    const r = await host.edit({ op: 'arg', text: doc.get().text, at: clause.at, index: 0, value });
    if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed('Could not change the distance', r); return; }
    edited(r.body.text, 'lens', 'Changed distance');
  }
}
/** The sheet of a drawn line or a region: its name, its controls, its points. */
function markModel(kind, m) {
  const t = tile(), sh = m?.sheet;
  if (!t || !sh || doc.get().file !== t.path) return null;
  const node = nodeNamed(kind);
  if (!node) return null;
  // renaming goes through the server's edit route (`viaSet`), which renames every use of the name
  const nameRow = { ...nodeControl(kind, 'name'), viaSet: true };
  const written = [{ name: 'name', text: `name=${m.name}`, span: [sh.at, 0] }];
  let controls, set;
  if (kind === 'line') {
    controls = node.controls.map((c) => (c.name === 'name' ? nameRow : c));
    written.push(...sh.controls.map(asWritten));
    const th = runWritten('through', sh.through);
    if (th) written.push(th);
    set = (name, value) => (name === 'name' ? renameMark('line', m.name, value) : setControl(sh.at, name, value));
  } else {
    // a region is made as its source says: lines and fill points, places it reaches round and regions it excepts, or both
    const rk = regionKind(sh);
    const lines = (doc.get().map?.lines || []).map((l) => ({ word: l.name, about: `bounded by the line ${l.name}` }));
    controls = [nameRow];
    if (rk !== 'places') {
      controls.push(...node.controls.filter((c) => c.name !== 'name').map((c) => (c.name === 'boundedBy' ? { ...c, words: lines } : c)));
      for (const [n, run] of [['boundedBy', sh.boundedBy], ['fill', sh.fill]]) { const w = runWritten(n, run); if (w) written.push(w); }
    }
    set = (name, value) => {
      if (name === 'name') return renameMark('region', m.name, value);
      if (name === 'boundedBy') {
        const run = sh.boundedBy;
        if (!value.length) { doc.set({ note: 'a region needs at least one line' }); return null; }
        if (!run?.span) return null;
        return replaceBytes(run.span[0], run.span[1], value.join(' '), true);
      }
      return null;
    };
    // each clause is a group with a × that takes it out of the source
    const clauses = clauseGroups(m, sh, doc.get().map, asWritten, nodeControl).map((g) => ({
      at: g.clause.at, heading: g.heading, schema: { controls: g.controls }, written: g.written,
      set: (name, value) => setClause(g.clause, name, value),
      remove: { title: `Take out this ${g.clause.kind} clause`, run: () => tileEdit({ op: 'remove', at: g.clause.at }, `Took out a ${g.clause.kind} clause of ${m.name}`) },
    }));
    // the bucket (boundedBy and its fill points) is a clause too: one group, taken out together
    const bucket = controls.filter((c) => c.name !== 'name');
    const main = [{ at: sh.at, schema: { controls: controls.filter((c) => c.name === 'name') }, written: written.filter((w) => w.name === 'name'), set }];
    if (bucket.length) {
      main.push({
        heading: 'paint bucket', at: sh.at, schema: { controls: bucket }, written: written.filter((w) => w.name !== 'name'), set,
        remove: { title: 'Take out the boundedBy and its fill points', run: () => tileEdit({ op: 'remove-children', at: sh.at, names: ['boundedBy', 'fill'] }, `Took out the boundedBy of ${m.name}`) },
      });
    }
    const add = {
      label: 'add clause',
      items: clauseMenu(m, sh, doc.get().map).map((c) => ({
        ...c, label: c.disabled ? `${c.label} (${c.why})` : c.label,
        run: () => tileEdit({ op: 'insert', place: 'in', at: sh.at, node: c.node }, `Added a ${c.kind} clause to ${m.name}`),
      })),
    };
    const note = combineNote(sh);
    const actions = doc.get().writable || canMine() ? [{ label: 'Delete', title: `Delete this ${kind} from the source`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && view.set({ sel: null })) }] : [];
    return {
      title: m.name, kind, href: node.href, note,
      groups: [...main, ...clauses], add, actions,
    };
  }
  const actions = doc.get().writable || canMine() ? [{ label: 'Delete', title: `Delete this ${kind} from the source`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && view.set({ sel: null })) }] : [];
  return {
    title: m.name, kind, href: node.href,
    groups: [{ at: sh.at, schema: { controls }, written, set }], actions,
  };
}
/** The sheet model of the entry `s` selects, or null while the text, the check or the program's description is not there. */
function entryModel(s) {
  const it = itemOf(s);
  const elements = server.get().elements;
  const { file, writable } = doc.get();
  if (!it || !it.span || it.file !== file || !elements.length) return null;
  // the base line: no settings of its own; the build target (the system chip) picks the connector
  const base = it.element === 'connector';
  const groups = base ? [] : [{ at: it.span[0], schema: elementNamed(it.what), written: it.controls || [] }];
  for (const th of it.things || []) groups.push({ heading: th.what, at: th.at, schema: elementNamed(th.what), written: th.controls || [] });
  const actions = [];
  if (it.file === file && (writable || canMine())) {
    actions.push(
      { label: 'Hide', title: `Hide ${it.label} without deleting it`, run: () => muteAt(it.span[0], it.label) },
      { label: 'Delete', title: `Delete ${it.label} from the document`, run: () => removeAt(it.span[0], it.label) },
    );
  }
  const note = base ? 'The connector under zero. It has no settings: the build target (the system chip at the top) chooses DungeonBlocks or OpenLOCK. Delete the line for a tile with no base.' : undefined;
  return { title: it.label, kind: it.what || it.element, href: it.href, note, groups, actions };
}

/** The tile level: its controls, and under them what it holds. */
function renderTile() {
  const t = tileModel();
  // a row the arrow keys are on keeps the focus when the rows are drawn again (a message came, a value changed);
  // after a move the keyboard follows the row to where it went (its key may have changed with its place)
  const was = /** @type {HTMLElement | null} */ (document.activeElement?.closest?.('#lens-tile .orow'));
  const wasAt = was ? { kind: was.dataset.kind, id: was.dataset.id } : null;
  const moved = takeFocusIndex();
  if (t) tileLens.show(t); else tileLens.hide();
  const rowsNow = [...$('lens-tile').querySelectorAll('.orow')];
  if (moved != null) {
    const r = rowsNow.find((x) => x.dataset.kind === 'entry' && Number(x.dataset.index) === moved);
    if (r) { r.focus({ preventScroll: false }); return; }
  }
  if (wasAt) rowsNow.find((r) => r.dataset.kind === wasAt.kind && r.dataset.id === wasAt.id)?.focus({ preventScroll: true });
}

/** Draw the three levels: the tile's (always), the selected thing's, and the open control's. */
function renderLens() {
  const s = view.get().sel;
  renderTile();
  const model = isMark(s) ? markModel(s.kind, mapNode(s)) : isEntry(s) ? entryModel(s) : null;
  if (model) lens.show(model); else lens.hide();
  renderProperty(model);
}

const HEIGHT_NAMES = /bottom|height/i;
/** The control page: the open control of the selected entry, if the sheet still has it (else the page closes). */
function renderProperty(model) {
  const { sel, control } = view.get();
  const root = $('lens-prop');
  if (!control || !isEntry(sel)) { root.replaceChildren(); delete root.dataset.key; return; }
  if (!model) return; // the sheet is not drawn yet (the text is being checked): keep the page as it is
  const g = model.groups[control.group];
  const c = g?.schema?.controls?.find((x) => x.name === control.name);
  if (!c) { closeControl(); return; }
  // a value is being typed: the page is not redrawn under it (it is drawn again once the edit is written)
  const field = /** @type {HTMLElement} */ (document.activeElement);
  if (root.contains(field) && field.tagName === 'INPUT' && field.dataset.dirty) return;
  const held = root.contains(field) && field.tagName === 'INPUT'; // the field kept the keyboard through its own edit
  const element = g.schema?.name;
  // the same page drawn again (a value written) stays where it was scrolled to
  const key = `${view.get().tileId}|${sel.id}|${control.group}|${control.name}`;
  const kept = root.dataset.key === key ? root.querySelector('.body')?.scrollTop ?? 0 : 0;
  root.dataset.key = key;
  const page = showProperty(root, {
    element, label: `${model.title}${g.heading ? ` · ${g.heading}` : ''}`, c,
    s: g.written.find((w) => w.name === c.name), at: g.at,
    weightings: elementNamed(element)?.weightings,
    // the names the program reads in this tile (its own heights too), else the language's plain ones
    names: { heights: (doc.get().map?.language?.names || server.get().language?.names || []).filter((n) => HEIGHT_NAMES.test(n)) },
  }, { ...sheetApi, showSource: () => showSourceAt(view.get().sel, g.written.find((w) => w.name === c.name)?.span?.[0] ?? null) });
  const body = root.querySelector('.body');
  if (body) body.scrollTop = kept;
  if (held) { page.input.focus({ preventScroll: true }); page.input.setSelectionRange(page.input.value.length, page.input.value.length); }
}

let lastLists = null; // the print lists there are: the tile level's list chooser follows them
/** Wire this part of the page (once, at load). */
export function init() {
  view.subscribe(['sel', 'tileId', 'control'], renderLens);
  log.subscribe(renderTile);
  doc.subscribe(['stack', 'map', 'file', 'writable', 'tileId'], renderLens);
  server.subscribe(['elements', 'language', 'inventory', 'system'], renderLens);
  server.subscribe(['tiles'], () => { const n = server.get().tiles.filter((t) => /\.dmap$/i.test(t.path)).map((t) => t.id).join(); if (n !== lastLists) { lastLists = n; renderLens(); } });
  // before the text is checked the sheet reads the build's plan: not redrawn for builds that change nothing it shows
  let plan = null;
  server.subscribe(['tiles'], () => {
    const now = doc.get().stack ? null : JSON.stringify(tile()?.plan?.stack);
    if (now !== plan) { plan = now; renderLens(); }
  });
  renderLens();
}
