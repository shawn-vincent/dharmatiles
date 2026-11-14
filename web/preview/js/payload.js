// Reads the binary mesh payload (see crates/dharmatiles/src/preview/payload.rs).
//
//   "DTM1" | u32 json_len | json (space padded to 4) |
//   f32 positions[9n] | f32 normals[9n] | u8 painted[9n] | u8 clean[9n] |
//   u16 entry[n] | u16 piece[n] | padding to 4
//
// entry is a place in the header's `stack`, the stack the mesh was built
// from (the connector, when the tile has a base, is last; 65535 none);
// piece counts the things within it (rock 0, rock 1 of a scatter).
// Everything is a typed-array view onto the one buffer: no copies, no JSON
// numbers.
//
// The text on screen may have moved on since the mesh was built (an entry
// inserted, deleted, moved), so an entry number is only ever read against
// the mesh's own stack, and what the page keeps of an entry is its key (its
// id, `tile/grass/0`), looked up in whichever stack a part of the page needs.

export const NO_ENTRY = 65535;

/** The stack the mesh was built from: `[{index, key, label, element, what}]` (none for a mesh made without one). */
export const meshStack = (payload) => payload?.meta?.stack ?? [];

/** The entry that mesh entry number `index` is (null for none), decoded against the mesh's own stack. */
export function meshEntry(payload, index) {
  if (index == null || index === NO_ENTRY) return null;
  return meshStack(payload)[index] ?? null;
}

/** The key of mesh entry number `index`, or null. */
export const entryKey = (payload, index) => meshEntry(payload, index)?.key ?? null;

/** The mesh's entry number for `key`, or -1 when the mesh has no such entry. */
export const meshIndex = (payload, key) => (key == null ? -1 : meshStack(payload).findIndex((e) => e.key === key));

/**
 * The height each entry's geometry reaches, by key: `Map(key -> [lo, hi])`, printed mm from the tile's zero.  The program
 * measured it in the mesh it built (`stack_json`, payload.rs); an entry that made no triangle has none.  Nothing is worked
 * out here.
 */
export function bandsByKey(payload) {
  const bands = new Map();
  for (const e of meshStack(payload)) if (Array.isArray(e.z)) bands.set(e.key, /** @type {[number, number]} */ (e.z));
  return bands;
}

/** How many pieces each entry's geometry has, by key: `Map(key -> n)`. */
export function piecesByKey(payload) {
  const pieces = new Map();
  for (let t = 0; t < payload.triangles; t++) {
    const key = entryKey(payload, payload.entry[t]);
    if (key != null) pieces.set(key, Math.max(pieces.get(key) || 0, payload.piece[t] + 1));
  }
  return pieces;
}

/**
 * Expand per-triangle ownership to the mesh's three vertices for the GPU.
 * Entry 0 means no owner; a real entry is stored +1. Uint16 is exact for
 * every practical stack and half the memory of the old float attribute.
 */
export function ownerVertices(entry, piece) {
  const who = new Uint16Array(6 * entry.length);
  for (let t = 0; t < entry.length; t++) {
    const e = entry[t] === NO_ENTRY ? 0 : entry[t] + 1;
    for (let v = 0; v < 3; v++) {
      who[6 * t + 2 * v] = e;
      who[6 * t + 2 * v + 1] = piece[t];
    }
  }
  return who;
}

/**
 * The triangles the gate painted: `{t, red}`, red past the steepest angle and orange in the band.  The one reading of the
 * painted mesh against its clean twin: the worst face and the underside plan both come from here.
 */
export function* flaggedTriangles(payload) {
  const p = payload.painted, c = payload.clean;
  for (let t = 0; t < payload.triangles; t++) {
    const k = 9 * t;
    if (p[k] === c[k] && p[k + 1] === c[k + 1] && p[k + 2] === c[k + 2]) continue;
    yield { t, red: p[k] > 180 && p[k + 1] < 90 && p[k + 2] < 90 };
  }
}

