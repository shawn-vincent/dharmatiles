// The property sheet (the lens): the controls of the selection, and edits
// made there.

import { Lens, kdlString } from '../lens.js';
import { byteToIndex } from '../text.js';
import { $, host, isEntry, state, tile, viewer } from './app.js';
import { doc, edited, elementNamed, elements, mapNode, renderTextBar, stackItems, texts } from './document.js';
import { drawOverlay, muteAt, removeAt } from './map.js';
import { setSel } from './selection.js';
import { offerMine, startRename, stem } from './tiles.js';

// ---- the lens ---------------------------------------------------------------

export const lens = new Lens($('lens'), {
  writable: () => doc.writable,
  replace(start, len, text, end) {
    const t = texts.text;
    const i = byteToIndex(t, start), j = byteToIndex(t, start + len);
    texts.setText(t.slice(0, i) + text + t.slice(j));
    edited(texts.text, end ? 'lens' : 'drag');
  },
  async set(at, name, value) {
    if (!doc.writable) return offerMine();
    const r = await host.post('/api/edit', { text: texts.text, at, name, value });
    if (r.status !== 200) { doc.note = r.body?.error || 'that change could not be made'; renderTextBar(); return; }
    texts.setText(r.body.text);
    edited(texts.text, 'lens', `Changed ${name}`);
  },
  makeMine: () => offerMine(),
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
  if (!doc.writable) return offerMine();
  const r = await host.post('/api/edit', { text: texts.text, ...body });
  if (r.status !== 200) { doc.note = r.body?.error || 'that change could not be made'; renderTextBar(); return; }
  texts.setText(r.body.text);
  edited(texts.text, 'lens', label);
  return true;
}
const LEVEL_ABOUT = {
  edge: 'where the ground meets the tile edge; neighbouring tiles line up on it',
  ground: 'grass and soil regions',
  water: 'the calm water surface',
  'wall-top': 'the top of a wall',
};
function renderTileLens() {
  const t = tile(), own = doc.map?.tile;
  if (!t || !own || texts.file !== t.path) { lens.hide(); return; }
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
  // the tile's levels: the standard ones it can override, and its own
  const lv = own.levels;
  if (!lv?.block) {
    groups.push({
      heading: 'Levels (mm)',
      at: lv?.at ?? own.at,
      schema: {
        controls: (own.standard_levels || []).map((l) => ({
          name: l.name, unit: 'mm', default: String(l.value),
          about: `${LEVEL_ABOUT[l.name] || 'a standard level'}; ${l.value} mm unless this tile says otherwise`,
        })),
      },
      written: (lv?.controls || []).map(asWritten),
      set: (name, value) => {
        if (lv) {
          if (value == null && lv.controls.length === 1 && lv.controls[0].name === name) return tileEdit({ op: 'remove', at: lv.at }, `Took out level ${name}`);
          return lens.api.set(lv.at, name, value);
        }
        if (value == null) return null;
        return tileEdit({ op: 'insert', place: 'tile', node: `levels ${name}=${typeof value === 'number' ? value : kdlString(String(value))}` }, `Set level ${name}`);
      },
    });
  }
  const actions = doc.writable ? [
    { label: 'Rename', title: 'Rename the tile’s file (F2)', run: () => startRename() },
  ] : [];
  const note = lv?.block ? 'The tile. Its levels are written as a block: change them in the text.' : 'The tile. Select something on it to change that instead.';
  lens.show({ title: stem(t), kind: 'tile', href: 'https://shawn-vincent.github.io/dharmatiles/language.html', note, groups, actions });
}
/** A run of written text (a line's points, a region's lines) as a lens setting. */
const runWritten = (name, run) => (run?.span ? asWritten({ name, text: run.text, span: run.span }) : null);
/** Rename the selected line or region, and keep it selected under its new name. */
async function renameMark(kind, old, name) {
  const next = String(name ?? '').trim();
  if (!next || next === old) return;
  if (await tileEdit({ op: 'rename', mark: kind, old, new: next }, `Renamed ${kind} ${old} to ${next}`)) {
    state.sel = kind === 'line' ? { line: next } : { region: next };
  }
}
/** The sheet of a drawn line or a region: its name, its controls, its points. */
function renderMarkLens(kind, m) {
  const t = tile(), sh = m?.sheet;
  if (!t || !sh || texts.file !== t.path) { lens.hide(); return; }
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
    set = (name, value) => (name === 'name' ? renameMark('line', m.name, value) : lens.api.set(sh.at, name, value));
  } else {
    const lines = (doc.map?.lines || []).map((l) => ({ word: l.name, about: `bounded by the line ${l.name}` }));
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
        if (!value.length) { doc.note = 'a region needs at least one line'; renderTextBar(); return null; }
        if (!run?.span) return null;
        return lens.api.replace(run.span[0], run.span[1], value.join(' '), true);
      }
      return null;
    };
  }
  const actions = doc.writable ? [{ label: 'Delete', title: `Delete this ${kind} from the text`, danger: true, run: () => tileEdit({ op: 'remove', at: sh.at }, `Deleted ${kind} ${m.name}`).then((ok) => ok && setSel(null)) }] : [];
  lens.show({
    title: m.name, kind, href: `https://shawn-vincent.github.io/dharmatiles/language.html#${kind === 'line' ? 'lines' : 'regions'}`,
    groups: [{ at: sh.at, schema: { controls }, written, set }], actions,
  });
}
export function renderLens() {
  const s = state.sel;
  if (s && (s.line != null || s.region != null)) { renderMarkLens(s.line != null ? 'line' : 'region', mapNode(s)); return; }
  const it = isEntry(s) ? stackItems()[s.entry] : null;
  if (!s || s.tile) { renderTileLens(); return; }
  if (!it || it.element === 'connector' || !it.span || it.file !== texts.file || !elements.length) { lens.hide(); return; }
  const groups = [{ at: it.span[0], schema: elementNamed(it.what), written: it.controls || [] }];
  for (const th of it.things || []) groups.push({ heading: th.what, at: th.at, schema: elementNamed(th.what), written: th.controls || [] });
  const actions = [];
  if (it.file === texts.file && doc.writable) {
    actions.push(
      { label: 'Hide', title: `Hide ${it.label} without deleting it`, run: () => muteAt(it.span[0], it.label) },
      { label: 'Delete', title: `Delete ${it.label} from the document`, run: () => removeAt(it.span[0], it.label) },
    );
  }
  lens.show({ title: it.label, kind: it.what || it.element, href: it.href, groups, actions });
}

/** Wire this part of the page (once, at load). */
export function init() {
  viewer.onRender = () => drawOverlay();
}
