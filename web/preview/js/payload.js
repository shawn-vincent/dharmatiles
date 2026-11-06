// Reads the binary mesh payload (see crates/dharmatiles/src/preview/payload.rs).
//
//   "DTM1" | u32 json_len | json (space padded to 4) |
//   f32 positions[9n] | f32 normals[9n] | u8 painted[9n] | u8 clean[9n] |
//   u16 entry[n] | u16 piece[n] | padding to 4
//
// entry is the index of the stack entry that made each triangle (the base is
// the stack's length, 65535 none); piece counts the things within it (rock 0,
// rock 1 of a scatter).  Everything is a typed-array view onto the one
// buffer: no copies, no JSON numbers.

export const NO_ENTRY = 65535;

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
