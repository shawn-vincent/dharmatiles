// The folder levels (docs: preview.html, "Places and folders"): the levels above the tile.
// The top is Places (My tiles, folders you added, the library, the tests); a place opens to
// a folder level listing its subfolders and then its tiles; a subfolder is another level.
// This is the model and nothing else (no page here): what the places are and in what
// order, what a folder level lists, the strip of context under the crumbs, the address of
// a level in the URL hash, and which drops of a tile are allowed.  editor/places.js draws it;
// folder-list.js builds the rows.
//
// A place is a root of the program's snapshot ({index, label, kind, path, role, writable,
// exists}); a level is `{root, dir}`: `root` the place's index (null for Places itself) and
// `dir` the folder below it, `a/b`, empty at the place.  A level is named in the address by
// the place's path, not its index (an index moves when a place is removed).

/**
 * The kinds of document a folder lists, one table: a new kind (a map, the inventory) is one more
 * entry, and the list, the rows and their words follow.  `exts` are the file extensions; `actions`
 * says the folder's actions (rename, move, copy, trash) apply; `noun` is what the strip counts.
 */
export const KINDS = [
  { kind: 'tile', exts: ['.dtile'], noun: 'tile', plural: 'tiles', actions: true },
  { kind: 'stl', exts: ['.stl'], noun: 'model', plural: 'models', actions: false },
];

/** The kind of a file (by its extension), or undefined. */
export const kindOf = (path) => {
  const p = String(path || '').toLowerCase();
  return KINDS.find((k) => k.exts.some((e) => p.endsWith(e)));
};

/** The file's name without its extension (`2x1-cliff`; an `.stl` keeps its suffix, as the menu always showed it). */
export const stemOf = (path) => String(path || '').replace(/^lib:/, '').split(/[\\/]/).pop().replace(/\.dtile$/i, '');

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const slashes = (s) => String(s || '').replace(/\\/g, '/');
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
const byName = (a, b) => collator.compare(a, b);

// ---- the places -----------------------------------------------------------------------------

const ROLE_ORDER = ['home', 'files', 'folder', 'added', 'library', 'tests'];

/** The places in the order Places lists them: My tiles, files named, folders named, folders added, the library, the tests. */
export function placesOf(roots) {
  return (roots || []).map((r, i) => ({ ...r, index: r.index ?? i }))
    .sort((a, b) => ROLE_ORDER.indexOf(a.role) - ROLE_ORDER.indexOf(b.role) || a.index - b.index);
}

/** What names a place in an address: its path (a folder's, or `lib:tiles/`), `files` for the files named. */
export const placeKey = (r) => r?.path ?? 'files';

/** The index of the place an address names, or -1. */
export const placeIndex = (roots, key) => (roots || []).findIndex((r) => placeKey(r) === key);

/** May a place's contents be changed from the page (it is a folder you own)? */
export const writable = (r) => !!r?.writable;

/** The tile paths of a place are relative to this prefix (`/Users/x/tiles/`, `lib:tiles/`), or null for the files named. */
function prefixOf(r) {
  if (!r || r.kind === 'files' || !r.path) return null;
  const p = slashes(r.path);
  return p.endsWith('/') ? p : `${p}/`;
}

/** The names a tile's path has below its place: `['dungeon', 'combo.dtile']`. */
export function relOf(r, path) {
  const p = slashes(path);
  const pre = prefixOf(r);
  if (pre == null) return [p.split('/').pop()];
  const rest = p.startsWith(pre) ? p.slice(pre.length) : p.split('/').pop();
  return rest.split('/').filter(Boolean);
}

/** Where a tile is: `{root, dir}` (its place and folder), or null for a tile no place holds. */
export function whereIs(roots, tile) {
  const r = (roots || [])[tile?.root];
  if (!r) return null;
  return { root: tile.root, dir: relOf(r, tile.path).slice(0, -1).join('/') };
}

// ---- what a folder level lists ------------------------------------------------------------------

/** Every folder path a place has: the ones the program lists (empty ones too) and the ones its tiles are in. */
function folderPaths(r, root, tiles, folders) {
  const all = new Set((folders || []).filter((f) => f.root === root).map((f) => f.path));
  for (const t of tiles || []) {
    if (t.root !== root) continue;
    const rel = relOf(r, t.path);
    for (let i = 1; i < rel.length; i++) all.add(rel.slice(0, i).join('/'));
  }
  return all;
}

