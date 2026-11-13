// The overlay: the tile's map drawn over the view (docs/design/editor.html,
// "The stack and the map"): the selected line with its wobble, or the
// selected region with its name and fill points; every line and region
// while a tool is held.  Nothing selected, nothing drawn.  The tools draw here too: a footprint under the pointer, the
// line being drawn, the box being dragged, and the handles of the
// selection.  Points are in squares; lines and tools lie on the ground (heights from
// the surfaces the build made); regions lie at level zero.

import { token } from './tokens.js';

/**
 * A region's lattice cells as flat triangles (x, y in mm, z 0: the caller sets the height): one quad per run of set nodes in a row, clipped to the tile.
 * Returns a Float32Array of 9 numbers per triangle; `sq` is mm per square.
 */
export function regionTriangles(r, m, sq = m.sq ?? 35) {
  const out = [];
  const step = 1 / m.cells;
  for (let j = 0; j < r.ny; j++) {
    let i = 0;
    while (i < r.nx) {
      if (r.bits[j * r.nx + i] !== '1') { i++; continue; }
      const a = i;
      while (i < r.nx && r.bits[j * r.nx + i] === '1') i++;
      const x0 = Math.max(0, (a - 0.5) * step) * sq, x1 = Math.min(m.cols, (i - 0.5) * step) * sq;
      const y0 = Math.max(0, (j - 0.5) * step) * sq, y1 = Math.min(m.rows, (j + 0.5) * step) * sq;
      if (x1 <= x0 || y1 <= y0) continue;
      out.push(x0, y0, 0, x1, y0, 0, x1, y1, 0, x0, y0, 0, x1, y1, 0, x0, y1, 0);
    }
  }
  return new Float32Array(out);
}


/**
 * A polyline (points in squares) as a ribbon in the 3D scene: a flat strip 2*`half` mm wide at z 0
 * and a curtain from -`half` to `half` along it, so it reads from above and from the side.
 * Returns a Float32Array of triangles (9 numbers each); `closed` joins the last point to the first;
 * `curtain` false leaves out the upright strip (a flat band).
 */
export function ribbonTriangles(pts, sq, half = 0.7, closed = false, curtain = true) {
  const out = [];
  const n = pts.length;
  for (let k = 0; k < (closed ? n : n - 1); k++) {
    const a = pts[k], b = pts[(k + 1) % n];
    let ax = a[0] * sq, ay = a[1] * sq, bx = b[0] * sq, by = b[1] * sq;
    const len = Math.hypot(bx - ax, by - ay);
    if (!len) continue;
    const ux = (bx - ax) / len, uy = (by - ay) / len;
    ax -= ux * half; ay -= uy * half; bx += ux * half; by += uy * half; // butt joints meet
    const nx = -uy * half, ny = ux * half;
    out.push(ax - nx, ay - ny, 0, bx - nx, by - ny, 0, bx + nx, by + ny, 0, ax - nx, ay - ny, 0, bx + nx, by + ny, 0, ax + nx, ay + ny, 0);
    if (curtain) out.push(ax, ay, -half, bx, by, -half, bx, by, half, ax, ay, -half, bx, by, half, ax, ay, half);
  }
  return new Float32Array(out);
}

/** The outline of a region's lattice cells as ribbons (mm; not along the tile's own border). */
export function outlineTriangles(r, m, sq = m.sq ?? 35, half = 0.5) {
  const step = 1 / m.cells, segs = [];
  const set = (i, j) => i >= 0 && j >= 0 && i < r.nx && j < r.ny && r.bits[j * r.nx + i] === '1';
  const clampX = (v) => Math.min(m.cols, Math.max(0, v)), clampY = (v) => Math.min(m.rows, Math.max(0, v));
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) {
    if (!set(i, j)) continue;
    const x0 = clampX((i - 0.5) * step), x1 = clampX((i + 0.5) * step), y0 = clampY((j - 0.5) * step), y1 = clampY((j + 0.5) * step);
    if (x1 <= x0 || y1 <= y0) continue;
    if (i > 0 && !set(i - 1, j)) segs.push([[x0, y0], [x0, y1]]);
    if (i < r.nx - 1 && !set(i + 1, j)) segs.push([[x1, y0], [x1, y1]]);
    if (j > 0 && !set(i, j - 1)) segs.push([[x0, y0], [x1, y0]]);
    if (j < r.ny - 1 && !set(i, j + 1)) segs.push([[x0, y1], [x1, y1]]);
  }
  const parts = segs.map((g) => ribbonTriangles(g, sq, half));
  const all = new Float32Array(parts.reduce((a, p) => a + p.length, 0));
  let at = 0;
  for (const p of parts) { all.set(p, at); at += p.length; }
  return all;
}

