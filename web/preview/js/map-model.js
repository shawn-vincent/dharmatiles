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

const norm = (r) => ((Math.round(Number(r) || 0) % 360) + 360) % 360;

/** The piece's footprint on the map as `[x0, y0, x1, y1]`, y down (as the document counts), mm; null when it is not placed. */
export function footprint(p) {
  if (!p.at) return null;
  const turned = (norm(p.rot) / 90) % 2 === 1;
  const [w, d] = turned ? [p.size[1], p.size[0]] : p.size;
  return [p.at[0], p.at[1], p.at[0] + w, p.at[1] + d];
}

/** The 4 x 4 matrix (column-major, three.js's) that puts a piece's tile mesh where the piece stands, raised by its bottom (the tile's zero stands there; the connector is below it). */
export function matrixOf(p) {
  const f = footprint(p);
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
export function toWorld(p, x, y, z = 0) {
  const m = matrixOf(p);
  return [m[0] * x + m[4] * y + m[12], m[1] * x + m[5] * y + m[13], z + m[14]];
}

/** The box a placed piece fills in the world: its footprint, from `below` mm under its bottom (the tile's connector hangs below the tile's zero) to `top` mm over it. */
export function worldBox(p, top = 8, below = 0) {
  const f = footprint(p);
  const z0 = p.bottomMm || 0;
  return { lo: [f[0], -f[3], z0 - below], hi: [f[2], -f[1], z0 + top] };
}

/** Do two footprints overlap by more than a hair? */
export function overlaps(a, b) {
  return a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01;
}

/** `(2sq, 1.5sq)`: a place as the document writes it, in the piece's own squares. */
export const atWord = (xSq, ySq) => `(${num(xSq)}sq, ${num(ySq)}sq)`;
const num = (v) => String(Number(v.toFixed(3)));

/** Snap a map point (mm, y down) to the whole squares of the piece's own system: {x, y} in squares of that piece. */
export function snapSquares(p, xMm, yMm) {
  return { x: Math.round(xMm / p.sq), y: Math.round(yMm / p.sq) };
}

/** Where the piece's footprint would stand with its north-west corner at `sq` squares, as a box. */
const boxAt = (p, x, y, rot = p.rot) => footprint({ ...p, at: [x * p.sq, y * p.sq], rot });

/**
 * The free place nearest to `want` (squares of the piece, as {x, y}), for the piece turned `rot`: a place where
 * its footprint overlaps no other placed piece standing at the same bottom.  Searched in rings out from the wish;
 * the answer says how far it had to move.  `others` are the pieces in the way (not the piece itself).
 */
export function nearestFree(others, p, want, rot = p.rot) {
  const clash = (x, y) => {
    const b = boxAt(p, x, y, rot);
    return others.some((o) => o.at && Math.abs((o.bottomMm || 0) - (p.bottomMm || 0)) < 0.5 && overlaps(b, footprint(o)));
  };
  if (!clash(want.x, want.y)) return { ...want, moved: 0 };
  for (let r = 1; r <= 40; r++) {
    let best = null;
    for (let dx = -r; dx <= r; dx++) {
      for (let dy = -r; dy <= r; dy++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
        if (clash(want.x + dx, want.y + dy)) continue;
        const d = Math.hypot(dx, dy);
        if (!best || d < best.d) best = { x: want.x + dx, y: want.y + dy, d };
      }
    }
    if (best) return { x: best.x, y: best.y, moved: best.d };
  }
  return { ...want, moved: -1 };
}

/** Which of a piece's sides are against another piece: not used to refuse, only to read. */
export const bounds = (pieces) => {
  const placed = pieces.filter((p) => p.at);
  if (!placed.length) return null;
  const f = placed.map(footprint);
  const lo = [Math.min(...f.map((b) => b[0])), Math.min(...f.map((b) => b[1]))];
  const hi = [Math.max(...f.map((b) => b[2])), Math.max(...f.map((b) => b[3]))];
  return { lo, hi };
};

// ---- seeds ---------------------------------------------------------------------------------------------

/**
 * A seed for a new piece or a new variant: one you do not own (of that tile, system and colour mode), that no
 * piece of the map already uses and that is not the tile's own.  `owned` and `used` are Sets of numbers; `rng`
 * gives 0 up to 1 (Math.random; a fixed one in the tests).
 */
export function freshSeed(owned, used, own, rng = Math.random) {
  for (let i = 0; i < 1000; i++) {
    const s = 1 + Math.floor(rng() * 99998);
    if (!owned.has(s) && !used.has(s) && s !== own) return s;
  }
  throw new Error('no free seed in a thousand tries');
}

/** The seeds you own of a piece's tile in its system and colour mode: [{seed, have}]. */
export function ownedSeeds(inv, p) {
  const by = new Map();
  for (const q of inv?.pieces || []) {
    if (q.tile !== p.canon || q.system !== p.system || q.colour !== p.colour || !(q.have > 0)) continue;
    by.set(q.seed, (by.get(q.seed) || 0) + q.have);
  }
  return [...by].map(([seed, have]) => ({ seed, have })).sort((a, b) => a.seed - b.seed);
}

// ---- the inventory on the map ---------------------------------------------------------------------------

/**
 * What the shelf says of each piece: `Map(index -> {state, text})`; state `have` (a print you own is this
 * piece), `need` (it prints), `other` (you own it in the other colour mode: it still prints), `skip`
 * (print=skip).  Copies of one piece share what you own: the first ones, in the order written, are the ones you
 * have.  With no inventory nothing is had and every piece prints.
 */
export function pills(pieces, inv) {
  const owned = new Map();
  const other = new Map();
  const key = (q, colour = q.colour) => `${q.canon ?? q.tile}|${q.seedUsed ?? q.seed}|${q.system}|${colour}`;
  for (const q of inv?.pieces || []) {
    if (!(q.have > 0)) continue;
    owned.set(`${q.tile}|${q.seed}|${q.system}|${q.colour}`, (owned.get(`${q.tile}|${q.seed}|${q.system}|${q.colour}`) || 0) + q.have);
    const o = `${q.tile}|${q.seed}|${q.system}|${q.colour === 'multi' ? 'single' : 'multi'}`;
    other.set(o, (other.get(o) || 0) + q.have);
  }
  const out = new Map();
  for (const p of pieces) {
    if (p.error) continue;
    if (p.print === 'skip') { out.set(p.id, { state: 'skip', text: 'left off' }); continue; }
    const k = key(p);
    const n = owned.get(k) || 0;
    if (p.print !== 'always' && inv?.exists && n > 0) {
      owned.set(k, n - 1);
      out.set(p.id, { state: 'have', text: 'have' });
    } else if (inv?.exists && !n && (other.get(k) || 0) > 0) {
      out.set(p.id, { state: 'other', text: 'to print' });
    } else out.set(p.id, { state: 'need', text: 'to print' });
  }
  return out;
}

/** `{have, toPrint}`: the pieces the inventory leaves off and the pieces that print. */
export function totals(pieces, inv) {
  const m = pills(pieces, inv);
  let have = 0, toPrint = 0;
  for (const v of m.values()) {
    if (v.state === 'have') have += 1;
    else if (v.state !== 'skip') toPrint += 1;
  }
  return { have, toPrint };
}

/** `3 in inventory · 11 to print`, or `11 to print` where there is no inventory. */
export function totalsText(pieces, inv) {
  const { have, toPrint } = totals(pieces, inv);
  return inv?.exists ? `${have} in inventory · ${toPrint} to print` : `${toPrint} to print`;
}

// ---- detail by distance ---------------------------------------------------------------------------------

export const TIERS = ['super-low', 'low', 'full'];
/** The triangles the map draws, in all (the plates' budget, about). */
export const BUDGET = 900_000;
/** A piece smaller than LOW_PX on screen (its longer side, CSS px) is drawn super-low; under FULL_PX, low; else full. */
export const LOW_PX = 48, FULL_PX = 260;
/** What a tier costs before it is known, triangles (the program's budgets: super-low 12 000, low about 60 000). */
/**
 * The geometry keys held that no piece of the scene uses: what the page lets go of when a scene arrives
 * (a piece moved to another seed, tile build or printer, or left the map, leaves its old mesh behind).
 * `held` is the keys kept, `pieces` the scene's (each with its `gkey`, null for one with no mesh).
 */
export function staleKeys(held, pieces) {
  const used = new Set(pieces.map((p) => p.gkey).filter(Boolean));
  return [...held].filter((k) => !used.has(k));
}

export const GUESS = { 'super-low': 12_000, low: 60_000, full: 350_000 };

/** The tier a piece of `px` pixels wants: its own size, never more than `cap`. */
export function wantedTier(px, selected = false) {
  if (px >= FULL_PX) return 'full';
  if (px >= LOW_PX || selected) return 'low';
  return 'super-low';
}

/**
 * Which tier each piece is drawn at.  `want[i]` is the tier piece i would like by its size, `px[i]` its size on
 * screen, `have[i]` the tiers loaded for it (a Set), `tris[i]` a function tier -> triangles (the loaded mesh's, or
 * a guess).  A piece is drawn at the best tier it has that is not above what it wants; the whole is held under
 * `budget` triangles by lowering the smallest-on-screen first (full to low, then low to super-low), as far as
 * the loaded tiers allow.  Returns the tier to draw for each piece (null: nothing loaded yet) and the total.
 */
export function chooseTiers(want, px, have, tris, budget = BUDGET) {
  const best = (i, cap) => {
    for (let t = cap; t >= 0; t--) if (have[i].has(TIERS[t])) return t;
    return -1;
  };
  const cap = want.map((w) => TIERS.indexOf(w));
  const pick = cap.map((c, i) => best(i, c));
  const cost = (i) => (pick[i] < 0 ? 0 : tris[i](TIERS[pick[i]]));
  let total = pick.reduce((n, _, i) => n + cost(i), 0);
  // lower the smallest on screen first
  const order = pick.map((_, i) => i).sort((a, b) => px[a] - px[b]);
  while (total > budget) {
    let moved = false;
    for (const i of order) {
      if (pick[i] <= 0) continue;
      const lower = best(i, pick[i] - 1);
      if (lower === pick[i] || lower < 0) continue;
      total -= cost(i);
      pick[i] = lower;
      total += cost(i);
      moved = true;
      if (total <= budget) break;
    }
    if (!moved) break;
  }
  return { tiers: pick.map((t) => (t < 0 ? null : TIERS[t])), total };
}

/** How many pieces are drawn at each tier, and how many have nothing yet: `{full, low, 'super-low', none}`. */
export function tierCounts(tiers) {
  const c = { full: 0, low: 0, 'super-low': 0, none: 0 };
  for (const t of tiers) c[t ?? 'none'] += 1;
  return c;
}

/** The pixel size of `mm` at `dist` from a perspective camera of `fov` degrees on a canvas `h` pixels tall. */
export const pxOfPerspective = (mm, dist, fov, h) => (mm * h) / (2 * Math.max(dist, 1e-3) * Math.tan((fov * Math.PI) / 360));
/** The same for an orthographic camera whose frustum is `half` mm from the middle to the top edge. */
export const pxOfOrtho = (mm, half, h) => (mm * h) / (2 * half);

// ---- a piece's words -------------------------------------------------------------------------------------

/** The words the panel says of a piece's bottom (where its tile's zero stands): a `$` name's own text or the length as written. */
export const bottomWords = (p, names = []) => {
  if (!p.bottom) return 'ground level';
  const n = names.find((x) => `$${x.name}` === p.bottom);
  return n ? `${p.bottom} · ${Number(n.dmm.toFixed(1))} dmm` : p.bottom;
};

/** A piece's label: its tile's name, and its id when it has one. */
export const label = (p) => (p.id ? `${p.name} · ${p.id}` : p.name);

/** The turn a quarter on from `rot`: clockwise (+1) or counter-clockwise (-1). */
export const turned = (rot, dir) => norm(rot + 90 * dir);

/** A piece as the page draws it on a plate pin: nothing here, see js/editor/plates.js. */
export const isPlaced = (p) => !!p.at;
