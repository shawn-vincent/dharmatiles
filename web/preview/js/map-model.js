// The map, as the page reads it (docs/maps.html): where a piece stands, how it is turned and raised, the
// snap to its own system's squares, the nearest free place, the matrix that puts its tile's mesh there, which
// level of detail each piece is drawn at under a triangle budget, and what the inventory says of each piece.
// The model and nothing else (no DOM, no three.js), so it is tested alone (tests/map.test.js).
//
// The program's numbers (preview/scene.rs): a piece has `at` [x, y] (mm across and down from the map's
// north-west corner), `rot` (0, 90, 180, 270 clockwise), `bottomMm` (the height above the map's ground of its tile's zero: the tile's own ground level, so at 0 its connector hangs below the map's ground), `size` [w, d] (its
// tile's squares in mm, before it is turned) and `sq` (its own system's square, mm).  The tile's mesh is in
// the tile's frame: x east, y north from its south-west corner.  The map's world frame, the one the view
// draws, has x east and y north from the map's north-west corner: y is minus the document's "down".

/** @typedef {import('./api').ScenePiece} Piece */
/** @typedef {Pick<Piece, 'at' | 'rot' | 'size'>} Placed  what a footprint reads of a piece */
/** @typedef {[number, number, number, number]} Box  `[x0, y0, x1, y1]`, mm, y down */

const norm = (/** @type {number | null | undefined} */ r) => ((Math.round(Number(r) || 0) % 360) + 360) % 360;

/** The piece's footprint on the map as `[x0, y0, x1, y1]`, y down (as the document counts), mm; null when it is not placed. */
/** @param {Placed} p @returns {Box | null} */
export function footprint(p) {
  if (!p.at) return null;
  const turned = (norm(p.rot) / 90) % 2 === 1;
  const [w, d] = turned ? [p.size[1], p.size[0]] : p.size;
  return [p.at[0], p.at[1], p.at[0] + w, p.at[1] + d];
}

/** The 4 x 4 matrix (column-major, three.js's) that puts a piece's tile mesh where the piece stands, raised by its bottom (the tile's zero stands there; the connector is below it). */
/** @param {Placed & { bottomMm?: number }} p */
export function matrixOf(p) {
  const f = footprint(p);
  if (!f) throw new Error('A piece with no place (`at`) has no matrix.');
  const th = (norm(p.rot) * Math.PI) / 180;
  const c = Math.round(Math.cos(th)), s = Math.round(Math.sin(th));
  const [w, d] = p.size;
  const cw = (f[0] + f[2]) / 2, cd = (f[1] + f[3]) / 2;
  const tx = cw - (c * w) / 2 - (s * d) / 2;
  const ty = -cd + (s * w) / 2 - (c * d) / 2;
  // rows: [c s 0 tx] [-s c 0 ty] [0 0 1 z]
  return [c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, tx, ty, p.bottomMm || 0, 1];
}

/** A point of the tile's frame as the map's world point [x, y, z] (y up). */
/** @param {Placed & { bottomMm?: number }} p @param {number} x @param {number} y */
export function toWorld(p, x, y, z = 0) {
  const m = matrixOf(p);
  return [m[0] * x + m[4] * y + m[12], m[1] * x + m[5] * y + m[13], z + m[14]];
}

/** The box a placed piece fills in the world: its footprint, from `below` mm under its bottom (the tile's connector hangs below the tile's zero) to `top` mm over it. */
/** @param {Placed & { bottomMm?: number }} p @param {number} [top] @param {number} [below] */
export function worldBox(p, top = 8, below = 0) {
  const f = footprint(p);
  if (!f) throw new Error('A piece with no place (`at`) has no box.');
  const z0 = p.bottomMm || 0;
  return { lo: [f[0], -f[3], z0 - below], hi: [f[2], -f[1], z0 + top] };
}

/** Do two footprints overlap by more than a hair? */
/** @param {Box} a @param {Box} b */
export function overlaps(a, b) {
  return a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01;
}