/** A point (squares) as a flat disc of radius `rad` mm, with a ring round it as a ribbon's worth of height. */
export function discTriangles(q, sq, rad = 1.6, sides = 20) {
  const out = [];
  const cx = q[0] * sq, cy = q[1] * sq;
  for (let k = 0; k < sides; k++) {
    const a = (k / sides) * Math.PI * 2, b = ((k + 1) / sides) * Math.PI * 2;
    out.push(cx, cy, 0, cx + rad * Math.cos(a), cy + rad * Math.sin(a), 0, cx + rad * Math.cos(b), cy + rad * Math.sin(b), 0);
  }
  return new Float32Array(out);
}

/** Where to hang a label on a place (squares): the middle of a line, the point itself, the set cell nearest a region's centre. */
export function refAnchor(ref, map) {
  if (ref.at) return ref.at;
  if (ref.kind === 'region') {
    const r = map.regions.find((x) => x.name === ref.name);
    return r ? regionAnchor(r, map) : null;
  }
  const pts = ref.pts || [];
  if (pts.length < 2) return pts[0] || null;
  // a loop (all four edges, the tile) is labelled on its first side
  const closed = pts.length > 2 && pts[0][0] === pts[pts.length - 1][0] && pts[0][1] === pts[pts.length - 1][1];
  const use = closed ? pts.slice(0, 2) : pts;
  const seg = use.slice(1).map((p, k) => Math.hypot(p[0] - use[k][0], p[1] - use[k][1]));
  let left = seg.reduce((a, b) => a + b, 0) / 2;
  for (let k = 0; k < seg.length; k++) {
    if (left <= seg[k] || k === seg.length - 1) {
      const t = seg[k] ? Math.min(1, left / seg[k]) : 0;
      return [use[k][0] + (use[k + 1][0] - use[k][0]) * t, use[k][1] + (use[k + 1][1] - use[k][1]) * t];
    }
    left -= seg[k];
  }
  return use[0];
}

/** The set cell of a region nearest its centre of mass, in squares (null for an empty region). */
export function regionAnchor(r, m) {
  const step = 1 / m.cells;
  let sx = 0, sy = 0, n = 0;
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) if (r.bits[j * r.nx + i] === '1') { sx += i; sy += j; n++; }
  if (!n) return null;
  const cx = sx / n, cy = sy / n;
  let best = null, d = Infinity;
  for (let j = 0; j < r.ny; j++) for (let i = 0; i < r.nx; i++) {
    if (r.bits[j * r.nx + i] !== '1') continue;
    const e = (i - cx) ** 2 + (j - cy) ** 2;
    if (e < d) { d = e; best = [i, j]; }
  }
  return [Math.min(m.cols, Math.max(0, best[0] * step)), Math.min(m.rows, Math.max(0, best[1] * step))];
}

/** Half the width of a drawn line, mm: thin on a small tile, a little wider on a big one, so about the same on the screen. */
export function lineHalf(map, sq) {
  return Math.min(1, Math.max(0.3, 0.012 * Math.max(map.cols, map.rows) * sq));
}

/**
 * The scene items for the places a region is made from (its `refs`): lines and edges as ribbons, a region
 * as a faint fill inside an outline (its points are markers, drawn with their labels).  Each {positions, color, role: 'line'|'fill', dz}.
 */
