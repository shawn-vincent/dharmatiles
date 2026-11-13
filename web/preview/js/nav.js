// The panel's levels (docs: preview.html): where you are is a path, and the
// left panel shows exactly one level of it, full height.  This is the model
// and nothing else (no page here): the address of a level, the crumbs over
// it, the strip of context under them, the address in the URL, the stack as
// the tile level lists it, and how a row moves.  editor/levels.js draws it.
//
//   depth 0  the tile: its own controls, and what it holds (lines, regions, the stack)
//   depth 1  one thing of the tile: a stack entry, a line or a region (its sheet)
//   depth 2  one control of it, where the control has more to show (a range, a height)
//
// A level is `{sel, control}`: `sel` is the page's selection (editor/stores.js:
// {kind: 'entry'|'region'|'line'|'tile', id, piece?}, an entry by its key), and
// `control` names the control at depth 2 ({name, group}: group is the index of
// the sheet's group, 0 for the entry's own controls, 1.. for a scatter's things).
// Folders are the levels above the tile (js/folders.js): Places, a place, a folder
// of it, then the tile.  The crumbs run through all of them, and the address of a
// folder level is `#place=<place>&dir=<folder>` (no `tile=`).  The folders a map's Add step walks are
// the same kind of level, below the map: Add, Places, a place, its folders; its address is `add=<place>&adir=<folder>`.

const entryLike = (s) => s?.kind === 'entry' || s?.kind === 'region' || s?.kind === 'line';

/** How deep `sel` (and the open `control`) is: 0 the tile, 1 a thing of it, 2 one control of that. */
export function depthOf(sel, control = null) {
  if (!entryLike(sel)) return 0;
  return control ? 2 : 1;
}

// ---- the shell's levels: one table, whatever the document ---------------------------------------
//
// The panel's levels shell (editor/levels.js) shows one level at a time and slides between them.  What it needs to know of a level is
// here and nowhere else: its depth, and which kind of document it belongs to.  A tile's levels are the tile, an entry and a control; a
// map's are the map, a piece (or a folder to add tiles from) and a property of it.  A level added later is a row of this table, a section of
// the page (`lvl-<id>`) and a view drawn into it; nothing in a view says how it comes on screen.

/**
 * Every level the panel can show: its depth in the path, and the kind of document it is a level of.  The Add step is a level for every
 * folder it walks (`folders`: Places, a place, a folder of it, each one deeper), and a step from one folder to the next is a step between
 * two sections of the page, so it has two (`add` and `add2`, taken in turn); `levelDepth` is the one place that says how deep a level is.
 */
export const LEVELS = {
  tile: { depth: 0, doc: 'tile' },
  entry: { depth: 1, doc: 'tile' },
  control: { depth: 2, doc: 'tile' },
  map: { depth: 0, doc: 'map' },
  add: { depth: 1, doc: 'map', folders: true },
  add2: { depth: 1, doc: 'map', folders: true },
  piece: { depth: 1, doc: 'map' },
  prop: { depth: 2, doc: 'map' },
};

/** The level of a tile at selection `sel` with `control` open. */
export const tileLevelOf = (sel, control = null) => ['tile', 'entry', 'control'][depthOf(sel, control)];

/** The sections of the Add step: its folders take them in turn, so a step from one folder to the next is a step between two levels. */
export const ADD_PANES = ['add', 'add2'];

/** The level of a map with `pieces` selected (a count), a property open on them (`prop`), and the Add step open (`add`: the section its folder is in, 0 or 1, else null; only while no piece is selected). */
export const mapLevelOf = ({ pieces = 0, prop = null, add = null }) => (prop && pieces === 1 ? 'prop' : pieces ? 'piece' : add != null ? ADD_PANES[add] : 'map');

/**
 * How deep level `id` is in its document (a row of LEVELS), and for the Add step the folder it is at (`{root, dir}`, as `folderDepth`):
 * Add is a step below the map, and each folder it walks is one deeper than the one it came from.  Every depth the shell, the crumbs and
 * the slide use comes from here.
 */
export const levelDepth = (id, folder = null) => LEVELS[id].depth + (LEVELS[id].folders ? folderDepth(folder) : 0);

/**
 * Which way the panel slides from level `from` to level `to` (each an id, or `{id, folder}` for a level of the Add step): 1 a deeper
 * level comes in from the right, -1 a shallower one comes in from the left, 0 no slide (the same level, another level of the same
 * depth, no level before, or a level of another kind of document).  A jump of several levels slides the same one step's distance; it is
 * the depth that says the way.
 */
export function slideBetween(from, to) {
  const f = typeof from === 'string' ? { id: from } : from, t = typeof to === 'string' ? { id: to } : to;
  const a = LEVELS[f?.id], b = LEVELS[t?.id];
  if (!a || !b || a.doc !== b.doc) return 0;
  return slideDir(levelDepth(f.id, f.folder), levelDepth(t.id, t.folder));
}

