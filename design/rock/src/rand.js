/* rand.js — owns every random number in the stone. There is no stateful generator: a number is a pure function of
   (seed, stage, purpose, indices), so no stage reads another's draws and raising a strength only adds items.
   laid(…, strength) is the one rule "a strength thresholds a fixed draw". */
var Stone = Stone || {};
(function (S) {
  const imul = Math.imul, fl = Math.floor;
  const hash3 = (x, y, z, s) => {
    let h = imul(x | 0, 374761393) ^ imul(y | 0, 668265263) ^ imul(z | 0, 2147483629) ^ imul(s | 0, 1274126177);
    h = imul(h ^ (h >>> 13), 1274126177); h ^= h >>> 16; h = imul(h, 2246822519); h ^= h >>> 13;
    return (h >>> 0) / 4294967296;
  };
  const fnv = (str) => { let h = 2166136261; for (let i = 0; i < str.length; i++) h = imul(h ^ str.charCodeAt(i), 16777619); return h | 0; };
  const fold = (salt, d) => (d ? imul(salt ^ imul(d | 0, 0x9E3779B1), 0x85EBCA6B) : salt);   // a fourth index joins the salt
  // trilinear value noise in [0,1), keyed by an integer salt
  function noise(x, y, z, s) {
    const ix = fl(x), iy = fl(y), iz = fl(z); let fx = x - ix, fy = y - iy, fz = z - iz;
    fx = fx * fx * (3 - 2 * fx); fy = fy * fy * (3 - 2 * fy); fz = fz * fz * (3 - 2 * fz);
    const a = hash3(ix, iy, iz, s), b = hash3(ix + 1, iy, iz, s), c = hash3(ix, iy + 1, iz, s), d = hash3(ix + 1, iy + 1, iz, s),
      e = hash3(ix, iy, iz + 1, s), f = hash3(ix + 1, iy, iz + 1, s), g = hash3(ix, iy + 1, iz + 1, s), h = hash3(ix + 1, iy + 1, iz + 1, s);
    const x1 = a + (b - a) * fx, x2 = c + (d - c) * fx, x3 = e + (f - e) * fx, x4 = g + (h - g) * fx;
    const y1 = x1 + (x2 - x1) * fy, y2 = x3 + (x4 - x3) * fy;
    return y1 + (y2 - y1) * fz;
  }
  function stream(seed) {
    const salts = new Map();
    const salt = (stage, purpose) => { const k = stage + '/' + purpose; let s = salts.get(k); if (s === undefined) { s = (fnv(k) ^ imul(seed | 0, 0x9E3779B1)) | 0; salts.set(k, s); } return s; };
    return {
      seed,
      salt,                                                                       // an integer key for noise(…, salt)
      unit: (stage, purpose, a = 0, b = 0, c = 0, d = 0) => hash3(a, b, c, fold(salt(stage, purpose), d)),
      laid: (strength, stage, purpose, a = 0, b = 0, c = 0, d = 0) => hash3(a, b, c, fold(salt(stage, purpose), d)) < strength,
      // a hasher with some indices already folded in, for hot loops: h(i, j, k, l) in [0,1)
      keyed: (stage, purpose, ...pre) => { let s = salt(stage, purpose); for (const p of pre) s = fold(s, p + 1); return (a, b, c, d = 0) => hash3(a, b, c, fold(s, d)); }
    };
  }
  S.rand = { stream, hash3, fnv, noise };
})(Stone);
