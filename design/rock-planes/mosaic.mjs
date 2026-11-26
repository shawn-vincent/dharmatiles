/* node design/rock-planes/mosaic.mjs <png> [page.html]: find the settings mosaic of Cliffs from Spines anywhere in a screenshot (an L of one colour on a
   contrasting quiet zone, whatever the colours), read it by the palette its clock shows, correct it with Reed-Solomon, and print the state and camera it holds, decoded by the page's own decodeBits (cliffs.html beside this file by default; a
   mosaic made by another version of the page needs that version, from git). Plain node, nothing to install. */
import fs from 'node:fs'; import zlib from 'node:zlib'; import { fileURLToPath } from 'node:url';
/* the page's settings code and mosaic, without the DOM: the engine's constants, then the page-state region up to the end of the settings code */
function load(file, eye = { position: [10, -50, 30], target: [0, 0, 20] }) {
  const src = fs.readFileSync(file, 'utf8').split('<script>\n')[1];
  const eng = src.slice(src.indexOf('"use strict";'), src.indexOf('// ---------- drawing ----------'));
  const a = src.indexOf('// ---------- page state ----------'), b = src.indexOf('// ---------- end of settings code ----------');
  const f = new Function('eye', eng + '\n' + src.slice(a, b) + `
    const camera = { position: { x: eye.position[0], y: eye.position[1], z: eye.position[2] } }, controls = { target: { x: eye.target[0], y: eye.target[1], z: eye.target[2] } };
    return { encode, decode, encodeBits, decodeBits, codeBits, bytesOf, mosaicOf, mosaicWord, MOSAIC, mosaicColours, rsParityCount, GF, GROUPS, PRESETS, DEFAULTS, SETTINGS, CODE_FP, VIEW, state, edited, baseOf, setEye(e) { camera.position.x = e.position[0]; camera.position.y = e.position[1]; camera.position.z = e.position[2]; controls.target.x = e.target[0]; controls.target.y = e.target[1]; controls.target.z = e.target[2]; } };`);
  return f(eye);
}