/** `(2sq, 1.5sq)`: a place as the document writes it, in the piece's own squares. */
export const atWord = (/** @type {number} */ xSq, /** @type {number} */ ySq) => `(${num(xSq)}sq, ${num(ySq)}sq)`;
const num = (/** @type {number} */ v) => String(Number(v.toFixed(3)));

/** Snap a map point (mm, y down) to the whole squares of the piece's own system: {x, y} in squares of that piece. */
/** @param {{ sq: number }} p @param {number} xMm @param {number} yMm */
export function snapSquares(p, xMm, yMm) {
  return { x: Math.round(xMm / p.sq), y: Math.round(yMm / p.sq) };
}

/** `column 1, row 1`: where a piece stands, counted from one (the document writes it from zero). A piece in the tray has no place. */
/** @param {Pick<Piece, 'at' | 'sq'>} p */
export function placeWords(p) {
  if (!p.at) return 'in the tray';
  const s = snapSquares(p, p.at[0], p.at[1]);
  return `column ${s.x + 1}, row ${s.y + 1}`;
}

/** Which of a piece's sides are against another piece: not used to refuse, only to read. */
export const bounds = (/** @type {Placed[]} */ pieces) => {
  const placed = pieces.filter((p) => p.at);
  if (!placed.length) return null;
  const f = /** @type {Box[]} */ (placed.map(footprint)); // each is placed, so each has a footprint
  const lo = [Math.min(...f.map((b) => b[0])), Math.min(...f.map((b) => b[1]))];
  const hi = [Math.max(...f.map((b) => b[2])), Math.max(...f.map((b) => b[3]))];
  return { lo, hi };
};

// ---- the inventory on the map ---------------------------------------------------------------------------

// What the shelf says of each piece, and how many pieces print, is the program's word (`piece.shelf`,
// `scene.have`, `scene.toPrint`: crates/dharmatiles/src/preview/scene.rs `mark_shelf`); the page draws it.

/** `Map(id -> {state, text})` of what the program says of each piece with a word: have, need, other or skip. */
export const marks = (/** @type {Pick<Piece, 'id' | 'shelf'>[]} */ pieces) => {
  /** @type {Map<string, import('./api').ShelfMark>} */
  const out = new Map();
  for (const p of pieces) if (p.shelf) out.set(p.id, p.shelf);
  return out;
};

/** `3 in inventory · 11 to print`, or `11 to print` where there is no inventory: the program's numbers, said. */
/** @param {Pick<import('./api').SceneJson, 'toPrint' | 'shelfExists' | 'have'> | null | undefined} scene */
export function totalsText(scene) {
  const toPrint = scene?.toPrint ?? 0;
  return scene?.shelfExists ? `${scene.have} in inventory · ${toPrint} to print` : `${toPrint} to print`;
}

// ---- detail by distance ---------------------------------------------------------------------------------

export const TIERS = ['super-low', 'low', 'full'];
/**
 * The triangles the map draws, in all: sixteen full tiles (a 4 x 4 map close up is all full).  From
 * measurements of 13 Nov 2026, library tiles at one seed from the program's tier lane: a super-low mesh is 4 000 to
 * 12 000 triangles, low 12 000 to 38 000, full from 224 000, 330 000 on average over the 20 pieces of the first plate.  The page, in Chrome on an M2 Max (Apple GPU,
 * 1147 x 650), drew 20 full pieces, 6.6 million triangles, in 4.6 ms a frame (0.7 ms a million), with 640 MB of
 * page heap for the meshes (30 MB a full): the limit is memory, not drawing, so 5.5 million holds sixteen fulls at
 * about 4 ms a frame and 500 MB.
 */
export const BUDGET = 5_500_000;
/**
 * A piece's size on screen (its longer side, CSS px) moves it up a tier at UP and down again only under DOWN: the
 * margin between them is what keeps a camera's small movement from changing any tier (wantedTier).
 * `UP.low` is the size where super-low gives way to low; `UP.full` where low gives way to full.
 */
export const UP = { low: 40, full: 120 };
export const DOWN = { low: 30, full: 90 };
/** A piece that has (or has asked for) full keeps it against a piece that is not over its own size times this. */
export const STICKY = 1.25;
/** What a tier costs before it is known, triangles: the middle of what was measured. */
export const GUESS = { 'super-low': 8_000, low: 25_000, full: 330_000 };