export function refItems(map, region, sq, color) {
  const items = [];
  // thin on a small tile, a little wider on a big one: about the same on the screen
  const half = lineHalf(map, sq);
  for (const ref of region.refs || []) {
    // a point is a marker with its label (Overlay.draw), not a disc
    if (ref.kind === 'point') continue;
    if (ref.kind === 'region') {
      const r = map.regions.find((x) => x.name === ref.name);
      if (!r) continue;
      items.push({ positions: regionTriangles(r, map, sq), color, role: 'fill', dz: 0.04 });
      items.push({ positions: outlineTriangles(r, map, sq, 0.7 * half), color, role: 'line', dz: 0.08 });
    } else if (ref.pts) {
      const loop = ref.kind === 'tile' || ref.name === 'edges';
      items.push({ positions: ribbonTriangles(ref.pts, sq, half), color, role: 'line', dz: 0.08, loop });
    }
  }
  return items.filter((i) => i.positions.length);
}

const css = (name) => token(name) || '#888'; // cached per theme (tokens.js)

export class Overlay {
  constructor(canvas, viewer) {
    this.canvas = canvas;
    this.viewer = viewer;
    this.ctx = canvas.getContext('2d');
    this.map = null;
    this.sq = 35;
    this.heights = null;
    this.scene = {};
  }

  setMap(map, squareMm) { this.map = map; if (squareMm) this.sq = squareMm; }