/* a PNG's pixels as RGBA bytes (8 bits a channel, not interlaced: greyscale, RGB, palette, grey with alpha, RGBA) */
function readPNG(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let o = 8, W = 0, H = 0, depth = 0, type = 0, inter = 0, pal = null, trns = null; const idat = [];
  while (o < buf.length) { const n = buf.readUInt32BE(o), t = buf.toString('latin1', o + 4, o + 8), d = buf.subarray(o + 8, o + 8 + n); o += 12 + n;
    if (t === 'IHDR') { W = d.readUInt32BE(0); H = d.readUInt32BE(4); depth = d[8]; type = d[9]; inter = d[12]; }
    else if (t === 'PLTE') pal = d; else if (t === 'tRNS') trns = d; else if (t === 'IDAT') idat.push(d); else if (t === 'IEND') break; }
  if (depth !== 8 || inter) throw new Error(`PNG with ${depth}-bit channels${inter ? ', interlaced' : ''}: not read here`);
  const ch = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[type], raw = zlib.inflateSync(Buffer.concat(idat)), stride = W * ch, px = Buffer.alloc(H * stride);
  for (let y = 0, p = 0; y < H; y++) { const f = raw[p++], row = y * stride;
    for (let x = 0; x < stride; x++, p++) { const a = x >= ch ? px[row + x - ch] : 0, b = y ? px[row - stride + x] : 0, c = x >= ch && y ? px[row - stride + x - ch] : 0;
      const pr = a + b - c, pa = Math.abs(pr - a), pb = Math.abs(pr - b), pc = Math.abs(pr - c);
      px[row + x] = (raw[p] + [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][f]) & 255; } }
  const data = Buffer.alloc(W * H * 4);
  for (let i = 0; i < W * H; i++) { const s = i * ch, q = i * 4;
    if (type === 3) { const k = px[s]; data[q] = pal[3 * k]; data[q + 1] = pal[3 * k + 1]; data[q + 2] = pal[3 * k + 2]; data[q + 3] = trns && k < trns.length ? trns[k] : 255; }
    else if (ch <= 2) { data[q] = data[q + 1] = data[q + 2] = px[s]; data[q + 3] = ch === 2 ? px[s + 1] : 255; }
    else { data[q] = px[s]; data[q + 1] = px[s + 1]; data[q + 2] = px[s + 2]; data[q + 3] = ch === 4 ? px[s + 3] : 255; } }
  return { width: W, height: H, data };
}
const MAIN = process.argv[1] === fileURLToPath(import.meta.url), PAGE = (MAIN && process.argv[3]) || fileURLToPath(new URL('cliffs.html', import.meta.url)), P = load(PAGE);
const { exp, log } = P.GF; const mul = (a, b) => a && b ? exp[log[a] + log[b]] : 0, inv = a => exp[255 - log[a]];
/* Berlekamp-Massey over GF(256), roots 1, 2, 4, ...: the corrected word and the number of bytes it fixed, or null */
export function rsDecode(word, nsym) {
  const n = word.length, w = Uint8Array.from(word), S = [];
  for (let i = 0; i < nsym; i++) { let v = 0; for (const b of w) v = mul(v, exp[i]) ^ b; S.push(v); }
  if (!S.some(Boolean)) return { word: w, fixed: 0 };
  let C = [1], B = [1], L = 0, m = 1, b = 1;
  for (let k = 0; k < nsym; k++) {
    let d = S[k]; for (let i = 1; i <= L; i++) d ^= mul(C[i] || 0, S[k - i]);
    if (!d) { m++; continue; }
    const T = C.slice(), coef = mul(d, inv(b)); while (C.length < B.length + m) C.push(0);
    B.forEach((x, i) => { C[i + m] ^= mul(coef, x); });
    if (2 * L <= k) { L = k + 1 - L; B = T; b = d; m = 1; } else m++;
  }
  const pos = []; for (let j = 0; j < n; j++) { const X = exp[(n - 1 - j) % 255], Xi = inv(X); let v = 0, p = 1; for (const c of C) { v ^= mul(c, p); p = mul(p, Xi); } if (!v) pos.push(j); }
  if (pos.length !== L || L * 2 > nsym) return null;
  const omega = Array(nsym).fill(0); for (let i = 0; i < nsym; i++) for (let j = 0; j < C.length && i + j < nsym; j++) omega[i + j] ^= mul(S[i], C[j]);
  for (const j of pos) {
    const X = exp[(n - 1 - j) % 255], Xi = inv(X); let o = 0, p = 1; for (const c of omega) { o ^= mul(c, p); p = mul(p, Xi); }
    let dd = 0; for (let i = 1; i < C.length; i += 2) dd ^= mul(C[i], exp[(log[Xi] * (i - 1)) % 255]);
    if (!dd) return null; w[j] ^= mul(mul(X, o), inv(dd));
  }
  for (let i = 0; i < nsym; i++) { let v = 0; for (const x of w) v = mul(v, exp[i]) ^ x; if (v) return null; }
  return { word: w, fixed: pos.length };
}
const SIZES = P.MOSAIC.sizes;
export function read(png) {
  const { width: W, height: H, data } = png, at = (x, y) => (y * W + x) * 4, inside = (x, y) => x >= 0 && y >= 0 && x < W && y < H;
  const px = (x, y) => inside(x, y) ? [data[at(x, y)], data[at(x, y) + 1], data[at(x, y) + 2]] : [0, 0, 0], dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
  const TOL = 80; let lastError = 'no mosaic found';
  /* the L is one colour (the seed): runs of pixels within TOL of it, and the fraction of it in an edge pixel, measured from the quiet zone's colour just outside */
  const same = (x, y, seed) => inside(x, y) && dist(px(x, y), seed) < TOL;
  const vrun = (x, y, seed) => { let a = y, b = y; while (same(x, a - 1, seed)) a--; while (same(x, b + 1, seed)) b++; return [a, b]; };
  const hrun = (x, y, seed) => { let a = x, b = x; while (same(a - 1, y, seed)) a--; while (same(b + 1, y, seed)) b++; return [a, b]; };
  /* how much of an edge pixel is the L's colour: its place on the line from the quiet zone's colour q to the seed */
  const cover = (p, q, seed) => { const n = dist(q, seed) ** 2; return n < 1 ? 0 : Math.max(0, Math.min(1, ((p[0] - q[0]) * (seed[0] - q[0]) + (p[1] - q[1]) * (seed[1] - q[1]) + (p[2] - q[2]) * (seed[2] - q[2])) / n)); };
  /* the quiet zone's colour just outside an edge: the one of the two pixels out from it that is most unlike the seed (the nearer may be part L) */
  const outside = (x, y, dx, dy, seed) => { const a = px(x + dx, y + dy), b = px(x + 2 * dx, y + 2 * dy); return dist(a, seed) >= dist(b, seed) ? a : b; };
  /* the edge's place from the run's last same pixel (x, y) going outward by (dx, dy): the pixel and the one beyond it hold what is left of a cell's width */
  const edge = (x, y, dx, dy, seed) => { const q = outside(x, y, dx, dy, seed); return cover(px(x, y), q, seed) + cover(px(x + dx, y + dy), q, seed); };
  for (let x = 1; x < W; x++) for (let y = 0; y < H; y++) {
    /* a run starts at a pixel like the one two below it and unlike the one above it (the first pixel of an edge may be half quiet zone, so it is not the colour to compare with; the pixel above must be clearly unlike, so every edge has a start) */
    const seed0 = px(x, y + 2); if (!same(x, y, seed0) || dist(px(x, y - 1), seed0) < 0.6 * TOL) continue;
    let [, y1] = vrun(x, y, seed0); if (y1 - y + 1 < 8) continue;
    /* the colour of the L is the mean of the middle of the run on the column of it that is most unlike the quiet zone (the first column may be part quiet) */
    const mid = (y + y1) >> 1, q4 = (y1 - y) >> 2, seed = [0, 0, 0], quiet = outside(x - 1, mid, -1, 0, seed0).slice(), vx = same(x + 1, mid, seed0) && dist(px(x + 1, mid), quiet) > dist(px(x, mid), quiet) ? x + 1 : x; let cnt = 0;
    for (let j = y + q4; j <= y1 - q4; j++) { const v = px(vx, j); for (let t = 0; t < 3; t++) seed[t] += v[t]; cnt++; }
    for (let t = 0; t < 3; t++) seed[t] /= cnt;
    if (same(x - 1, mid, seed)) continue;
    /* the column's ends are rounded by the corner pixels, so the run is taken at this column and the next */
    const [ya, yb_] = vrun(x + 1, mid, seed), [yc, yd] = vrun(x, mid, seed); const top = Math.min(ya, yc), bot = Math.max(yb_, yd);
    const Lv = bot - top + 1; if (Lv < 14 || Lv > 120) continue;
    y1 = bot; const y0 = top;
    for (const N of SIZES) {
      /* the bottom row of the L is read on the row of its last full pixels, taken at the second cell, where the cell above is not the L's colour */
      const s = Lv / N, cx2 = Math.min(W - 1, x + Math.ceil(1.5 * s)), yr = dist(px(cx2, y1), seed) <= dist(px(cx2, y1 - 1), seed) ? y1 : y1 - 1; if (!same(cx2, yr, seed)) continue;
      const [, xa] = hrun(x, yr, seed), [, xb] = hrun(x, Math.max(y0, yr - 1), seed); const x1 = Math.max(xa, xb);
      const Lh = x1 - x + 1; if (Math.abs(Lh - Lv) > 0.15 * s + 3) continue;
      /* the edges are measured on the column and the row that are most the L's colour (the bottom row's is the row it was found on), where the other direction's blend is least */
      const cx = dist(px(x, mid), seed) <= dist(px(x + 1, mid), seed) ? x : x + 1;
      const xl = x + 1 - edge(x, mid, -1, 0, seed), xr = x1 + edge(x1, yr, 1, 0, seed), yt = y0 + 1 - edge(cx, y0, 0, -1, seed), yb = y1 + edge(cx, y1, 0, 1, seed);
      const sx = (xr - xl) / N, sy = (yb - yt) / N;
      const sample = (r, c) => {
        const cx = xl + (c + 0.5) * sx, cy = yt + (r + 0.5) * sy, hx = 0.3 * sx, hy = 0.3 * sy; let n = 0, R = 0, G = 0, B = 0;
        for (let j = Math.ceil(cy - hy - 0.5); j <= Math.floor(cy + hy - 0.5); j++) for (let i = Math.ceil(cx - hx - 0.5); i <= Math.floor(cx + hx - 0.5); i++) { if (i < 0 || j < 0 || i >= W || j >= H) continue; R += data[at(i, j)]; G += data[at(i, j) + 1]; B += data[at(i, j) + 2]; n++; }
        if (!n) { const i = Math.min(W - 1, Math.max(0, Math.floor(cx))), j = Math.min(H - 1, Math.max(0, Math.floor(cy))); return [data[at(i, j)], data[at(i, j) + 1], data[at(i, j) + 2]]; }
        return [R / n, G / n, B / n];
      };
      const cache = new Map(), cell = (r, c) => { const k = r * 64 + c; if (!cache.has(k)) cache.set(k, sample(r, c)); return cache.get(k); };
      /* the borders: the left column and bottom row are the L's colour, the clock's even cells are too and its odd cells are another colour */
      const Lc = [0, 1, 2].map(t => { let sum = 0; for (let i = 0; i < N; i++) sum += cell(i, 0)[t] + cell(N - 1, i)[t]; return sum / (2 * N); });
      const near = v => dist(v, Lc) < 45; let ok = true;
      for (let i = 0; i < N && ok; i++) {
        if (!near(cell(i, 0)) || !near(cell(N - 1, i))) ok = false;
        if ((i % 2 === 0) !== near(cell(0, i))) ok = false;
        if (((N - 1 - i) % 2 === 0) !== near(cell(i, N - 1))) ok = false;
      }
      if (!ok) { lastError = `a border did not match N=${N} s=${sx.toFixed(2)}`; continue; }
      /* calibration: colour 0 from the finder L, each other colour from every clock cell that shows it */
      const acc = Array.from({ length: 8 }, () => ({ n: 0, v: [0, 0, 0] })), add = (k, v) => { acc[k].n++; for (let t = 0; t < 3; t++) acc[k].v[t] += v[t]; };
      for (let i = 0; i < N; i++) { add(0, cell(i, 0)); add(0, cell(N - 1, i)); if (i % 2 === 0) { add(0, cell(0, i)); } else add(1 + (((i - 1) >> 1) % 7), cell(0, i)); const j = N - 1 - i; if (j % 2 === 0) add(0, cell(i, N - 1)); else add(1 + (((j - 1) >> 1) % 7), cell(i, N - 1)); }
      const pal = acc.map(a => a.v.map(t => t / a.n)); let close = false;
      for (let a = 0; a < 8; a++) for (let b = a + 1; b < 8; b++) if (Math.hypot(...pal[a].map((t, i) => t - pal[b][i])) < 50) close = true;
      if (close) { lastError = 'the colours are not distinct'; continue; }
      const idx = (r, c) => { const v = cell(r, c); let best = 0, bd = 1e9; pal.forEach((p, k) => { const d = (p[0] - v[0]) ** 2 + (p[1] - v[1]) ** 2 + (p[2] - v[2]) ** 2; if (d < bd) { bd = d; best = k; } }); return best; };
      let k, word; try { ({ k, word } = P.mosaicWord(N, idx)); } catch (e) { lastError = e.message; continue; }
      const fixed = rsDecode(word, word.length - k);
      if (!fixed) { lastError = `Reed-Solomon could not correct the ${N}x${N} icon at ${Math.round(xl)},${Math.round(yt)} (cell ${sx.toFixed(2)} px)`; continue; }
      let out; try { out = P.decodeBits(fixed.word.slice(0, k)); } catch (e) { lastError = 'decoded bytes, but ' + e.message; continue; }
      return { N, k, fixed: fixed.fixed, at: [Math.round(xl), Math.round(yt)], cell: [sx, sy], bytes: fixed.word.slice(0, k), ...out };
    }
  }
  throw new Error(lastError);
}
export const readFile = f => read(readPNG(fs.readFileSync(f)));
if (MAIN) {
  const f = process.argv[2]; if (!f) { console.error('usage: node design/rock-planes/mosaic.mjs <png> [page.html]'); process.exit(2); }
  try {
    const r = readFile(f), { values, view } = r;
    console.log(`found a ${r.N}x${r.N} mosaic at ${r.at.join(',')}, cell ${r.cell.map(c => c.toFixed(2)).join('x')} px, ${r.k} bytes, ${r.fixed} corrected`);
    console.log(`style ${values.style}\nseed ${values.seed}`);
    for (const p of P.SETTINGS) if (!p.own) { const v = values[p.key], b = P.baseOf(p, values.style); console.log(`${p.key} ${Array.isArray(v) ? v[0] === v[1] ? v[0] : v.join('..') : v}${String(v) !== String(b) ? '   *changed from the preset' : ''}`); }
    if (view) console.log(`draw ${view.draw}\nview ${view.view}\ncamera ${view.position.join(' ')}\ntarget ${view.target.join(' ')}`); else console.log('(no camera part)');
  } catch (e) { console.error('cannot read: ' + e.message); process.exit(1); }
}