/**
 * The geometry keys held that no piece of the scene uses: what the page lets go of when a scene arrives
 * (a piece moved to another seed, tile build or printer, or left the map, leaves its old mesh behind).
 * `held` is the keys kept, `pieces` the scene's (each with its `gkey`, null for one with no mesh).
 */
/** @param {Iterable<string>} held @param {{ gkey?: string | null }[]} pieces */
export function staleKeys(held, pieces) {
  const used = new Set(pieces.map((p) => p.gkey).filter(Boolean));
  return [...held].filter((k) => !used.has(k));
}

/**
 * The tier a piece of `px` pixels wants, given the tier it wanted last time (`was`, null for a piece new to the
 * view).  A function of the two alone: from `was` it goes up past UP and down under DOWN, so between the two
 * it stays where it was, and a size that jitters inside the margin never changes the answer.  A selected piece
 * wants at least low.
 */
/** @param {number} px @param {string | null} [was] @param {boolean} [selected] */
export function wantedTier(px, was = null, selected = false) {
  let t = was ? Math.max(0, TIERS.indexOf(was)) : 0;
  while (t < 2 && px >= (t === 0 ? UP.low : UP.full)) t += 1;
  while (t > 0 && px < (t === 2 ? DOWN.full : DOWN.low)) t -= 1;
  return TIERS[Math.max(t, selected ? 1 : 0)];
}

/**
 * @typedef {{
 *   want: string[], px: number[], have: Set<string>[], tris: ((tier: string) => number)[],
 *   current?: (string | null)[], dist?: number[], ids?: string[], budget?: number,
 * }} TierInput
 */

/**
 * Which tier each piece is drawn at, as one input object: `want[i]` the tier piece i would like by its size,
 * `px[i]` its size on screen, `have[i]` the tiers loaded for it (a Set), `tris[i]` a function tier -> triangles
 * (the loaded mesh's, or a guess), `current[i]` the tier it holds now (shown, or asked for and on its way; null for
 * none), `dist[i]` its distance from the eye and `ids[i]` its id.  A piece is drawn at the best tier it has that is
 * not above what it wants; the whole is held under `budget` triangles by lowering the least deserving first (full to
 * low, then low to super-low), as far as the loaded tiers allow.
 *
 * Who deserves most is a total order that does not depend on the order of the list: by size on screen, a piece that
 * holds full counting STICKY times its size (so another takes its room only by being clearly bigger), then by the
 * nearer, then by id.  Returns the tier to draw for each piece (null: nothing loaded yet) and the total.
 * @param {TierInput} input
 */
export function chooseTiers({ want, px, have, tris, current = [], dist = [], ids = [], budget = BUDGET }) {
  const best = (/** @type {number} */ i, /** @type {number} */ cap) => {
    for (let t = cap; t >= 0; t--) if (have[i].has(TIERS[t])) return t;
    return -1;
  };
  const pick = want.map((w, i) => best(i, TIERS.indexOf(w)));
  const cost = (/** @type {number} */ i) => (pick[i] < 0 ? 0 : tris[i](TIERS[pick[i]]));
  let total = pick.reduce((n, _, k) => n + cost(k), 0);
  const eff = (/** @type {number} */ i) => px[i] * (current[i] === 'full' ? STICKY : 1);
  const id = (/** @type {number} */ i) => String(ids[i] ?? i);
  const lowestFirst = pick.map((_, i) => i).sort((a, b) =>
    eff(a) - eff(b) || (dist[b] ?? 0) - (dist[a] ?? 0) || id(b).localeCompare(id(a)) || b - a);
  // the fulls go first (to low), the least deserving of them first; only when that is not enough, the lows (to super-low)
  for (const level of [2, 1]) {
    for (const i of lowestFirst) {
      if (total <= budget) break;
      if (pick[i] !== level) continue;
      const lower = best(i, level - 1);
      if (lower < 0) continue;
      total -= cost(i);
      pick[i] = lower;
      total += cost(i);
    }
  }
  return { tiers: pick.map((t) => (t < 0 ? null : TIERS[t])), total };
}

