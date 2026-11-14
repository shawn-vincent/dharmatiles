// What the rows of the tile level can do to the tile, as plain data and no page
// (docs/preview.html, "The panel"): where a new entry goes, what a drag or ⌥↑
// asks of the source-edit layer, which entries can be moved at all, what an
// entry's id= is and what it may be called, and the small pictures the Shape rows
// wear (a region's mask, a line's points).  The page part (editor/rows.js) sends
// the edits; every rule about them is here, so it can be tested without one.

const isBase = (it) => it.what === 'base' || (it.element === 'connector' && it.key === 'tile/connector');

// ---- which node of the document an entry is -------------------------------------------------------
//
// A stack entry is written in the tile's own document (a node of its stack), or made by a definition
// call (`long-grass at=bank`: it carries the call, as the node the document writes), or written in another
// file (a library part).  A `scatter` written inside a `dirt` is an entry of its own, listed after it.
// A move, a hide and a delete address a node of the document, never an entry as such.

/** The bytes [start, end) of the node of `file` this entry stands for: its own node, or the call that made it; null when `file` does not write it. */
export function nodeRange(it, file) {
  if (it?.call) return it.call.file === file ? [it.call.span[0], it.call.span[0] + it.call.span[1]] : null;
  return it?.span && it.file === file ? [it.span[0], it.span[0] + it.span[1]] : null;
}

/** The byte of the node a hide, a delete or a name addresses: the entry's own, or the call that made it; null when `file` does not write it. */
export const ownNode = (it, file) => nodeRange(it, file)?.[0] ?? null;

/**
 * The byte of the outermost node of `file` that holds this entry, the one a move carries it in: a scatter written
 * inside a dirt moves with the dirt.  Null for an entry `file` does not write.
 */
export function topNode(it, items, file) {
  const r = nodeRange(it, file);
  if (!r) return null;
  let top = r;
  for (const o of items) {
    const c = o === it ? null : nodeRange(o, file);
    if (c && c[0] <= top[0] && c[1] >= top[1] && (c[0] < top[0] || c[1] > top[1])) top = c;
  }
  return top[0];
}

/**
 * The id the program named the node by (`tile/stack/rock#2`): of the nodes its check listed, the one whose text starts at byte
 * `at`, else the innermost that holds it.  An edit names a node by this id, never by the byte: the id is what the program
 * resolves in the text it is sent, and refuses when the text has no such node.  Null when no node is there.
 * @param {{id: string, at: number, len: number}[] | null | undefined} nodes the check's `nodes`
 * @param {number | null | undefined} at
 */
export function idAtByte(nodes, at) {
  if (at == null || !nodes) return null;
  let best = null;
  for (const n of nodes) {
    if (n.at === at) return n.id;
    if (at > n.at && at < n.at + Math.max(n.len, 1) && (!best || n.len < best.len)) best = n;
  }
  return best ? best.id : null;
}

/** The id of the tile's `line` or `region` called `name` among the check's nodes; null when the text writes none. */
export function markId(nodes, kind, name) {
  return nodes?.find((n) => n.what === kind && n.name === name)?.id ?? null;
}

/** Is `it` a node of this document that a hide, a delete or a move can address? */
export const writtenIn = (it, file) => ownNode(it, file) != null;

/** The first and last list position of the entries that one node makes, around `i` (just `i` when `file` does not write it). */
export function groupAt(items, i, file) {
  const n = topNode(items[i], items, file);
  if (n == null) return [i, i];
  let a = i, b = i;
  while (a > 0 && topNode(items[a - 1], items, file) === n) a--;
  while (b < items.length - 1 && topNode(items[b + 1], items, file) === n) b++;
  return [a, b];
}

/** How many of the entries before list position `i` are stack entries (the base is a line of the stack that is not one): the place the plan is asked about. */
export const entryPlace = (items, i) => items.slice(0, i).filter((it) => !isBase(it)).length;

/**
 * Where a new entry goes when it is put before list position `slot` (the list length is the end):
 * `{place: 'before', at}` above the first node from there on that the document writes (a slot inside what one node makes
 * is after it), else `{place: 'stack'}`, the end of the stack; and `index`, the place for the offers.
 */