/** The one direction rule of every slide: from depth `a` to depth `b` of the trail, 1 deeper (in from the right), -1 shallower (in from the left), 0 the same depth (a swap). */
export const slideDir = (a, b) => Math.sign(b - a);

/** How deep a folder level is: Places 0, a place 1, a folder in it 2, its subfolder 3. */
export const folderDepth = (l) => (l?.root == null ? 0 : 1 + (l.dir ? l.dir.split('/').length : 0));

/**
 * How deep a document's level is in the whole trail (Places › a place › folders › the tile › a thing › a control): the depth of the
 * folder it is in, one for the document, and its own level below that.  A folder and a document are steps of the same trail, so the step
 * between them slides by the same rule: opening a tile from its folder is one deeper, Up from it one shallower.
 */
export const trailDepth = (folder, levelDepth = 0) => folderDepth(folder) + 1 + levelDepth;

/** The key a level keeps its scroll position under. */
export function levelKey(sel, control = null) {
  if (!entryLike(sel)) return 'tile';
  const at = `${sel.kind}:${sel.id}`;
  return control ? `${at}/${control.name}@${control.group ?? 0}` : at;
}

/** Is `a` the same level as `b`? (A piece of an entry is the entry's level.) */
export const sameLevel = (a, b) => levelKey(a.sel, a.control) === levelKey(b.sel, b.control);

/** The level one step up from `{sel, control}`: a control goes to its thing, a thing to the tile. */
export function up(level, tileSel) {
  if (level.control) return { sel: level.sel, control: null };
  if (entryLike(level.sel)) return { sel: tileSel, control: null };
  return null;
}

// ---- the English of it -------------------------------------------------------

/** 1st, 2nd, 3rd, 4th, 11th, 22nd. */
export function ordinal(n) {
  const r = n % 100;
  if (r >= 11 && r <= 13) return `${n}th`;
  return `${n}${{ 1: 'st', 2: 'nd', 3: 'rd' }[n % 10] ?? 'th'}`;
}

