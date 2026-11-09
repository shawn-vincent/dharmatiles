// The property sheet (the lens): the controls of the selection, and edits
// made there.

import { Lens, kdlString } from '../lens.js';
import { byteToIndex } from '../text.js';
import { failed } from '../problems.js';
import { $, host, offerMine, toast } from './app.js';
import { askDraft, edited, replaceBytes, undo } from './document.js';
import { doc, elementNamed, isEntry, isMark, mapNode, markSel, server, stackItems, stem, tile, view } from './stores.js';
import { startRename } from './tiles.js';

// ---- the lens ---------------------------------------------------------------

/** A control written on the node at byte `at`, or taken out (`value` null). */
async function setControl(at, name, value) {
  if (!doc.get().writable) return offerMine();
  const r = await host.edit({ text: doc.get().text, at, name, value });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed(`Could not change ${name}`, r); return; }
  edited(r.body.text, 'lens', `Changed ${name}`);
}

export const lens = new Lens($('lens'), {
  writable: () => doc.get().writable,
  replace: replaceBytes,
  set: setControl,
  makeMine: () => offerMine(),
  checkValue: async (element, control, typed) => {
    const r = await host.checkValue(element, control, typed);
    if (failed(`Could not check what was typed for ${control}`, r)) return null;
    return r.body?.message ?? null; // offline: nothing to say yet; the edit is checked again when it is sent
  },
});
/** The lens for the selection: an entry's controls (a scatter's and each of its things'). */
// the tile node's own controls: shown when nothing is selected, so the sheet is never empty
const TILE_SCHEMA = {
  name: 'tile',
  controls: [
    { name: 'size', about: 'squares across, then up: "2x1"; click to type or pick it from a grid, or drag a number', default: '"1x1"', picker: 'size' },
    { name: 'seed', unit: 'number', about: 'the layout: another seed, another arrangement of the same tile', default: '1' },
  ],
};
// a written property as the lens reads it: `name=value` and its span
const asWritten = (c) => ({ name: c.name, text: `${c.name}=${c.text}`, span: [c.span[0] - c.name.length - 1, c.span[1] + c.name.length + 1] });
/** A whole-node change to the tile's text (insert or remove), as one step of history. */
async function tileEdit(body, label) {
  if (!doc.get().writable) return offerMine();
  const r = await host.edit({ text: doc.get().text, ...body });
  if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed(`Could not make the change: ${label}`, r); return; }
  edited(r.body.text, 'lens', label);
  return true;
}
function renderTileLens() {
  const t = tile(), own = doc.get().map?.tile;
  if (!t || !own || doc.get().file !== t.path) { lens.hide(); return; }
  const groups = [{ at: own.at, schema: TILE_SCHEMA, written: own.controls.map(asWritten) }];
  // the description: `about "…"` in the tile's body
  const about = own.about, av = about?.value;
  groups.push({
    heading: 'Description',
    at: own.at,
    schema: { controls: [{ name: 'about', unit: 'text', about: 'a sentence on what the tile is; shown in the library' }] },
    written: av ? [asWritten({ name: 'about', text: av.text, span: av.span })] : [],
    set: (_, value) => (value == null
      ? about && tileEdit({ op: 'remove', at: about.at }, 'Took out the description')
      : about ? null : tileEdit({ op: 'insert', place: 'tile', node: `about ${kdlString(String(value))}` }, 'Described the tile')),
  });
  const actions = doc.get().writable ? [
    { label: 'Rename', title: 'Rename the tile’s file (F2)', run: () => startRename() },
  ] : [];
  const note = 'The tile. Select something on it to change that instead.';
  lens.show({ title: stem(t), kind: 'tile', href: 'https://shawn-vincent.github.io/dharmatiles/language.html', note, groups, actions });
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
/** The sheet of a drawn line or a region: its name, its controls, its points. */
function renderMarkLens(kind, m) {
  const t = tile(), sh = m?.sheet;
  if (!t || !sh || doc.get().file !== t.path) { lens.hide(); return; }
  const nameRow = { name: 'name', unit: 'word', required: true, viaSet: true, about: `what the tile calls this ${kind}; renaming it renames every ${kind === 'line' ? '`bounded-by` and `along=`' : '`in=`'} that names it` };
  const written = [{ name: 'name', text: `name=${m.name}`, span: [sh.at, 0] }];
  let controls, set;
  if (kind === 'line') {
    controls = [
      nameRow,
      { name: 'wobble', unit: 'mm', default: '0', about: 'how far the line may wander off its points' },
      { name: 'width', unit: 'mm', about: 'makes the line a strip; an element along= it covers the strip' },
      { name: 'through', unit: 'raw', required: true, about: 'the points it runs through: "x,y" in squares, corners, or edge middles (south, east, north, west); drag a number or click to type' },
    ];
    written.push(...sh.controls.map(asWritten));
    const th = runWritten('through', sh.through);
    if (th) written.push(th);
    set = (name, value) => (name === 'name' ? renameMark('line', m.name, value) : setControl(sh.at, name, value));
  } else {
    const lines = (doc.get().map?.lines || []).map((l) => ({ word: l.name, about: `bounded by the line ${l.name}` }));
    controls = [
      nameRow,
      { name: 'bounded-by', unit: 'several', words: lines, required: true, about: 'the lines that divide the tile into this region and the rest' },
      { name: 'fill', unit: 'raw', required: true, about: 'a point inside the region, "x,y" in squares: the region is the area around it that the lines enclose' },
    ];
    for (const [n, run] of [['bounded-by', sh.bounded_by], ['fill', sh.fill]]) { const w = runWritten(n, run); if (w) written.push(w); }
    set = (name, value) => {
      if (name === 'name') return renameMark('region', m.name, value);
      if (name === 'bounded-by') {
        const run = sh.bounded_by;
        if (!value.length) { doc.set({ note: 'a region needs at least one line' }); return null; }
        if (!run?.span) return null;
        return replaceBytes(run.span[0], run.span[1], value.join(' '), true);
      }
      return null;
    };
  }
  const actions = doc.get().writable ? [{ label: 'Delete', title: `Delete this ${kind} from the source`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && view.set({ sel: null })) }] : [];
  lens.show({
    title: m.name, kind, href: `https://shawn-vincent.github.io/dharmatiles/language.html#${kind === 'line' ? 'lines' : 'regions'}`,
    groups: [{ at: sh.at, schema: { controls }, written, set }], actions,
  });
}
function renderLens() {
  const s = view.get().sel;
  if (isMark(s)) { renderMarkLens(s.kind, mapNode(s)); return; }
  const it = isEntry(s) ? stackItems()[s.id] : null;
  const elements = server.get().elements;
  if (!s || s.kind === 'tile') { renderTileLens(); return; }
  const { file, writable } = doc.get();
  if (!it || !it.span || it.file !== file || !elements.length) { lens.hide(); return; }
  // the base line: no settings of its own; the build target (the system chip) picks the connector
  const base = it.element === 'connector';
  const groups = base ? [] : [{ at: it.span[0], schema: elementNamed(it.what), written: it.controls || [] }];
  for (const th of it.things || []) groups.push({ heading: th.what, at: th.at, schema: elementNamed(th.what), written: th.controls || [] });
  const actions = [];
  if (it.file === file && writable) {
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
  if (!doc.get().writable) return offerMine();
  const t = doc.get().text, i = byteToIndex(t, byte);
  if (isEntry(view.get().sel)) view.set({ sel: null });
  edited(`${t.slice(0, i)}/- ${t.slice(i)}`, 'view', `Hid ${label}`);
  askDraft();
  toast(`${label} hidden`, () => undo(-1));
}
/** Take a stack entry out of the text, with a way back. */
async function removeAt(byte, label = 'Entry') {
  if (!doc.get().writable) return offerMine();
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
  server.subscribe(['elements'], renderLens);
  // before the text is checked the sheet reads the build's plan: not redrawn for builds that change nothing it shows
  let plan = null;
  server.subscribe(['tiles'], () => {
    const now = doc.get().stack ? null : JSON.stringify(tile()?.plan?.stack);
    if (now !== plan) { plan = now; renderLens(); }
  });
  renderLens();
}
