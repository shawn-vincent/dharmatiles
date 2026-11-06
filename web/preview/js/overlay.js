// The overlay: the tile's map drawn over the view (docs/design/editor.html,
// "The stack and the map").  Looking straight down it is the map: regions
// tinted by what fills them, their names, the lines with their wobble, the
// fill points.  In every other view the same drawing lies on the tile,
// fainter.  The tools draw here too: a footprint under the pointer, the
// line being drawn, the box being dragged, and the handles of the
// selection.  Points are in squares; heights come from the surfaces the
// build made, so the drawing lies on the ground, not on the datum.

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#888';

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
    if (!H) return (this.viewer.plan?.datum_z ?? 0) + 3.4;
    const i = Math.min(H.nx - 1, Math.max(0, Math.floor((x / H.w) * H.nx)));
    const j = Math.min(H.ny - 1, Math.max(0, Math.floor((y / H.h) * H.ny)));
    return H.z[j * H.nx + i];
  }

  /** Where point `q` in squares falls on the canvas, on the ground (lifted `up` mm). */
  at(q, up = 0.25) {
    const x = q[0] * this.sq, y = q[1] * this.sq;
    return this.viewer.project([x, y, this.heightAt(x, y) + up]);
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
   * Draw `scene`: {show, tints: {region: colour}, selRegion, selLine,
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
    if (m && scene.show !== false) {
      // regions, as runs of lattice nodes, one quad a run
      for (const r of m.regions) {
        const sel = scene.selRegion === r.name;
        const tint = scene.tints?.[r.name];
        if (!tint && !sel && k < 1) continue;
        c.fillStyle = sel ? accent : tint || ink;
        c.globalAlpha = sel ? 0.34 : (tint ? 0.26 : 0.1) * k;
        const step = 1 / m.cells;
        for (let j = 0; j < r.ny; j++) {
          let i = 0;
          while (i < r.nx) {
            if (r.bits[j * r.nx + i] !== '1') { i++; continue; }
            const a = i;
            while (i < r.nx && r.bits[j * r.nx + i] === '1') i++;
            const x0 = Math.max(0, (a - 0.5) * step), x1 = Math.min(m.cols, (i - 0.5) * step);
            const y0 = Math.max(0, (j - 0.5) * step), y1 = Math.min(m.rows, (j + 0.5) * step);
            const corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map((q) => this.at(q, 0.1));
            c.beginPath();
            corners.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
            c.closePath();
            c.fill();
          }
        }
      }
      c.globalAlpha = 1;
      // lines: the wobble's band, then the line
      for (const l of m.lines) {
        const sel = scene.selLine === l.name;
        const pts = l.pts.map((q) => this.at(q));
        if (pts.length < 2) continue;
        const path = () => { c.beginPath(); pts.forEach((p, n) => (n ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y))); };
        if (l.wobble) {
          const mid = l.pts[Math.floor(l.pts.length / 2)];
          const a = this.at(mid), b = this.at([mid[0] + (2 * l.wobble) / this.sq, mid[1]]);
          c.lineWidth = Math.max(2, Math.hypot(b.x - a.x, b.y - a.y));
          c.strokeStyle = sel ? accent : ink;
          c.globalAlpha = 0.1 * k + (sel ? 0.1 : 0);
          c.lineJoin = 'round'; c.lineCap = 'round';
          path(); c.stroke();
        }
        c.globalAlpha = sel ? 1 : 0.55 + 0.35 * k;
        c.lineWidth = sel ? 2.5 : 1.5;
        c.strokeStyle = sel ? accent : ink;
        c.setLineDash(sel ? [] : [6, 4]);
        path(); c.stroke();
        c.setLineDash([]);
      }
      c.globalAlpha = 1;
      // region names, at their first fill point, on the map
      if (k >= 1 || scene.selRegion) {
        c.font = '600 12px "IBM Plex Sans", sans-serif';
        c.textAlign = 'center'; c.textBaseline = 'bottom';
        for (const r of m.regions) {
          if (k < 1 && scene.selRegion !== r.name) continue;
          for (const f of r.fills) {
            if (!f.at) continue;
            const p = this.at(f.at);
            this.tag(p.x, p.y - 8, r.name, scene.selRegion === r.name);
            c.fillStyle = scene.selRegion === r.name ? accent : ink;
            c.beginPath(); c.arc(p.x, p.y, 3, 0, 7); c.fill();
          }
        }
      }
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

  /** A small label on a pale ground. */
  tag(x, y, text, on, align = 'center') {
    const c = this.ctx;
    const w = c.measureText(text).width + 10;
    const left = align === 'center' ? x - w / 2 : x;
    c.globalAlpha = 0.92;
    c.fillStyle = on ? css('--accent') : 'rgb(255 255 255)';
    c.fillRect(left, y - 17, w, 18);
    c.globalAlpha = 1;
    c.fillStyle = on ? css('--on-accent') : '#1A1714';
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
