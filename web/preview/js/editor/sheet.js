// The property sheet (the lens): the controls of the selection, and edits
// made there.

import { Lens, kdlString } from '../lens.js';
import { byteToIndex } from '../text.js';
import { failed, showError } from '../problems.js';
import { $, host, makeMine, toast } from './app.js';
import { askDraft, edited, replaceBytes, spansUsable, undo } from './document.js';
import { doc, elementNamed, isEntry, isMark, itemOf, mapNode, markSel, nodeControl, nodeNamed, server, stackItems, stem, tile, view } from './stores.js';
import { clauseGroups, clauseMenu, combineNote, regionKind } from './region-sheet.js';
import { startRename } from './tiles.js';

/** A library tile's own text is shown: its first change copies it into My tiles (tiles.js). */
const canMine = () => { const t = tile(), d = doc.get(); return !d.writable && !!t?.path.startsWith('lib:') && d.file === t.path; };

// ---- the lens ---------------------------------------------------------------

/** A control written on the node at byte `at`, or taken out (`value` null). */
async function setControl(at, name, value) {
  if (!doc.get().writable) return makeMine({ run: () => setControl(at, name, value) });
  if (!spansUsable()) return;
  const r = await host.edit({ text: doc.get().text, at, name, value });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed(`Could not change ${name}`, r); return; }
  edited(r.body.text, 'lens', `Changed ${name}`);
}

export const lens = new Lens($('lens'), {
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
});
// the tile node's own controls: shown when nothing is selected, so the sheet is never empty.
// What each is (its unit, help, default) is the program's; this only picks the rows
const tileRows = (names) => ({ controls: names.map((n) => nodeControl('tile', n)).filter(Boolean) });
// a written property as the lens reads it: `name=value` and its span
const asWritten = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: [c.span[0] - c.name.length - 1, c.span[1] + c.name.length + 1] });
/** A whole-node change to the tile's text (insert or remove), as one step of history. */
async function tileEdit(body, label) {
  if (!doc.get().writable) return makeMine({ run: () => tileEdit(body, label) });
  if (!spansUsable()) return;
  const r = await host.edit({ text: doc.get().text, ...body });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed(`Could not make the change: ${label}`, r); return; }
  edited(r.body.text, 'lens', label);
  return true;
}
function renderTileLens() {
  const t = tile(), own = doc.get().map?.tile;
  if (!t || !own || doc.get().file !== t.path) { lens.hide(); return; }
  const node = nodeNamed('tile');
  if (!node) { lens.hide(); return; } // the program's description of the tile is not here yet
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
  const note = 'The tile. Select something on it to change that instead.';
  lens.show({ title: stem(t), kind: 'tile', href: node.href, note, groups, actions });
}
/** A run of written text (a line's points, a region's lines) as a lens setting. */
const runWritten = (name, run) => (run?.span ? asWritten({ name, text: run.text, span: run.span }) : null);
/** Rename the selected line or region, and keep it selected under its new name. */
async function renameMark(kind, old, name) {
  const next = String(name ?? '').trim();
  if (!next || next === old) return;
  if (await tileEdit({ op: 'rename', mark: kind, old, new: next }, `Renamed ${kind} ${old} to ${next}`)) {
    view.set({ sel: markSel(kind, next) });
  }
}
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
function renderMarkLens(kind, m) {
  const t = tile(), sh = m?.sheet;
  if (!t || !sh || doc.get().file !== t.path) { lens.hide(); return; }
  const node = nodeNamed(kind);
  if (!node) { lens.hide(); return; }
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
      for (const [n, run] of [['boundedBy', sh.bounded_by], ['fill', sh.fill]]) { const w = runWritten(n, run); if (w) written.push(w); }
    }
    set = (name, value) => {
      if (name === 'name') return renameMark('region', m.name, value);
      if (name === 'boundedBy') {
        const run = sh.bounded_by;
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
    lens.show({
      title: m.name, kind, href: node.href, note,
      groups: [...main, ...clauses], add, actions,
    });
    return;
  }
  const actions = doc.get().writable || canMine() ? [{ label: 'Delete', title: `Delete this ${kind} from the source`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && view.set({ sel: null })) }] : [];
  lens.show({
    title: m.name, kind, href: node.href,
    groups: [{ at: sh.at, schema: { controls }, written, set }], actions,
  });
}
function renderLens() {
  const s = view.get().sel;
  if (isMark(s)) { renderMarkLens(s.kind, mapNode(s)); return; }
  const it = isEntry(s) ? itemOf(s) : null;
  const elements = server.get().elements;
  if (!s || s.kind === 'tile') { renderTileLens(); return; }
  const { file, writable } = doc.get();
  if (!it || !it.span || it.file !== file || !elements.length) { lens.hide(); return; }
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
  lens.show({ title: it.label, kind: it.what || it.element, href: it.href, note, groups, actions });
}

/** Comment a stack entry out (`/-`), with a way back. */
function muteAt(byte, label = 'Entry') {
  if (!doc.get().writable) return makeMine({ run: () => muteAt(byte, label) });
  if (!spansUsable()) return;
  const t = doc.get().text, i = byteToIndex(t, byte);
  if (isEntry(view.get().sel)) view.set({ sel: null });
  edited(`${t.slice(0, i)}/- ${t.slice(i)}`, 'view', `Hid ${label}`);
  askDraft();
  toast(`${label} hidden`, () => undo(-1));
}
/** Take a stack entry out of the text, with a way back. */
async function removeAt(byte, label = 'Entry') {
  if (!doc.get().writable) return makeMine({ run: () => removeAt(byte, label) });
  if (!spansUsable()) return;
  const r = await host.edit({ op: 'remove', text: doc.get().text, at: byte });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that could not be taken out' }); failed(`Could not delete ${label}`, r); return; }
  view.set({ sel: null });
  edited(r.body.text, 'view', `Deleted ${label}`);
  askDraft();
  toast(`${label} removed`, () => undo(-1));
}

/** Wire this part of the page (once, at load). */
export function init() {
  view.subscribe(['sel', 'tileId'], renderLens);
  doc.subscribe(['stack', 'map', 'file', 'writable', 'tileId'], renderLens);
  server.subscribe(['elements', 'language'], renderLens);
  // before the text is checked the sheet reads the build's plan: not redrawn for builds that change nothing it shows
  let plan = null;
  server.subscribe(['tiles'], () => {
    const now = doc.get().stack ? null : JSON.stringify(tile()?.plan?.stack);
    if (now !== plan) { plan = now; renderLens(); }
  });
  renderLens();
}
