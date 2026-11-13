// The map's list of pieces, as the panel shows it (docs/maps.html): the pieces in three sections (placed, not placed,
// left off), a row for each tile (a piece of the tray is a row of its own), under a sort, a filter and a search.
// The model and nothing else (no DOM), so it is tested alone (tests/map-list.test.js).  What the shelf says of a
// piece is the program's word (`piece.shelf`, read through `marks` in map-model.js); this only arranges it.

/** The sorts, as [value, text, title]. */
export const SORTS = [
  ['name', 'Name', 'By the tile’s name'],
  ['count', 'Count', 'The tiles with the most pieces first'],
  ['print', 'To print first', 'The tiles with the most pieces to print first'],
];
/** The filters, as [value, text, title]. */
export const FILTERS = [
  ['all', 'All', 'Every piece'],
  ['print', 'To print', 'The pieces that print'],
  ['have', 'Have', 'The pieces a print you own stands for'],
  ['other', 'Other colour', 'The pieces you own only in the other colour mode (they print)'],
];
/** What a map's list shows until its user chooses: by name, every piece, no search. */
export const DEFAULT_LIST = Object.freeze({ sort: 'name', filter: 'all', query: '' });

/** What the shelf says of a piece (a piece the program has no word for prints). */
export const stateOf = (p, marks) => marks.get(p.id)?.state ?? 'need';

const passes = (state, filter) => {
  if (filter === 'print') return state === 'need' || state === 'other';
  if (filter === 'have') return state === 'have';
  if (filter === 'other') return state === 'other';
  return true;
};

/** Does a piece's tile name have the words of the search (every word, in any order, ignoring case)? */
export const matches = (p, query) => {
  const words = String(query ?? '').toLowerCase().split(/\s+/).filter(Boolean);
  const name = String(p.name ?? '').toLowerCase();
  return words.every((w) => name.includes(w));
};

const byName = (a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }) || a.name.localeCompare(b.name) || (a.firstId < b.firstId ? -1 : a.firstId > b.firstId ? 1 : 0);

/** The order of two rows under a sort; ties fall back to the name, then the id of the row's first piece, so a sort never shuffles. */
export const compare = (sort) => (a, b) => {
  if (sort === 'count') return b.count - a.count || byName(a, b);
  if (sort === 'print') return b.toPrint - a.toPrint || byName(a, b);
  return byName(a, b);
};

const rowOf = (key, ps, marks) => {
  const states = ps.map((p) => stateOf(p, marks));
  const n = (s) => states.filter((x) => x === s).length;
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
 * @param {any[]} pieces  the map's pieces
 * @param {Map<string, {state: string}>} marks  what the shelf says of each (map-model.js `marks`)
 * @param {{sort?: string, filter?: string, query?: string}} [how]
 */
export function sectionsOf(pieces, marks, how = DEFAULT_LIST) {
  const { sort = 'name', filter = 'all', query = '' } = how;
  const kept = pieces.filter((p) => passes(stateOf(p, marks), filter) && matches(p, query));
  const groups = (ps) => {
    const by = new Map();
    for (const p of ps) { if (!by.has(p.canon)) by.set(p.canon, []); by.get(p.canon).push(p); }
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
export const pillsWanted = (stored) => stored === '1';

/** A stored choice of sort and filter, read back as a list view: anything else is the default (a stale or hand-edited value never breaks the panel). */
export function listFrom(stored) {
  const sort = SORTS.some(([v]) => v === stored?.sort) ? stored.sort : DEFAULT_LIST.sort;
  const filter = FILTERS.some(([v]) => v === stored?.filter) ? stored.filter : DEFAULT_LIST.filter;
  return { sort, filter, query: typeof stored?.query === 'string' ? stored.query : '' };
}