/** "1x1" or `"2x1"` (as written) as 1×1. */
export const sizeWords = (written) => String(written ?? '').replace(/"/g, '').replace(/x/i, '×');

const SYSTEM = { db: 'DungeonBlocks', ol: 'OpenLOCK' };
export const systemName = (s) => SYSTEM[s] ?? s;

// ---- the stack as the tile level lists it -------------------------------------

/**
 * The stack in the order it is written, which is the order it builds.  The program lists the
 * base (the connector) last, for the mesh's sake; here it stands where the source has it.
 */
export function orderStack(items) {
  const base = items.findIndex((it) => it.element === 'connector' && it.lines);
  if (base < 0 || base !== items.length - 1) return items;
  const b = items[base], rest = items.slice(0, base);
  const at = rest.findIndex((it) => it.file === b.file && it.lines && it.lines[0] > b.lines[0]);
  return at < 0 ? items : [...rest.slice(0, at), b, ...rest.slice(at)];
}

/** `at=bank` and the like: the one phrase that says where an entry is put (what is written, else nothing). */
export function placeOf(it) {
  const p = (it.place || []).find((x) => x.name === 'at');
  return p ? `at=${p.text}` : '';
}

/** Where entry `key` stands in the stack as listed: {n, of} (1-based), or null. */
export function standing(items, key) {
  const i = items.findIndex((it) => it.key === key);
  return i < 0 ? null : { n: i + 1, of: items.length };
}

/** The badge a stack row wears: its messages by key, worst first, as {tone, n, title} (null when it has none). */
export function badgeOf(messages) {
  if (!messages?.length) return null;
  const errs = messages.filter((m) => m.severity === 'error');
  const tone = errs.length ? 'error' : 'note';
  return { tone, n: messages.length, title: messages.map((m) => m.text).filter(Boolean).slice(0, 3).join('\n') };
}

/** Which row an arrow key moves to (the list does not wrap): the index, or `i` when it cannot. */
export function moveRow(i, key, count) {
  if (!count) return -1;
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  if (key === 'ArrowDown') return Math.min(count - 1, i < 0 ? 0 : i + 1);
  if (key === 'ArrowUp') return Math.max(0, i < 0 ? count - 1 : i - 1);
  return i;
}

// ---- the crumbs and the strip --------------------------------------------------

/** The crumbs of a folder path: `Places`, the place (`root`), a crumb for each folder below it (`dir`), each saying where it goes (`place`: `{root, dir}`; null for Places). */
function folderCrumbs(place) {
  const out = [{ id: 'places', label: 'Places', depth: -1, place: null }];
  if (place) {
    out.push({ id: 'place', label: place.label, depth: -1, place: { root: place.root, dir: '' } });
    const names = place.dir ? place.dir.split('/') : [];
    names.forEach((n, i) => out.push({ id: `dir:${i}`, label: n, depth: -1, place: { root: place.root, dir: names.slice(0, i + 1).join('/') } }));
  }
  return out;
}

/**
 * The crumbs over the panel, each `{id, label, depth, current}`: `Places`, the place (`root`), a crumb
 * for each folder below it (`dir`), then, when a tile is open (`tileName`), the tile, the thing and the
 * control the level is at (`what` names the thing: an entry's label, a line's or region's name).  A crumb
 * above the tile has `depth` -1, and says where it goes: `{root, dir}` (none for Places).  On a folder
 * level (`tileName` null) the last folder crumb is the current one.
 * `place` is `{root, label, dir}` (`dir` as `a/b`), or null at the top.  The levels below the tile are the same
 * for a tile and for a map: `depth` 1 is the thing (`what`: a stack entry, a line or region, or the map's
 * pieces) and 2 one control of it (`control`, a name: a tile's control, or a piece's property).
 * On a map's Add step (`add`: the folder it is at, `{root, label, dir}`, or null at Places) the crumbs below the map are `Add` and the
 * folders it walks, Places first; each says where it goes (`add`: `{root, dir}`), at the depth `levelDepth` gives it, and the last is current.
 */
export function crumbsFor({ place = null, tileName = null, depth = 0, what = '', control = '', add = undefined }) {
  const out = folderCrumbs(place);
  if (tileName == null) return out.map((c, i) => ({ ...c, current: i === out.length - 1 }));
  out.push({ id: 'tile', label: tileName, depth: 0 });
  if (add !== undefined) {
    const top = { root: null, dir: '' };
    out.push({ id: 'add', label: 'Add', depth: levelDepth('add', top), add: top });
    for (const c of folderCrumbs(add)) {
      const at = c.place ?? top;
      out.push({ id: `add:${c.id}`, label: c.label, depth: levelDepth('add', at), add: at });
    }
    return out.map((c, i) => ({ ...c, current: i === out.length - 1 }));
  }
  if (depth >= 1) out.push({ id: 'thing', label: what, depth: 1 });
  if (depth >= 2) out.push({ id: 'control', label: control, depth: 2 });
  return out.map((c) => ({ ...c, current: c.depth === depth }));
}

/**
 * The strip under the crumbs: up to two short lines about what the level sits in, never the
 * parent's own panel.  `tile` is `{entries, lines, regions}` (the name is the crumb's, the size and seed the tile level's rows, the target the title bar's chip: each fact has one home); `entry` is
 * `{standing: {n, of}, place}` for a stack entry, or `{note}` for a line or a region; `value` is
 * the control's current value at depth 2.
 */
export function contextFor({ depth, tile, entry = null, thing = '', control = null, value = '' }) {
  const t = tile || {};
  if (depth === 0) {
    const n = (k, one, many = `${one}s`) => `${k} ${k === 1 ? one : many}`;
    const holds = [n(t.entries ?? 0, 'entry', 'entries'), t.lines ? n(t.lines, 'line') : '', t.regions ? n(t.regions, 'region') : ''].filter(Boolean).join(' · ');
    return [holds].filter(Boolean);
  }
  const where = entry?.standing ? `${ordinal(entry.standing.n)} of ${entry.standing.of} in the stack${entry.place ? ` · ${entry.place}` : ''}` : entry?.note || '';
  if (depth === 1) return [where].filter(Boolean);
  return [[thing, where].filter(Boolean).join(' · '), control ? `${control} = ${value || 'not set'}` : ''].filter(Boolean);
}

// ---- the address -----------------------------------------------------------------

const slashy = (s) => encodeURIComponent(s).replace(/%2F/g, '/').replace(/%3A/g, ':');

/** `at=` for a selection: an entry's key, `region:name`, `line:name`; null for the tile. */
export function atOf(sel) {
  if (!entryLike(sel)) return null;
  return sel.kind === 'entry' ? sel.id : `${sel.kind}:${sel.id}`;
}

/** The selection an `at=` names (the tile for none), or null when it names nothing. */
export function selOfAt(at) {
  if (at == null || at === '' || at === 'tile') return { kind: 'tile' };
  const m = at.match(/^(region|line):(.+)$/);
  if (m) return { kind: m[1], id: m[2] };
  return { kind: 'entry', id: at };
}

/**
 * The address as the part of the page's URL hash it owns: `at`, `piece`, `control`, `group`, `face` for a tile, and
 * `pieces` (the ids, comma by comma), `prop`, and `add` and `adir` (the folder the Add step is at: a place's key, empty at Places, and the
 * folder below it) for a map's levels.  `add` is that folder as `{place, dir}` (folders.js `placeParams`), or null.  (A folder level's,
 * `place` and `dir`, is js/folders.js.)
 */
export function levelParams({ sel, control, face, pieces = null, prop = null, add = null }) {
  const p = {};
  if (pieces?.length) {
    p.pieces = pieces.join(',');
    if (prop) p.prop = prop;
  } else if (add) {
    p.add = add.place;
    if (add.dir) p.adir = add.dir;
  }
  const at = atOf(sel);
  if (at) {
    p.at = at;
    if (sel.piece != null) p.piece = String(sel.piece);
    if (control) { p.control = control.name; if (control.group) p.group = String(control.group); }
  }
  if (face === 'source') p.face = 'source';
  return p;
}

/** A URL hash as its pairs. */
export function parseHash(hash) {
  const out = {};
  for (const part of String(hash || '').replace(/^#/, '').split('&')) {
    if (!part) continue;
    const i = part.indexOf('=');
    const k = i < 0 ? part : part.slice(0, i), v = i < 0 ? '' : part.slice(i + 1);
    try { out[k] = decodeURIComponent(v); } catch { out[k] = v; }
  }
  return out;
}

/** Pairs as a hash; `tile` first, then `at` and the rest in a fixed order, then anything else kept (theme, paint). */
export function buildHash(pairs) {
  const order = ['tile', 'place', 'dir', 'at', 'piece', 'control', 'group', 'pieces', 'prop', 'add', 'adir', 'face'];
  const keys = [...order.filter((k) => pairs[k] != null), ...Object.keys(pairs).filter((k) => !order.includes(k) && pairs[k] != null)];
  return `#${keys.map((k) => (k === 'tile' ? `tile=${encodeURIComponent(pairs[k])}` : `${k}=${slashy(String(pairs[k]))}`)).join('&')}`;
}

/** The pairs of the hash that the address owns: what is written over, and nothing else (the theme and the like stay). */
export const OWNED = ['tile', 'place', 'dir', 'at', 'piece', 'control', 'group', 'pieces', 'prop', 'add', 'adir', 'face'];

/** The key of the level an address (its pairs) is at: a new key is a new level, the same key another piece of the same thing. */
export function addressLevel(p) {
  if (p.pieces) return `pieces/${p.prop || ''}`;
  if (p.add != null) return `add/${p.add}/${p.adir || ''}`;
  return `${p.at || ''}|${p.control || ''}|${p.group || ''}`;
}

/**
 * Does writing `next` over the address `cur` (both as pairs) take a step of the browser's history, or replace the
 * one it is on?  A new level is a step: another folder level (the Add step's too), a folder into a document (Back goes to the folder), a
 * document's new level (an entry, a control, a map's pieces, a property of them).  Anything else replaces: the same
 * level again, a step in the source, another piece of the same entry or another selection of the same level, a
 * different document (it opened in place).
 */
export function stepsHistory(cur, next) {
  const kind = (p) => (p.tile != null ? 'tile' : p.place != null ? 'folder' : 'none');
  const from = kind(cur), to = kind(next);
  if (to === 'folder') return from === 'tile' || (from === 'folder' && (cur.place !== next.place || (cur.dir || '') !== (next.dir || '')));
  if (to === 'tile') return from === 'folder' || (from === 'tile' && cur.tile === next.tile && next.face !== 'source' && addressLevel(cur) !== addressLevel(next));
  return false;
}

/** The level a hash asks for: {sel, control, face} (sel the tile when it names none), and for a map `pieces` (ids) and `prop`, or `add` (the folder of the Add step, `{place, dir}`: the place's key, which the page resolves). */
export function levelOfHash(hash) {
  const h = parseHash(hash);
  const sel = selOfAt(h.at);
  if (sel.kind === 'entry' && h.piece != null && /^\d+$/.test(h.piece)) sel.piece = Number(h.piece);
  const control = sel.kind !== 'tile' && h.control ? { name: h.control, group: /^\d+$/.test(h.group || '') ? Number(h.group) : 0 } : null;
  const out = { sel, control, face: h.face === 'source' ? 'source' : 'tile' };
  const pieces = (h.pieces || '').split(',').filter(Boolean);
  if (pieces.length) {
    out.pieces = pieces;
    if (['bottom', 'seed', 'print'].includes(h.prop)) out.prop = h.prop;
  } else if (h.add != null) out.add = { place: h.add, dir: h.adir || '' };
  return out;
}

// ---- source and levels --------------------------------------------------------------
//
// The level is the selection, and the source's caret selects (the entry written on its line, else the
// tile), so flipping back from the source needs no translation: the level is the caret's thing, at that
// thing's own level, and a control stays open only while its thing stays selected (editor/selection.js).

/** The line the source should put its caret on for a level: the first line of the selection, or 1 for the tile. */
export const caretLineFor = (lines) => (lines && lines[0] > 0 ? lines[0] : null);