/**
 * The folder level `{root, dir}`: its subfolders (`{name, dir, tiles, folders}`: the tiles under each, all
 * the way down, and its own subfolders) and the tiles in it, each folder and tile by name.  With a `query`
 * the place is searched instead: no folders, and every tile of it, at any depth, whose name or path has the
 * words in it (each carries `where`, its folder).  Null when the place is not there.
 */
export function levelOf({ roots, tiles, folders = [], root, dir = '', query = '' }) {
  const r = (roots || [])[root];
  if (!r) return null;
  const mine = (tiles || []).filter((t) => t.root === root).map((t) => ({ t, rel: relOf(r, t.path) }));
  const q = query.trim().toLowerCase();
  if (q) {
    const items = mine
      .filter(({ t, rel }) => stemOf(t.path).toLowerCase().includes(q) || rel.join('/').toLowerCase().includes(q))
      .map(({ t, rel }) => ({ tile: t, where: rel.slice(0, -1).join('/') }))
      .sort((a, b) => byName(a.where, b.where) || byName(stemOf(a.tile.path), stemOf(b.tile.path)));
    return { place: r, dir, folders: [], items, total: items.length, here: items.length };
  }
  const base = dir ? `${dir}/` : '';
  const all = folderPaths(r, root, tiles, folders);
  const subs = new Map();
  for (const f of all) {
    if (!f.startsWith(base)) continue;
    const rest = f.slice(base.length);
    if (!rest) continue;
    const name = rest.split('/')[0];
    if (!subs.has(name)) subs.set(name, { name, dir: `${base}${name}`, tiles: 0, folders: 0 });
    if (!rest.includes('/')) continue;
    // a deeper path: it is a folder of the sub
    if (rest.split('/').length === 2) subs.get(name).folders += 1;
  }
  const items = [];
  for (const { t, rel } of mine) {
    const at = rel.slice(0, -1).join('/');
    if (at === dir) items.push({ tile: t, where: dir });
    else if (at.startsWith(base)) {
      const s = subs.get(at.slice(base.length).split('/')[0]);
      if (s) s.tiles += 1;
    }
  }
  items.sort((a, b) => byName(stemOf(a.tile.path), stemOf(b.tile.path)));
  const list = [...subs.values()].sort((a, b) => byName(a.name, b.name));
  const total = items.length + list.reduce((n, f) => n + f.tiles, 0);
  return { place: r, dir, folders: list, items, total, here: items.length };
}

/** Does the level `{root, dir}` exist (the place is listed, and the folder is one of it)? */
export function levelExists({ roots, tiles, folders = [], root, dir = '' }) {
  const r = (roots || [])[root];
  if (!r) return false;
  return !dir || folderPaths(r, root, tiles, folders).has(dir);
}

/** The folder one level up from `{root, dir}`: its parent folder, else the place, else Places (null). */
export function parentLevel({ root, dir = '' }) {
  if (root == null) return null;
  if (!dir) return { root: null, dir: '' };
  const i = dir.lastIndexOf('/');
  return { root, dir: i < 0 ? '' : dir.slice(0, i) };
}

// ---- the words ------------------------------------------------------------------------------------

/** `/Users/me/dharmatiles/dungeon` as `~/dharmatiles/dungeon`; a long one keeps its end: `~/…/dungeon/combo`. */
export function shortPath(path, home, max = 38) {
  let p = slashes(path);
  const h = slashes(home).replace(/\/$/, '');
  const homed = h && (p === h || p.startsWith(`${h}/`));
  if (homed) p = `~${p.slice(h.length)}`;
  if (p.length <= max) return p;
  const parts = p.split('/');
  const keep = parts.slice(-2).join('/');
  return `${homed ? '~/' : parts[0] === '' ? '/' : ''}…/${keep}`;
}

/** Where a folder level is on disk, as a short path (a library group as `library · tiles/ground`). */
export function wherePath(r, dir, home) {
  if (!r) return '';
  if (r.kind === 'dir') return shortPath(dir ? `${slashes(r.path)}/${dir}` : r.path, home);
  if (r.kind === 'library') {
    const base = slashes(r.path).replace(/^lib:/, '').replace(/\/$/, '');
    return `${r.role === 'tests' ? 'test tiles' : 'library'} · ${dir ? `${base}/${dir}` : base}`;
  }
  return 'named when dharmatiles started';
}