export function addPlace(items, slot, file) {
  let from = slot;
  if (slot > 0 && slot < items.length) {
    const n = topNode(items[slot - 1], items, file);
    while (n != null && from < items.length && topNode(items[from], items, file) === n) from++;
  }
  for (let i = from; i < items.length; i++) {
    const n = topNode(items[i], items, file);
    if (n != null) return { place: 'before', at: n, index: entryPlace(items, i) };
  }
  return { place: 'stack', index: entryPlace(items, items.length) };
}

/**
 * What moving entry `from` (with whatever shares its node) to stand before list position `slot` asks of the source:
 * `{at, before}` (`before` the byte of the node it goes above, null for the end), or null when it would not move (it is
 * already there, or its node is not this document's, or nothing this document writes is there to go above).
 */
export function planMove(items, from, slot, file) {
  const it = items[from];
  if (!it) return null;
  const n = topNode(it, items, file);
  if (n == null) return null;
  const [g0, g1] = groupAt(items, from, file);
  if (slot >= g0 && slot <= g1 + 1) return null;
  if (slot < g0) {
    for (let i = slot; i < g0; i++) {
      const t = topNode(items[i], items, file);
      if (t != null) return { at: n, before: t };
    }
    return null;
  }
  for (let i = slot; i < items.length; i++) {
    const t = topNode(items[i], items, file);
    if (t != null) return { at: n, before: t };
  }
  return { at: n, before: null };
}

/** The slot for a move one node up (`-1`) or down (`+1`) from `from`: where planMove takes it; null when there is no such step. */
export function stepSlot(items, from, dir, file) {
  const [g0, g1] = groupAt(items, from, file);
  if (dir < 0) return g0 === 0 ? null : groupAt(items, g0 - 1, file)[0];
  return g1 >= items.length - 1 ? null : groupAt(items, g1 + 1, file)[1] + 1;
}

/** Can entry `from` step that way (`dir` -1 up, +1 down)? */
export function canStep(items, from, dir, file) {
  const s = stepSlot(items, from, dir, file);
  return s != null && planMove(items, from, s, file) != null;
}

/** Which slot of the rows a pointer at height `y` means: `mids` are the middles of the rows (in order); the slot is the number of rows whose middle is above `y`. */
export function slotAt(mids, y) {
  let n = 0;
  for (const m of mids) if (y > m) n++;
  return n;
}

/**
 * The element a move changes the seeds of, if it does.  An entry without `id=` is keyed by its element
 * and its count (`tile/dirt/1`), so moving it across another entry of the same element and no id swaps the
 * two counts, and so the two looks.  Returns that element's name, else null.
 */
export function seedSwap(items, from, slot) {
  const it = items[from];
  if (!it || idOf(it.key) != null) return null;
  // the entries it steps over
  const crossed = slot > from ? items.slice(from + 1, slot) : items.slice(slot, from);
  return crossed.some((o) => o.what === it.what && idOf(o.key) == null) ? it.what : null;
}

// ---- id= ----------------------------------------------------------------------------

/** The `id=` an entry's key says (`tile/rock/oak` -> `oak`); null for one named by its count (`tile/rock/0`), the base and anything inlined from a call. */
export function idOf(key) {
  const p = String(key || '').split('/');
  return p.length === 3 && p[0] === 'tile' && !/^\d+$/.test(p[2]) ? p[2] : null;
}

/** What the page says an entry is called: its id, else what it is. */
export const nameOf = (it) => idOf(it.key) ?? it.label;

/**
 * Why `name` cannot be the id of entry `it` (null when it can).  The reader refuses an empty id, one
 * with a slash and one that is only digits (an id becomes a part of a seed path beside the counts);
 * this takes the plain names: letters, digits, `-` and `_`.  No other entry of the same element has it.
 */
export function idFlaw(name, items, it) {
  const s = String(name ?? '').trim();
  if (!s) return null; // empty clears it
  if (!/^[A-Za-z0-9_-]+$/.test(s)) return 'Use letters, digits, - and _ only';
  if (/^\d+$/.test(s)) return 'A name cannot be only digits';
  const other = items.find((o) => o !== it && o.what === it.what && idOf(o.key) === s);
  return other ? `Another ${it.what} is already called ${s}` : null;
}

// ---- the pictures a Shape row wears ------------------------------------------------------

