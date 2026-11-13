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
// folder level is `#place=<place>&dir=<folder>` (no `tile=`).

const entryLike = (s) => s?.kind === 'entry' || s?.kind === 'region' || s?.kind === 'line';

/** How deep `sel` (and the open `control`) is: 0 the tile, 1 a thing of it, 2 one control of that. */
export function depthOf(sel, control = null) {
  if (!entryLike(sel)) return 0;
  return control ? 2 : 1;
}

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

/**
 * The crumbs over the panel, each `{id, label, depth, current}`: `Places`, the place (`root`), a crumb
 * for each folder below it (`dir`), then, when a tile is open (`tileName`), the tile, the thing and the
 * control the level is at (`what` names the thing: an entry's label, a line's or region's name).  A crumb
 * above the tile has `depth` -1, and says where it goes: `{root, dir}` (none for Places).  On a folder
 * level (`tileName` null) the last folder crumb is the current one.
 * `place` is `{root, label, dir}` (`dir` as `a/b`), or null at the top.  The levels below the tile are the same
 * for a tile and for a map: `depth` 1 is the thing (`what`: a stack entry, a line or region, or the map's
 * pieces) and 2 one control of it (`control`, a name: a tile's control, or a piece's property).
 */
export function crumbsFor({ place = null, tileName = null, depth = 0, what = '', control = '' }) {
  const out = [{ id: 'places', label: 'Places', depth: -1, place: null }];
  if (place) {
    out.push({ id: 'place', label: place.label, depth: -1, place: { root: place.root, dir: '' } });
    const names = place.dir ? place.dir.split('/') : [];
    names.forEach((n, i) => out.push({ id: `dir:${i}`, label: n, depth: -1, place: { root: place.root, dir: names.slice(0, i + 1).join('/') } }));
  }
  if (tileName == null) return out.map((c, i) => ({ ...c, current: i === out.length - 1 }));
  out.push({ id: 'tile', label: tileName, depth: 0 });
  if (depth >= 1) out.push({ id: 'thing', label: what, depth: 1 });
  if (depth >= 2) out.push({ id: 'control', label: control, depth: 2 });
  return out.map((c) => ({ ...c, current: c.depth === depth }));
}

/**
 * The strip under the crumbs: up to two short lines about what the level sits in, never the
 * parent's own panel.  `tile` is `{name, size, seed, system, entries, lines, regions}`; `entry` is
 * `{standing: {n, of}, place}` for a stack entry, or `{note}` for a line or a region; `value` is
 * the control's current value at depth 2.
 */
export function contextFor({ depth, tile, entry = null, thing = '', control = null, value = '' }) {
  const t = tile || {};
  const tileLine = [t.size && sizeWords(t.size), t.seed != null && `seed ${t.seed}`, t.system].filter(Boolean).join(' · ');
  if (depth === 0) {
    const n = (k, one, many = `${one}s`) => `${k} ${k === 1 ? one : many}`;
    const holds = [n(t.entries ?? 0, 'entry', 'entries'), t.lines ? n(t.lines, 'line') : '', t.regions ? n(t.regions, 'region') : ''].filter(Boolean).join(' · ');
    return [holds, [t.system, t.printer].filter(Boolean).join(' · ')].filter(Boolean);
  }
  const where = entry?.standing ? `${ordinal(entry.standing.n)} of ${entry.standing.of} in the stack${entry.place ? ` · ${entry.place}` : ''}` : entry?.note || '';
  if (depth === 1) return [[t.name, tileLine].filter(Boolean).join(' · '), where].filter(Boolean);
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
 * `pieces` (the ids, comma by comma) and `prop` for a map's levels.  (A folder level's, `place` and `dir`, is js/folders.js.)
 */
export function levelParams({ sel, control, face, pieces = null, prop = null }) {
  const p = {};
  if (pieces?.length) {
    p.pieces = pieces.join(',');
    if (prop) p.prop = prop;
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
  const order = ['tile', 'place', 'dir', 'at', 'piece', 'control', 'group', 'pieces', 'prop', 'face'];
  const keys = [...order.filter((k) => pairs[k] != null), ...Object.keys(pairs).filter((k) => !order.includes(k) && pairs[k] != null)];
  return `#${keys.map((k) => (k === 'tile' ? `tile=${encodeURIComponent(pairs[k])}` : `${k}=${slashy(String(pairs[k]))}`)).join('&')}`;
}

/** The pairs of the hash that the address owns: what is written over, and nothing else (the theme and the like stay). */
export const OWNED = ['tile', 'place', 'dir', 'at', 'piece', 'control', 'group', 'pieces', 'prop', 'face'];

/** The key of the level an address (its pairs) is at: a new key is a new level, the same key another piece of the same thing. */
export function addressLevel(p) {
  if (p.pieces) return `pieces/${p.prop || ''}`;
  return `${p.at || ''}|${p.control || ''}|${p.group || ''}`;
}

/**
 * Does writing `next` over the address `cur` (both as pairs) take a step of the browser's history, or replace the
 * one it is on?  A new level is a step: another folder level, a folder into a document (Back goes to the folder), a
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

/** The level a hash asks for: {sel, control, face} (sel the tile when it names none), and for a map `pieces` (ids) and `prop`. */
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
  }
  return out;
}

// ---- source and levels --------------------------------------------------------------
//
// The level is the selection, and the source's caret selects (the entry written on its line, else the
// tile), so flipping back from the source needs no translation: the level is the caret's thing, at that
// thing's own level, and a control stays open only while its thing stays selected (editor/selection.js).

/** The line the source should put its caret on for a level: the first line of the selection, or 1 for the tile. */
export const caretLineFor = (lines) => (lines && lines[0] > 0 ? lines[0] : null);