/** The strip under the crumbs on a folder level: up to two short lines. */
export function contextForFolder({ roots, level, view, home }) {
  if (level.root == null) {
    const n = (roots || []).length;
    return [plural(n, 'place'), 'your tiles, folders you added, the library'];
  }
  const r = (roots || [])[level.root];
  if (!r) return [];
  const v = view;
  const counts = [];
  if (v) {
    counts.push(plural(v.total, 'tile'));
    if (v.here > 0 && v.here !== v.total) counts[0] = `${plural(v.total, 'tile')} (${v.here} here)`;
    if (v.folders.length) counts.push(plural(v.folders.length, 'folder'));
  }
  const where = wherePath(r, level.dir, home);
  const note = !writable(r) ? ' · read only' : r.exists === false ? ' · not there yet' : '';
  return [counts.join(' · '), `${where}${note}`].filter(Boolean);
}

/** A tile's row: its size (`2×1`, from the build's plan once there is one, else what the document writes). */
export function sizeLine(t) {
  const p = t?.plan;
  if (p?.cols && p?.rows) return `${p.cols}×${p.rows}`;
  return String(t?.size || '').replace(/x/i, '×');
}

/** The size as squares across and up, for a thumbnail: [cols, rows] or null. */
export function sizeSquares(t) {
  const p = t?.plan;
  if (p?.cols && p?.rows) return [p.cols, p.rows];
  const m = /^(\d+)x(\d+)$/i.exec(String(t?.size || ''));
  return m ? [Number(m[1]), Number(m[2])] : null;
}

// ---- the address ------------------------------------------------------------------------------------

/** The hash pairs of a folder level: `place` (its key; empty at Places) and `dir`. */
export function placeParams(roots, { root, dir = '' }) {
  if (root == null || !roots?.[root]) return { place: '' };
  const p = { place: placeKey(roots[root]) };
  if (dir) p.dir = dir;
  return p;
}

/**
 * The folder level a parsed hash asks for: `{root, dir}` (root null for Places), or null when the hash
 * asks for no folder level (it names a tile, or nothing).  A place the program does not list gives
 * `{missing: key}` so the page can say so.
 */
export function levelOfParams(roots, h) {
  if (h.tile != null || !('place' in h)) return null;
  if (h.place === '') return { root: null, dir: '' };
  const root = placeIndex(roots, h.place);
  if (root < 0) return { missing: h.place };
  return { root, dir: slashes(h.dir || '').replace(/^\/+|\/+$/g, '') };
}

// ---- what may be dropped where ------------------------------------------------------------------------

/**
 * May the tile `t` be dropped on `target` (`{root, dir}`, a folder row or a crumb)?  Not into the folder
 * it is in, nor a place that is read only.  The program checks all of it again: this is only what the
 * page offers, and the reason a target that cannot take it is not lit.
 */
export function canDrop(roots, t, target) {
  if (!t || !target || target.root == null) return { ok: false, why: 'not a folder' };
  const to = (roots || [])[target.root];
  if (!writable(to)) return { ok: false, why: `${to?.label ?? 'that place'} is read only` };
  if (!kindOf(t.path)?.actions) return { ok: false, why: 'only a tile is moved' };
  const from = (roots || [])[t.root];
  if (!writable(from)) return { ok: false, why: 'a library tile is copied, not moved' };
  const here = whereIs(roots, t);
  if (here && here.root === target.root && here.dir === (target.dir || '')) return { ok: false, why: 'already in this folder' };
  return { ok: true };
}

/** The folders a tile can move to, for its menu: every folder of every place you own, each `{root, dir, label, place}`, the place itself first. */
export function moveTargets(roots, folders, t) {
  const out = [];
  for (const r of placesOf(roots)) {
    if (!writable(r)) continue;
    const dirs = [''].concat((folders || []).filter((f) => f.root === r.index).map((f) => f.path).sort(byName));
    for (const dir of dirs) {
      const target = { root: r.index, dir };
      if (canDrop(roots, t, target).ok) out.push({ ...target, label: dir || r.label, place: r.label });
    }
  }
  return out;
}

/** After a folder is renamed, the folder level `dir` (below it, or itself) with its new name: the dir it is now. */
export function renamedDir(dir, from, to) {
  if (dir === from) return to;
  return dir.startsWith(`${from}/`) ? `${to}${dir.slice(from.length)}` : dir;
}