/**
 * A region's mask (`bits` in `nx` by `ny` cells, row 0 at the south) boxed down to at most `max` across: `{w, h, a}`
 * with `a` each cell's coverage 0-255, the top row first (the picture's).  The mask is the one the build paints on.
 */
export function maskThumb(r, max = 64) {
  const { nx, ny, bits } = r;
  const k = Math.max(1, Math.max(nx, ny) / max);
  const w = Math.max(1, Math.round(nx / k)), h = Math.max(1, Math.round(ny / k));
  const a = new Uint8Array(w * h);
  for (let y = 0; y < h; y++) {
    // the picture's top row is the mask's last
    const j0 = Math.floor(((h - 1 - y) * ny) / h), j1 = Math.max(j0 + 1, Math.floor(((h - y) * ny) / h));
    for (let x = 0; x < w; x++) {
      const i0 = Math.floor((x * nx) / w), i1 = Math.max(i0 + 1, Math.floor(((x + 1) * nx) / w));
      let on = 0, n = 0;
      for (let j = j0; j < j1; j++) for (let i = i0; i < i1; i++) { n++; if (bits[j * nx + i] === '1') on++; }
      a[y * w + x] = n ? Math.round((on * 255) / n) : 0;
    }
  }
  return { w, h, a };
}

/** How many cells of the mask are set, as a share of the tile (0-1): a region with none shows a dash, not a blank. */
export const coverage = (r) => {
  let n = 0;
  for (let i = 0; i < r.bits.length; i++) if (r.bits[i] === '1') n++;
  return r.bits.length ? n / r.bits.length : 0;
};

/**
 * A line's points (in squares from the south-west corner, as the map has them) as pixel points in a box
 * `w` by `h` for a tile `cols` by `rows` squares (the box has the tile's shape), with `pad` kept clear and y turned over.
 */
export function linePoints(pts, cols, rows, w, h, pad = 2) {
  const iw = w - 2 * pad, ih = h - 2 * pad;
  return (pts || []).map(([x, y]) => [
    Math.round((pad + (x / cols) * iw) * 100) / 100,
    Math.round((pad + (1 - y / rows) * ih) * 100) / 100,
  ]);
}

/** The size (px) of a thumbnail box for a tile `cols` by `rows`: the longer side is `max`. */
export function thumbBox(cols, rows, max = 36) {
  const c = cols || 1, r = rows || 1;
  return c >= r ? [max, Math.max(8, Math.round((max * r) / c))] : [Math.max(8, Math.round((max * c) / r)), max];
}

// ---- what a change to the order says ---------------------------------------------------

/** The codes the plan refuses an order by (`dt::order`: listed too early; `dt::plan`: reads what nothing makes). */
const ORDER_CODES = new Set(['dt::order', 'dt::plan']);

/** The check's refusal of the order, if its errors hold one: the first. */
export const orderRefusal = (errors) => (errors || []).find((e) => ORDER_CODES.has(e.code)) ?? null;

/** The first letter of `s` as a capital. */
const sentence = (s) => s.charAt(0).toUpperCase() + s.slice(1);

/** One line for a refused order, in the checker's own words: what it says, then how to mend it, each as a sentence. */
export const refusalLine = (e) => [e.message, e.help].filter(Boolean).map((s) => sentence(s.replace(/\.$/, ''))).join('. ');

/** Where the moved entry's first row stands in the list once the move is made: its list position (`from` is any row of what moves). */
export function landing(items, from, slot, file) {
  const [g0, g1] = groupAt(items, from, file);
  return (slot < g0 ? slot : slot - (g1 - g0 + 1)) + (from - g0);
}

/** What a finished move says: where the entry went as the stack is drawn, top of the tile first (below which, or to the bottom or the top), and that two same-named entries with no id swapped their looks. */
export function movedNote(items, from, slot, label) {
  const where = slot <= 0 ? 'to the bottom' : slot >= items.length ? 'to the top' : `below ${nameOf(items[slot])}`;
  const swap = seedSwap(items, from, slot);
  const base = `Moved ${label} ${where}`;
  return swap ? `${base}. Entries with no id= are seeded by their count, so the two ${swap}s swapped looks: name them to keep each.` : base;
}
