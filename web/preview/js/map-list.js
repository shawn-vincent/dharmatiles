// The map's list of pieces, as the panel shows it (docs/maps.html): the pieces in three sections (placed, not placed,
// left off), a row for each tile (a piece of the tray is a row of its own), under a sort, a filter and a search.
// The model and nothing else (no DOM), so it is tested alone (tests/map-list.test.js).  What the shelf says of a
// piece is the program's word (`piece.shelf`, read through `marks` in map-model.js); this only arranges it.

/** @typedef {import('./api').ScenePiece} ScenePiece */
/** @typedef {Pick<ScenePiece, 'id' | 'canon' | 'name' | 'at'> & { count?: number }} Listed  what the list reads of a piece */
/** @typedef {Pick<ScenePiece, 'shelf'>['shelf']} Shelf */
/** @typedef {Map<string, { state: string }>} Marks  what the shelf says of each piece (map-model.js `marks`) */
/** @typedef {{ sort?: string, filter?: string, query?: string }} How  a list's sort, filter and search */
/**
 * @template {Listed} P
 * @typedef {{ key: string, name: string, canon: string, pieces: P[], ids: string[], firstId: string, count: number, have: number, toPrint: number, other: number, skip: number }} Row  a tile's row of the list
 */

/**
 * The sorts, as [value, text, title].
 * @type {[string, string, string][]}
 */
export const SORTS = [
  ['name', 'Name', 'By the tile’s name'],
  ['count', 'Count', 'The tiles with the most pieces first'],
  ['print', 'To print first', 'The tiles with the most pieces to print first'],
];
/**
 * The filters, as [value, text, title].
 * @type {[string, string, string][]}
 */
export const FILTERS = [
  ['all', 'All', 'Every piece'],
  ['print', 'To print', 'The pieces that print'],
  ['have', 'Have', 'The pieces a print you own stands for'],
  ['other', 'Other colour', 'The pieces you own only in the other colour (they print)'],
];
/** What a map's list shows until its user chooses: by name, every piece, no search. */
export const DEFAULT_LIST = Object.freeze({ sort: 'name', filter: 'all', query: '' });

/** What the shelf says of a piece (a piece the program has no word for prints). */
export const stateOf = (/** @type {{ id: string }} */ p, /** @type {Marks} */ marks) => marks.get(p.id)?.state ?? 'need';

const passes = (/** @type {string} */ state, /** @type {string} */ filter) => {
  if (filter === 'print') return state === 'need' || state === 'other';
  if (filter === 'have') return state === 'have';
  if (filter === 'other') return state === 'other';
  return true;
};

/** Does a piece's tile name have the words of the search (every word, in any order, ignoring case)? */
export const matches = (/** @type {{ name?: string }} */ p, /** @type {string | null | undefined} */ query) => {
  const words = String(query ?? '').toLowerCase().split(/\s+/).filter(Boolean);
  const name = String(p.name ?? '').toLowerCase();
  return words.every((w) => name.includes(w));
};

const byName = (/** @type {{ name: string, firstId: string }} */ a, /** @type {{ name: string, firstId: string }} */ b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }) || a.name.localeCompare(b.name) || (a.firstId < b.firstId ? -1 : a.firstId > b.firstId ? 1 : 0);

/** The order of two rows under a sort; ties fall back to the name, then the id of the row's first piece, so a sort never shuffles. */
export const compare = (/** @type {string} */ sort) => (/** @type {Row<any>} */ a, /** @type {Row<any>} */ b) => {
  if (sort === 'count') return b.count - a.count || byName(a, b);
  if (sort === 'print') return b.toPrint - a.toPrint || byName(a, b);
  return byName(a, b);
};

/**
 * @template {Listed} P
 * @param {string} key @param {P[]} ps @param {Marks} marks
 * @returns {Row<P>}
 */
const rowOf = (key, ps, marks) => {
  const states = ps.map((p) => stateOf(p, marks));
  const n = (/** @type {string} */ s) => states.filter((x) => x === s).length;
  return {
    key, name: ps[0].name, canon: ps[0].canon, pieces: ps, ids: ps.map((p) => p.id),
    firstId: String(ps[0].id ?? ''),
    count: ps.reduce((k, p) => k + (p.at ? 1 : (p.count ?? 1)), 0),
    have: n('have'), toPrint: n('need') + n('other'), other: n('other'), skip: n('skip'),
  };
};

/**
 * The map's pieces as the panel lists them: `{placed, tray, off, shown}`, each section a sorted list of rows
 * `{key, name, canon, pieces, ids, count, have, toPrint, other, skip}`.  A piece left off (the shelf says `skip`)
 * is in `off` wherever it stands; of the rest, those with a place are `placed` (a row per tile) and those without
 * are in the `tray` (a row per piece, whose `count` is its copies).  The filter and the search take pieces out
 * before the rows are made, so a row says only the pieces that pass.  `shown` is how many pieces are listed.
 * @template {Listed} P
 * @param {P[]} pieces  the map's pieces
 * @param {Marks} marks  what the shelf says of each (map-model.js `marks`)
 * @param {How} [how]
 */
export function sectionsOf(pieces, marks, how = DEFAULT_LIST) {
  const { sort = 'name', filter = 'all', query = '' } = how;
  const kept = pieces.filter((p) => passes(stateOf(p, marks), filter) && matches(p, query));
  const groups = (/** @type {P[]} */ ps) => {
    /** @type {Map<string, P[]>} */
    const by = new Map();
    for (const p of ps) { let g = by.get(p.canon); if (!g) { g = []; by.set(p.canon, g); } g.push(p); }
    return [...by].map(([canon, g]) => rowOf(canon, g, marks));
  };
  const off = kept.filter((p) => stateOf(p, marks) === 'skip');
  const live = kept.filter((p) => stateOf(p, marks) !== 'skip');
  const cmp = compare(sort);
  return {
    placed: groups(live.filter((p) => p.at)).sort(cmp),
    tray: live.filter((p) => !p.at).map((p) => rowOf(p.id, [p], marks)).sort(cmp),
    off: groups(off).sort(cmp),
    shown: kept.length,
  };
}

/** Do the pills float over the pieces in the view?  Only where the user turned them on (a stored '1'): a missing value, and the old default's '0', are off, because the list in the panel says the shelf's words. */
export const pillsWanted = (/** @type {string | null} */ stored) => stored === '1';

/** A stored choice of sort and filter, read back as a list view: anything else is the default (a stale or hand-edited value never breaks the panel). */
/** @param {How | null | undefined} stored */
export function listFrom(stored) {
  const how = stored ?? {};
  const sort = SORTS.some(([v]) => v === how.sort) ? /** @type {string} */ (how.sort) : DEFAULT_LIST.sort;
  const filter = FILTERS.some(([v]) => v === how.filter) ? /** @type {string} */ (how.filter) : DEFAULT_LIST.filter;
  return { sort, filter, query: typeof how.query === 'string' ? how.query : '' };
}
