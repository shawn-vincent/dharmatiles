// Reads the binary mesh payload (see crates/dharmatiles/src/preview/payload.rs).
//
//   "DTM1" | u32 json_len | json (space padded to 4) |
//   f32 positions[9n] | f32 normals[9n] | u8 painted[9n] | u8 clean[9n]
//
// Everything is a typed-array view onto the one buffer: no copies, no JSON numbers.

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
  const clean = new Uint8Array(buffer, o, 9 * n);
  return { meta, triangles: n, positions, normals, painted, clean };
}
