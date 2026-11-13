// What the page remembers of how each map's list of pieces is sorted and filtered (the panel's sort and filter),
// by the map's tile id, across visits.  It is a convenience of the viewer, so it lives in the browser's own
// storage and nowhere else; the search is not kept.

const KEY = 'dt-map-list';

/** The remembered choices, `{tileId: {sort, filter}}` (none where storage is blocked or holds something that is not ours). */
export function readKept() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '{}');
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
  } catch { return {}; /* private mode, or a value that is not JSON: no choices remembered */ }
}

/** Remember a map's sort and filter. */
export function keep(id, { sort, filter }) {
  try { localStorage.setItem(KEY, JSON.stringify({ ...readKept(), [id]: { sort, filter } })); } catch { /* private mode: the choice lasts until the page closes */ }
}
