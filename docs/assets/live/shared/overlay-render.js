// The flat renderer: the overlay's marks (overlay-marks.js) drawn on a 2D canvas over any picture of the tile, the poster before
// the engine wakes or the live 3D view after.  The editor draws the same marks into its 3D scene (overlay.js); both take their
// shapes from overlay-geometry.js.  The caller says where a point of the tile falls on the canvas (`project`, mm to canvas px),
// so this draws over a flat top picture and over a camera that orbits alike.
//
// Colours are the visual system's tokens (--accent for lit, --plate-ink for shown, --point for points, --ref for fill points):
// never red or orange, which mean overhang on a model.

import { lineHalf, regionAnchor, refAnchor, regionTriangles, ribbonTriangles } from './overlay-geometry.js';
import { token } from './tokens.js';

const css = (/** @type {string} */ name) => token(name) || '#888';

/**
 * @typedef {(x: number, y: number, z?: number) => { x: number, y: number }} Project
 * @typedef {{ mark: import('./overlay-marks.js').Mark, state: 'shown' | 'lit' }} Drawn
 */

/** @param {CanvasRenderingContext2D} c @param {Float32Array} tris @param {Project} project @param {number} z */
function fillTriangles(c, tris, project, z) {
  c.beginPath();
  for (let i = 0; i < tris.length; i += 9) {
    const a = project(tris[i], tris[i + 1], z), b = project(tris[i + 3], tris[i + 4], z), d = project(tris[i + 6], tris[i + 7], z);
    c.moveTo(a.x, a.y); c.lineTo(b.x, b.y); c.lineTo(d.x, d.y); c.closePath();
  }
  c.fill();
}

/** A small label with a ground of its own, so it reads over any picture. */
function tag(/** @type {CanvasRenderingContext2D} */ c, x, y, text, fill, ink) {
  c.font = '500 12px "JetBrains Mono", monospace';
  const w = c.measureText(text).width + 10;
  c.globalAlpha = 0.94;
  c.fillStyle = fill;
  c.fillRect(x, y - 16, w, 18);
  c.globalAlpha = 1;
  c.fillStyle = ink;
  c.textBaseline = 'alphabetic';
  c.fillText(text, x + 5, y - 3);
  return w;
}

/**
 * Draw the marks.  `map` is the program's map JSON, `sq` mm per square, `drawn` what `drawn(marks, moment)` returned, `squares`
 * whether to draw the squares' grid.  `zAt(x, y)` is the ground's height under a point, mm (0 over a flat picture).
 * @param {CanvasRenderingContext2D} c
 * @param {{ map: any, sq: number, drawn: Drawn[], squares?: boolean, project: Project, zAt?: (x: number, y: number) => number, size: { w: number, h: number } }} o
 */