/**
 * Where the overhang is, seen from below: the flagged triangles' footprints as two paths (red, orange) in mm, over the tile's
 * outline.  Seen from below the tile is turned over about its y axis, so x runs the other way (`hi.x - x`).  The plan is of the
 * program's own flags (the gate's painted faces), never a second classification; the areas are the gate's to give.  Null when
 * the mesh has no flagged face or no tile outline.
 * @returns {{ width: number, height: number, red: string, orange: string, reds: number, oranges: number } | null}
 */
export function undersidePlan(payload) {
  const lo = payload.meta?.lo, hi = payload.meta?.hi; // the mesh's own box, in the header of every mesh
  if (!lo || !hi) return null;
  const q = payload.positions;
  const r = (n) => Math.round(n * 100) / 100;
  const out = { width: hi[0] - lo[0], height: hi[1] - lo[1], red: '', orange: '', reds: 0, oranges: 0 };
  for (const { t, red } of flaggedTriangles(payload)) {
    const k = 9 * t;
    let d = '';
    for (let v = 0; v < 3; v++) d += `${v ? 'L' : 'M'}${r(hi[0] - q[k + 3 * v])} ${r(hi[1] - q[k + 3 * v + 1])}`;
    if (red) { out.red += `${d}Z`; out.reds += 1; } else { out.orange += `${d}Z`; out.oranges += 1; }
  }
  return out.reds + out.oranges ? out : null;
}

/**
 * The largest face painted by the gate, red before orange. Returns its owner
 * and bounds so the editor can explain a verdict by taking the user there.
 */
export function worstFlagged(payload) {
  let best = null;
  for (const { t, red } of flaggedTriangles(payload)) {
    const k = 9 * t;
    const priority = red ? 2 : 1;
    const q = payload.positions;
    const ax = q[k + 3] - q[k], ay = q[k + 4] - q[k + 1], az = q[k + 5] - q[k + 2];
    const bx = q[k + 6] - q[k], by = q[k + 7] - q[k + 1], bz = q[k + 8] - q[k + 2];
    const cx = ay * bz - az * by, cy = az * bx - ax * bz, cz = ax * by - ay * bx;
    const area = Math.hypot(cx, cy, cz) / 2;
    if (best && (priority < best.priority || (priority === best.priority && area <= best.area))) continue;
    const xs = [q[k], q[k + 3], q[k + 6]], ys = [q[k + 1], q[k + 4], q[k + 7]], zs = [q[k + 2], q[k + 5], q[k + 8]];
    best = {
      priority, area, kind: red ? 'red' : 'orange', triangle: t,
      entry: payload.entry[t] === NO_ENTRY ? null : payload.entry[t], key: entryKey(payload, payload.entry[t]), piece: payload.piece[t],
      lo: [Math.min(...xs), Math.min(...ys), Math.min(...zs)],
      hi: [Math.max(...xs), Math.max(...ys), Math.max(...zs)],
    };
  }
  return best;
}

export function parsePayload(buffer) {
  const head = new DataView(buffer);
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, 4));
  if (magic !== 'DTM1') throw new Error('not a dharmatiles mesh payload');
  const jsonLen = head.getUint32(4, true);
  const meta = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 8, jsonLen)));
  const n = meta.triangles;
  let o = 8 + jsonLen;
  const positions = new Float32Array(buffer, o, 9 * n); o += 36 * n;
  const normals = new Float32Array(buffer, o, 9 * n); o += 36 * n;
  const painted = new Uint8Array(buffer, o, 9 * n); o += 9 * n;
  const clean = new Uint8Array(buffer, o, 9 * n); o += 9 * n;
  const traced = buffer.byteLength >= o + 4 * n;
  const entry = traced ? new Uint16Array(buffer, o, n) : new Uint16Array(n).fill(NO_ENTRY);
  const piece = traced ? new Uint16Array(buffer, o + 2 * n, n) : new Uint16Array(n);
  return { meta, triangles: n, positions, normals, painted, clean, entry, piece };
}