/**
 * What to draw and what to ask for.  `tiers` is chooseTiers over what is loaded; `intended` is the same as if
 * everything wanted had arrived, so the pieces that will fit the budget are the ones fetched, all together:
 * `fetch` lists `[i, tier]` of each piece's intended tier that is not loaded, most deserving first.
 */
/** @param {TierInput} input */
export function planTiers(input) {
  const shown = chooseTiers(input);
  const intended = chooseTiers({ ...input, have: input.have.map(() => new Set(TIERS)) });
  const id = (/** @type {number} */ i) => String(input.ids?.[i] ?? i);
  /** @type {[number, string][]} */
  const fetch = [];
  intended.tiers.forEach((t, i) => { if (t && !input.have[i].has(t)) fetch.push([i, t]); });
  fetch.sort(([a], [b]) => input.px[b] - input.px[a] || (input.dist?.[a] ?? 0) - (input.dist?.[b] ?? 0) || id(a).localeCompare(id(b)));
  return { tiers: shown.tiers, total: shown.total, intended: intended.tiers, fetch };
}

/** How many pieces are drawn at each tier, and how many have nothing yet: `{full, low, 'super-low', none}`. */
/** @param {(string | null)[]} tiers */
export function tierCounts(tiers) {
  /** @type {Record<string, number>} */
  const c = { full: 0, low: 0, 'super-low': 0, none: 0 };
  for (const t of tiers) c[t ?? 'none'] += 1;
  return c;
}

/** The pixel size of `mm` at `dist` from a perspective camera of `fov` degrees on a canvas `h` pixels tall. */
export const pxOfPerspective = (/** @type {number} */ mm, /** @type {number} */ dist, /** @type {number} */ fov, /** @type {number} */ h) => (mm * h) / (2 * Math.max(dist, 1e-3) * Math.tan((fov * Math.PI) / 360));
/** The same for an orthographic camera whose frustum is `half` mm from the middle to the top edge. */
export const pxOfOrtho = (/** @type {number} */ mm, /** @type {number} */ half, /** @type {number} */ h) => (mm * h) / (2 * half);

// ---- a piece's words -------------------------------------------------------------------------------------

/** The words the panel says of a piece's bottom (where its tile's zero stands): a `$` name's own text or the length as written. */
export const bottomWords = (/** @type {Pick<Piece, 'bottom'>} */ p, /** @type {{ name: string, dmm: number }[]} */ names = []) => {
  if (!p.bottom) return 'ground level';
  const n = names.find((x) => `$${x.name}` === p.bottom);
  return n ? `${p.bottom} · ${Number(n.dmm.toFixed(1))} dmm` : p.bottom;
};

/** A piece's label: its tile's name, and its id when it has one. */
export const label = (/** @type {Pick<Piece, 'id' | 'name'>} */ p) => (p.id ? `${p.name} · ${p.id}` : p.name);

/** The turn a quarter on from `rot`: clockwise (+1) or counter-clockwise (-1). */
export const turned = (/** @type {number} */ rot, /** @type {number} */ dir) => norm(rot + 90 * dir);

/** A piece as the page draws it on a plate pin: nothing here, see js/editor/plates.js. */
/**
 * @template {Pick<Piece, 'at'>} P
 * @param {P} p
 * @returns {p is P & { at: [number, number] }}
 */
export const isPlaced = (p) => !!p.at;

/** The groups of the map's pieces by tile: [{canon, name, pieces}], in the order the tiles first appear. */
/**
 * @template {Pick<Piece, 'canon' | 'name'>} P
 * @param {P[]} ps
 */
export function groupsOf(ps) {
  /** @type {Map<string, { canon: string, name: string, pieces: P[] }>} */
  const by = new Map();
  for (const p of ps) {
    if (!by.has(p.canon)) by.set(p.canon, { canon: p.canon, name: p.name, pieces: [] });
    /** @type {{ pieces: P[] }} */ (by.get(p.canon)).pieces.push(p); // set on the line above
  }
  return [...by.values()];
}