export function renderMarks(c, { map, sq, drawn, squares = false, project, zAt = () => 0, size }) {
  c.clearRect(0, 0, size.w, size.h);
  if (!map) return;
  const accent = css('--accent'), ink = css('--plate-ink'), point = css('--point'), onPoint = css('--on-point'), ref = css('--ref'), onRef = css('--on-ref');
  const half = lineHalf(map, sq);
  const at = (/** @type {number[]} */ q, up = 0.3) => { const x = q[0] * sq, y = q[1] * sq; return project(x, y, zAt(x, y) + up); };
  c.save();
  c.lineJoin = 'round';
  c.lineCap = 'round';

  if (squares) {
    c.strokeStyle = ink; c.globalAlpha = 0.55; c.lineWidth = 1; c.setLineDash([5, 4]);
    for (let i = 0; i <= map.cols; i++) { const a = at([i, 0]), b = at([i, map.rows]); c.beginPath(); c.moveTo(a.x, a.y); c.lineTo(b.x, b.y); c.stroke(); }
    for (let j = 0; j <= map.rows; j++) { const a = at([0, j]), b = at([map.cols, j]); c.beginPath(); c.moveTo(a.x, a.y); c.lineTo(b.x, b.y); c.stroke(); }
    c.setLineDash([]);
    c.globalAlpha = 1;
    // squares are named column then row: C1 R1 is the north-west one
    // (the map counts y up from the south, so row 1 is the highest band)
    for (let r = 1; r <= map.rows; r++) for (let i = 0; i < map.cols; i++) {
      const p = at([i + 0.03, map.rows - r + 0.97]);
      tag(c, p.x + 3, p.y + 20, `C${i + 1} R${r}`, ink, css('--bg') || '#fff');
    }
  }

  // regions under lines, lines under points, lit over shown
  const order = { region: 0, line: 1, fill: 2, point: 2, square: 3 };
  const list = [...drawn].sort((a, b) => (a.state === 'lit') - (b.state === 'lit') || order[a.mark.kind] - order[b.mark.kind]);
  const labelled = [];
  for (const { mark, state } of list) {
    const lit = state === 'lit';
    if (mark.kind === 'region') {
      const r = map.regions.find((/** @type {any} */ x) => x.name === mark.name);
      if (!r) continue;
      c.fillStyle = lit ? accent : ink;
      c.globalAlpha = lit ? 0.42 : 0.2;
      fillTriangles(c, regionTriangles(r, map, sq), project, zAt(map.cols * sq / 2, map.rows * sq / 2) + 0.2);
      c.globalAlpha = 1;
      const a = regionAnchor(r, map);
      if (a && lit) labelled.push({ at: a, text: r.name, fill: accent, ink: css('--on-accent') });
    } else if (mark.kind === 'line') {
      const l = map.lines.find((/** @type {any} */ x) => x.name === mark.name);
      if (!l || l.pts.length < 2) continue;
      const pts = l.pts.map((/** @type {number[]} */ q) => at(q));
      // how far it may wander: the band of its wobble, under the line
      if (l.wobbleMm && lit) {
        c.fillStyle = accent; c.globalAlpha = 0.2;
        fillTriangles(c, ribbonTriangles(l.pts, sq, l.wobbleMm, false, false), project, zAt(l.pts[0][0] * sq, l.pts[0][1] * sq) + 0.2);
      }
      c.globalAlpha = lit ? 1 : 0.85;
      c.strokeStyle = lit ? accent : ink;
      c.lineWidth = lit ? 4 : 2;
      if (!lit) c.setLineDash([6, 4]);
      c.beginPath();
      pts.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
      c.stroke();
      c.setLineDash([]);
      c.globalAlpha = 1;
      if (lit) {
        const a = refAnchor({ kind: 'line', pts: l.pts }, map);
        if (a) labelled.push({ at: a, text: l.name, fill: accent, ink: css('--on-accent') });
      }
      void half;
    }
  }
  // points last, on top: a ring and a dot, with what was written beside the lit one
  for (const { mark, state } of list) {
    if ((mark.kind !== 'point' && mark.kind !== 'fill') || !mark.at) continue;
    const lit = state === 'lit';
    const p = at(mark.at);
    const fill = mark.kind === 'fill' ? ref : point;
    c.beginPath(); c.arc(p.x, p.y, lit ? 7 : 5, 0, 7);
    c.lineWidth = 3; c.strokeStyle = mark.kind === 'fill' ? onRef : onPoint; c.stroke();
    c.lineWidth = lit ? 3 : 2; c.strokeStyle = lit ? accent : fill; c.stroke();
    c.fillStyle = fill; c.fill();
    if (lit && mark.text) labelled.push({ px: { x: p.x + 11, y: p.y - 6 }, text: mark.text, fill, ink: mark.kind === 'fill' ? onRef : onPoint });
  }
  // labels keep clear of each other: a few steps up when one is taken
  const taken = [];
  c.font = '500 12px "JetBrains Mono", monospace';
  for (const l of labelled) {
    const w = c.measureText(l.text).width + 10;
    let { x, y } = l.px ?? (() => { const p = at(l.at); return { x: p.x - w / 2, y: p.y - 10 }; })();
    for (let k = 0; k < 5 && taken.some((t) => x < t.x1 && x + w > t.x0 && y - 16 < t.y1 && y + 2 > t.y0); k++) y -= 20;
    taken.push({ x0: x, x1: x + w, y0: y - 16, y1: y + 2 });
    tag(c, x, y, l.text, l.fill, l.ink);
  }
  c.restore();
}

/**
 * Which mark is under canvas point (x, y): a shown point within `px`, else a shown line within `px`, else the region whose
 * cells hold it.  Only marks that are drawn can be pointed at.  Null for none.
 * @param {{ map: any, sq: number, drawn: Drawn[], project: Project, zAt?: (x: number, y: number) => number }} o
 * @returns {string | null}
 */
export function markAtPoint({ map, sq, drawn, project, zAt = () => 0 }, x, y, px = 9) {
  if (!map) return null;
  const at = (/** @type {number[]} */ q) => project(q[0] * sq, q[1] * sq, zAt(q[0] * sq, q[1] * sq) + 0.3);
  let best = null, d = px;
  for (const { mark } of drawn) {
    if ((mark.kind !== 'point' && mark.kind !== 'fill') || !mark.at) continue;
    const p = at(mark.at), e = Math.hypot(p.x - x, p.y - y);
    if (e <= d) { d = e; best = mark.id; }
  }
  if (best) return best;
  for (const { mark } of drawn) {
    if (mark.kind !== 'line') continue;
    const l = map.lines.find((/** @type {any} */ k) => k.name === mark.name);
    const pts = (l?.pts ?? []).map(at);
    for (let n = 1; n < pts.length; n++) {
      const a = pts[n - 1], b = pts[n];
      const vx = b.x - a.x, vy = b.y - a.y, len = vx * vx + vy * vy || 1;
      const t = Math.max(0, Math.min(1, ((x - a.x) * vx + (y - a.y) * vy) / len));
      const e = Math.hypot(a.x + t * vx - x, a.y + t * vy - y);
      if (e < d) { d = e; best = mark.id; }
    }
  }
  if (best) return best;
  return null;
}

/**
 * The region (id) holding tile point (mx, my) in mm, among the drawn marks: the last listed wins, as the editor's does.
 * @param {{ map: any, sq: number, drawn: Drawn[] }} o
 */
export function regionAtTile({ map, sq, drawn }, mx, my) {
  if (!map) return null;
  const i = Math.round((mx / sq) * map.cells), j = Math.round((my / sq) * map.cells);
  let hit = null;
  for (const { mark } of drawn) {
    if (mark.kind !== 'region') continue;
    const r = map.regions.find((/** @type {any} */ k) => k.name === mark.name);
    if (r && i >= 0 && j >= 0 && i < r.nx && j < r.ny && r.bits[j * r.nx + i] === '1') hit = mark.id;
  }
  return hit;
}