  /**
   * A height for every mm of the tile from the triangles `isSurface(entry)`
   * says are ground, dirt, floor or water: the highest corner in each cell,
   * the empty cells filled from their neighbours.
   */
  setHeights(payload, isSurface, footprint) {
    if (!payload || !footprint) { this.heights = null; return; }
    const [w, h] = footprint;
    const nx = Math.max(2, Math.ceil(w)), ny = Math.max(2, Math.ceil(h));
    const z = new Float32Array(nx * ny).fill(-Infinity);
    const P = payload.positions;
    for (let t = 0; t < payload.triangles; t++) {
      if (!isSurface(payload.entry[t])) continue;
      for (let v = 0; v < 3; v++) {
        const o = 9 * t + 3 * v;
        const i = Math.min(nx - 1, Math.max(0, Math.floor((P[o] / w) * nx)));
        const j = Math.min(ny - 1, Math.max(0, Math.floor((P[o + 1] / h) * ny)));
        if (P[o + 2] > z[j * nx + i]) z[j * nx + i] = P[o + 2];
      }
    }
    // fill the holes a few cells at a time from their neighbours
    for (let pass = 0, holes = true; holes && pass < 64; pass++) {
      holes = false;
      const next = z.slice();
      for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) {
        if (z[j * nx + i] > -Infinity) continue;
        let s = 0, n = 0;
        for (const [di, dj] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
          const a = i + di, b = j + dj;
          if (a >= 0 && b >= 0 && a < nx && b < ny && z[b * nx + a] > -Infinity) { s += z[b * nx + a]; n++; }
        }
        if (n) next[j * nx + i] = s / n; else holes = true;
      }
      z.set(next);
    }
    this.heights = z[0] > -Infinity ? { z, nx, ny, w, h } : null;
  }

  /** The ground's height at (x, y) mm. */
  heightAt(x, y) {
    const H = this.heights;
    if (!H) return (this.viewer.plan?.datumZ ?? 0) + 3.4;
    const i = Math.min(H.nx - 1, Math.max(0, Math.floor((x / H.w) * H.nx)));
    const j = Math.min(H.ny - 1, Math.max(0, Math.floor((y / H.h) * H.ny)));
    return H.z[j * H.nx + i];
  }

  /** Where point `q` in squares falls on the canvas, on the ground (lifted `up` mm). */
  at(q, up = 0.25) {
    const x = q[0] * this.sq, y = q[1] * this.sq;
    return this.viewer.project([x, y, this.heightAt(x, y) + up]);
  }

  /** Where point `q` in squares falls on the canvas at level zero, where regions lie. */
  atZero(q) {
    return this.viewer.project([q[0] * this.sq, q[1] * this.sq, this.viewer.plan?.datumZ ?? 0]);
  }

  /**
   * The regions are meshes in the 3D scene at level zero, not drawings on
   * the canvas: the selected one opaque in the accent (the viewer fades
   * the tile round it), or all of them faint while a tool is held.
   */
  drawRegions(scene) {
    const m = this.map, v = this.viewer;
    const z = v.plan?.datumZ ?? 0;
    const want = [];
    if (m && scene.show !== false) {
      for (const r of m.regions) {
        const sel = scene.selRegion === r.name;
        if (!sel && !scene.all) continue;
        want.push({ r, sel });
      }
    }
    const line = m && scene.show !== false ? m.lines.find((l) => l.name === scene.selLine && l.pts.length > 1) : null;
    const focus = want.some((w) => w.sel) || !!line;
    const items = (focus ? want.filter((w) => w.sel) : want).map(({ r, sel }) => ({
      positions: regionTriangles(r, m, this.sq),
      color: sel ? css('--accent') : scene.tints?.[r.name] || css('--plate-ink'),
      alpha: 0.16,
    }));
    // the selected line is a ribbon at zero, as a region is: its wobble's band under it, then the line
    if (line) {
      const half = lineHalf(m, this.sq);
      if (line.wobble) items.push({ positions: ribbonTriangles(line.pts, this.sq, line.wobble * (this.sq / 35), false, false), color: css('--accent'), role: 'fill', dz: 0.02, alpha: 0.18 });
      items.push({ positions: ribbonTriangles(line.pts, this.sq, 1.5 * half), color: css('--accent'), alpha: 1, dz: 0.04 });
    }
    // the hovered place, as the selected one is drawn but quieter: it changes nothing else
    if (m && scene.show !== false) {
      const hr = m.regions.find((r) => r.name === scene.hovRegion);
      if (hr) items.push({ positions: regionTriangles(hr, m, this.sq), color: css('--accent'), role: 'fill', alpha: 0.4 });
      const hl = m.lines.find((l) => l.name === scene.hovLine && l.pts.length > 1);
      if (hl) items.push({ positions: ribbonTriangles(hl.pts, this.sq, 1.5 * lineHalf(m, this.sq)), color: css('--accent'), role: 'fill', alpha: 0.7, dz: 0.04 });
      const hp = m.points?.find((p) => p.name === scene.hovPoint);
      if (hp) items.push({ positions: discTriangles(hp.at, this.sq, 3 * lineHalf(m, this.sq)), color: css('--accent'), role: 'fill', alpha: 0.9, dz: 0.04 });
    }
    // what the selected region is made from, drawn beside it
    if (focus) for (const w of want) if (w.sel) items.push(...refItems(m, w.r, this.sq, css('--ref')).map((i) => ({ ...i, alpha: i.role === 'fill' ? 0.22 : 1 })));
    const key = JSON.stringify([items.map((i) => [i.color, i.role, i.dz, i.positions.length, i.positions.reduce((a, x, n) => a + x * ((n % 97) + 1), 0)]), focus, z, m && [m.cols, m.rows, m.cells]]);
    if (key === this.regionKey) return;
    this.regionKey = key;
    v.setRegionLayer(items.length ? { focus, z, items } : null);
  }

  /** How strongly to draw: 1 looking straight down (the map), fainter as the view tilts. */
  strength() {
    const d = this.viewer.pose().dir;
    const down = d.z / (d.length() || 1); // dir runs from the target to the eye
    return down > 0.94 ? 1 : 0.45;
  }

  resize() {
    const dpr = devicePixelRatio || 1;
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (this.canvas.width !== Math.round(w * dpr) || this.canvas.height !== Math.round(h * dpr)) {
      this.canvas.width = Math.round(w * dpr);
      this.canvas.height = Math.round(h * dpr);
    }
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  /**
   * Draw `scene`: {show, all (every line and region: a tool is held), tints: {region: colour}, selRegion, selLine,
   * handles: [{q, hot, kind}], ring: {q, r}, path: [q], box: [q, q],
   * label: {q, text}}.
   */
  draw(scene = this.scene) {
    this.scene = scene;
    this.resize();
    const c = this.ctx, m = this.map;
    c.clearRect(0, 0, this.canvas.clientWidth, this.canvas.clientHeight);
    if (!this.viewer.payload && !this.viewer.plan) return;
    const k = this.strength();
    const ink = css('--plate-ink'), accent = css('--accent');
    // The map shows only what is being worked on: the selected line or
    // region, or all of them while a tool that draws or places is held.
    // Nothing selected, nothing drawn.
    this.drawRegions(scene);
    if (m && scene.show !== false) {
      const all = !!scene.all;
      c.save();
      // never past the tile's edge
      const edge = [[0, 0], [m.cols, 0], [m.cols, m.rows], [0, m.rows]].map((q) => this.at(q, 0.1));
      c.beginPath(); edge.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y))); c.closePath();
      c.clip();
      c.globalAlpha = 1;
      // lines drawn faint while a tool is held; the selected one is a ribbon in the scene (drawRegions)
      for (const l of m.lines) {
        if (!all || scene.selLine === l.name) continue;
        const pts = l.pts.map((q) => this.at(q));
        if (pts.length < 2) continue;
        c.globalAlpha = 0.7;
        c.lineWidth = 1.5;
        c.strokeStyle = ink;
        c.setLineDash([6, 4]);
        c.beginPath(); pts.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
        c.stroke();
        c.setLineDash([]);
      }
      c.restore();
      c.globalAlpha = 1;
      this.drawLabels(scene);
    }
    // the tool's own drawing
    if (scene.box) {
      const [a, b] = scene.box;
      const q = [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]].map((p) => this.at(p));
      c.beginPath(); q.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y))); c.closePath();
      c.fillStyle = accent; c.globalAlpha = 0.18; c.fill();
      c.globalAlpha = 1; c.strokeStyle = accent; c.lineWidth = 1.5; c.setLineDash([5, 4]); c.stroke(); c.setLineDash([]);
    }
    if (scene.path?.length) {
      c.strokeStyle = accent; c.lineWidth = 2.5; c.globalAlpha = 1;
      c.beginPath();
      scene.path.map((q) => this.at(q)).forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
      c.stroke();
    }
    if (scene.ring) {
      const { q, r } = scene.ring;
      c.strokeStyle = accent; c.fillStyle = accent; c.lineWidth = 2;
      c.beginPath();
      for (let n = 0; n <= 32; n++) {
        const t = (n / 32) * Math.PI * 2;
        const p = this.at([q[0] + (r * Math.cos(t)) / this.sq, q[1] + (r * Math.sin(t)) / this.sq]);
        if (n) c.lineTo(p.x, p.y); else c.moveTo(p.x, p.y);
      }
      c.globalAlpha = 0.2; c.fill(); c.globalAlpha = 1; c.stroke();
    }
    for (const h of scene.handles || []) {
      const p = this.at(h.q);
      c.beginPath(); c.arc(p.x, p.y, h.hot ? 7 : 5.5, 0, 7);
      c.fillStyle = h.hot ? accent : css('--raised');
      c.fill();
      c.lineWidth = 2; c.strokeStyle = accent; c.stroke();
    }
    if (scene.label) {
      const p = this.at(scene.label.q);
      c.font = '500 12px "JetBrains Mono", monospace';
      c.textAlign = 'left'; c.textBaseline = 'bottom';
      this.tag(p.x + 12, p.y - 10, scene.label.text, true, 'left');
    }
  }

  /**
   * The names and control points of the selected line or region.  Every control point it refers to is a dot with its label beside it
   * (the source as written: coordinates, or the point's name), then the names: the line's or region's own, and what a region is made from.
   * Labels keep clear of each other and of the dots.
   */
  drawLabels(scene) {
    const m = this.map, c = this.ctx;
    const sel = m.regions.find((r) => r.name === scene.selRegion) || null;
    const line = m.lines.find((l) => l.name === scene.selLine) || null;
    const markers = ((sel || line)?.markers || []).filter((k) => k.at);
    const taken = []; // rects {x0, y0, x1, y1} already used on the canvas
    const free = (r) => !taken.some((t) => r.x0 < t.x1 && r.x1 > t.x0 && r.y0 < t.y1 && r.y1 > t.y0);
    const width = (text) => c.measureText(text).width + 10;
    // the dots first, so labels keep off them
    const dots = markers.map((k) => ({ k, p: this.atZero(k.at) }));
    for (const { p } of dots) taken.push({ x0: p.x - 7, y0: p.y - 7, x1: p.x + 7, y1: p.y + 7 });
    c.font = '500 12px "JetBrains Mono", monospace';
    c.textBaseline = 'bottom';
    for (const { k, p } of dots) {
      const w = width(k.label);
      // beside the dot: right, then left, then stepping up and down
      const spots = [];
      for (const dy of [0, -20, 20, -40, 40]) {
        spots.push({ x: p.x + 9, dy }, { x: p.x - 9 - w, dy });
      }
      const at = spots.find((o) => free({ x0: o.x, y0: p.y - 9 + o.dy, x1: o.x + w, y1: p.y + 9 + o.dy })) || spots[0];
      taken.push({ x0: at.x, y0: p.y - 9 + at.dy, x1: at.x + w, y1: p.y + 9 + at.dy });
      this.tag(at.x, p.y + 8 + at.dy, k.label, 'point', 'left');
    }
    for (const { k, p } of dots) {
      // a point drawn from a range lies somewhere in it: a ring, not a dot
      c.beginPath(); c.arc(p.x, p.y, 5, 0, 7);
      c.lineWidth = 2.5; c.strokeStyle = css('--on-point'); c.stroke();
      c.lineWidth = 1.5; c.strokeStyle = css('--point'); c.stroke();
      if (!k.ranged) { c.fillStyle = css('--point'); c.fill(); }
    }
    // the names, centred above their places
    c.font = '600 12px "IBM Plex Sans", sans-serif';
    const name = (q, text, tone) => {
      if (!q) return;
      const p = this.atZero(q), w = width(text);
      let y = p.y - 8;
      while (!free({ x0: p.x - w / 2, y0: y - 17, x1: p.x + w / 2, y1: y + 1 }) && y > p.y - 120) y -= 20;
      taken.push({ x0: p.x - w / 2, y0: y - 17, x1: p.x + w / 2, y1: y + 1 });
      this.tag(p.x, y, text, tone);
    };
    if (line) name(refAnchor({ kind: 'line', pts: line.pts }, m), line.name, true);
    if (sel) {
      name(regionAnchor(sel, m), sel.name, true);
      for (const ref of sel.refs || []) if (ref.kind !== 'point') name(refAnchor(ref, m), ref.name, 'ref');
    }
  }

  /** A small label: `on` true in the accent, 'point' in the colour of control points, 'ref' in the colour of the places a region is made from, else on a pale ground. */
  tag(x, y, text, on, align = 'center') {
    const c = this.ctx;
    const w = c.measureText(text).width + 10;
    const left = align === 'center' ? x - w / 2 : x;
    c.globalAlpha = 0.92;
    c.fillStyle = on === 'point' ? css('--point') : on === 'ref' ? css('--ref') : on ? css('--accent') : 'rgb(255 255 255)';
    c.fillRect(left, y - 17, w, 18);
    c.globalAlpha = 1;
    c.fillStyle = on === 'point' ? css('--on-point') : on === 'ref' ? css('--on-ref') : on ? css('--on-accent') : '#1A1714';
    c.textAlign = 'left';
    c.fillText(text, left + 5, y - 3);
    c.textAlign = align;
  }

  /**
   * The handle (index into `scene.handles`) within `px` pixels of canvas
   * point (x, y), nearest first; -1 for none.
   */
  handleAt(x, y, px = 10) {
    let best = -1, d = px;
    (this.scene.handles || []).forEach((h, i) => {
      const p = this.at(h.q);
      const e = Math.hypot(p.x - x, p.y - y);
      if (e <= d) { d = e; best = i; }
    });
    return best;
  }

  /** The region (by name) holding point `q` in squares, the last one listed winning; null outside them all. */
  regionAt(q) {
    const m = this.map;
    if (!m) return null;
    const i = Math.round(q[0] * m.cells), j = Math.round(q[1] * m.cells);
    let hit = null;
    for (const r of m.regions) if (i >= 0 && j >= 0 && i < r.nx && j < r.ny && r.bits[j * r.nx + i] === '1') hit = r.name;
    return hit;
  }

  /** The line within `px` pixels of canvas point (x, y); null for none. */
  lineAt(x, y, px = 8) {
    let best = null, d = px;
    for (const l of this.map?.lines || []) {
      const pts = l.pts.map((q) => this.at(q));
      for (let n = 1; n < pts.length; n++) {
        const a = pts[n - 1], b = pts[n];
        const vx = b.x - a.x, vy = b.y - a.y, len = vx * vx + vy * vy || 1;
        const t = Math.max(0, Math.min(1, ((x - a.x) * vx + (y - a.y) * vy) / len));
        const e = Math.hypot(a.x + t * vx - x, a.y + t * vy - y);
        if (e < d) { d = e; best = l.name; }
      }
    }
    return best;
  }
}
