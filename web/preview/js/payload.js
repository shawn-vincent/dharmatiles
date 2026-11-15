// @ts-check
// Reads the binary mesh payload (see crates/dharmatiles/src/preview/payload.rs).
//
//   "DTM2" | u32 json_len | json (space padded to 4) |
//   f32 positions[3v] | u32 indices[3n] | u8 palette[3p] (padded to 4) |
//   u16 painted[n] | u16 clean[n] | u16 entry[n] | u16 piece[n]
//
// The wire holds the shared vertices, the indices and one palette index for each face; `parsePayload` lays them out
// as the viewer draws them (three vertices of each triangle, the face normal and the face's colour on each).
//
// entry is a place in the header's `stack`, the stack the mesh was built
// from (the connector, when the tile has a base, is last; 65535 none);
// piece counts the things within it (rock 0, rock 1 of a scatter).
// The entry and piece lists are views onto the one buffer; the positions, normals and colours are made once, here.
//
// The text on screen may have moved on since the mesh was built (an entry
// inserted, deleted, moved), so an entry number is only ever read against
// the mesh's own stack, and what the page keeps of an entry is its key (its
// id, `tile/grass/0`), looked up in whichever stack a part of the page needs.

export const NO_ENTRY = 65535;

/** @typedef {import('./api').MeshHeader} MeshHeader */
/** @typedef {import('./api').MeshStackEntry} MeshStackEntry */
/**
 * A mesh as the page holds it: its header (the program's type) and the typed arrays over the one buffer.
 * @typedef {{ meta: MeshHeader, triangles: number, positions: Float32Array, normals: Float32Array,
 *   painted: Uint8Array, clean: Uint8Array, entry: Uint16Array, piece: Uint16Array }} Payload
 */

/**
 * The stack the mesh was built from (empty for a mesh made without one).
 * @param {Payload | null | undefined} payload
 * @returns {MeshStackEntry[]}
 */
export const meshStack = (payload) => payload?.meta?.stack ?? [];

/**
 * The entry that mesh entry number `index` is (null for none), decoded against the mesh's own stack.
 * @param {Payload | null | undefined} payload
 * @param {number | null | undefined} index
 * @returns {MeshStackEntry | null}
 */
export function meshEntry(payload, index) {
  if (index == null || index === NO_ENTRY) return null;
  return meshStack(payload)[index] ?? null;
}

/**
 * The key of mesh entry number `index`, or null.
 * @param {Payload | null | undefined} payload
 * @param {number | null | undefined} index
 */
export const entryKey = (payload, index) => meshEntry(payload, index)?.key ?? null;

/**
 * The mesh's entry number for `key`, or -1 when the mesh has no such entry.
 * @param {Payload | null | undefined} payload
 * @param {string | null | undefined} key
 */
export const meshIndex = (payload, key) => (key == null ? -1 : meshStack(payload).findIndex((e) => e.key === key));

/**
 * The height each entry's geometry reaches, by key: `Map(key -> [lo, hi])`, printed mm from the tile's zero.  The program
 * measured it in the mesh it built (`stack_json`, payload.rs; `MeshStackEntry.z`); an entry that made no triangle has none.  Nothing is worked
 * out here.
 * @param {Payload | null | undefined} payload
 */
export function bandsByKey(payload) {
  const bands = new Map();
  for (const e of meshStack(payload)) if (Array.isArray(e.z)) bands.set(e.key, /** @type {[number, number]} */ (e.z));
  return bands;
}

/**
 * How many pieces each entry's geometry has, by key: `Map(key -> n)`.
 * @param {Payload} payload
 */
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
/** @param {Uint16Array} entry @param {Uint16Array} piece */
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
 * @param {Payload} payload
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
 * @param {Payload} payload
 * @returns {{ width: number, height: number, red: string, orange: string, reds: number, oranges: number } | null}
 */
export function undersidePlan(payload) {
  const lo = payload.meta?.lo, hi = payload.meta?.hi; // the mesh's own box, in the header of every mesh
  if (!lo || !hi) return null;
  const q = payload.positions;
  const r = (/** @type {number} */ n) => Math.round(n * 100) / 100;
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
 * @param {Payload} payload
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

/** @param {ArrayBuffer} buffer @returns {Payload} */
export function parsePayload(buffer) {
  const head = new DataView(buffer);
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, 4));
  if (magic !== 'DTM2') throw new Error('not a dharmatiles mesh payload');
  const jsonLen = head.getUint32(4, true);
  // the one place the header is read: it is the program's `MeshHeader`, written by serializing that type
  const meta = /** @type {MeshHeader} */ (JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 8, jsonLen))));
  const n = meta.triangles, v = meta.vertices, p = meta.palette;
  let o = 8 + jsonLen;
  const shared = new Float32Array(buffer, o, 3 * v); o += 12 * v;
  const indices = new Uint32Array(buffer, o, 3 * n); o += 12 * n;
  const palette = new Uint8Array(buffer, o, 3 * p); o += (3 * p + 3) & ~3;
  const paintedAt = new Uint16Array(buffer, o, n); o += 2 * n;
  const cleanAt = new Uint16Array(buffer, o, n); o += 2 * n;
  const entry = new Uint16Array(buffer, o, n);
  const piece = new Uint16Array(buffer, o + 2 * n, n);
  const positions = new Float32Array(9 * n), normals = new Float32Array(9 * n);
  const painted = new Uint8Array(9 * n), clean = new Uint8Array(9 * n);
  for (let t = 0; t < n; t++) {
    const k = 9 * t;
    for (let a = 0; a < 3; a++) {
      const i = 3 * indices[3 * t + a];
      positions[k + 3 * a] = shared[i]; positions[k + 3 * a + 1] = shared[i + 1]; positions[k + 3 * a + 2] = shared[i + 2];
      const pa = 3 * paintedAt[t], ca = 3 * cleanAt[t];
      for (let c = 0; c < 3; c++) { painted[k + 3 * a + c] = palette[pa + c]; clean[k + 3 * a + c] = palette[ca + c]; }
    }
    const ax = positions[k + 3] - positions[k], ay = positions[k + 4] - positions[k + 1], az = positions[k + 5] - positions[k + 2];
    const bx = positions[k + 6] - positions[k], by = positions[k + 7] - positions[k + 1], bz = positions[k + 8] - positions[k + 2];
    const nx = ay * bz - az * by, ny = az * bx - ax * bz, nz = ax * by - ay * bx;
    const l = Math.hypot(nx, ny, nz);
    for (let a = 0; a < 3; a++) {
      normals[k + 3 * a] = l > 0 ? nx / l : 0; normals[k + 3 * a + 1] = l > 0 ? ny / l : 0; normals[k + 3 * a + 2] = l > 0 ? nz / l : 0;
    }
  }
  return { meta, triangles: n, positions, normals, painted, clean, entry, piece };
}
