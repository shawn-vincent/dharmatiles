// The viewer's layers over the mesh: the debug dots, the clouds, the region map, the
// grid and the build's sketches.
//
// Methods of `Viewer`, installed on its prototype by `viewer.js`: they run
// with `this` the viewer.

import * as THREE from 'three';
import { classInfo, segments } from '../debug-layer.js';
import { cloudOf, columnGeometry, shadeAlpha, SHADE_COLUMN, SHADE_SHEET, sheetGeometry } from '../cloud.js';
import { FAT_FRAG, FAT_VERT, fatGeometry } from '../fatline.js';

const BED_MM = 4; // how thick the printer's bed is drawn
import { CLOUD_CONTOUR_PX, CLOUD_HOVER, CLOUD_SPAN_PX, CONTOUR_ALPHA, DOT_FRAG, DOT_VERT, GRID_BELOW_MM, GRID_FRAG } from './consts.js';

export const layersMethods = {
  /** The debug layer's objects, made again from `this.debug` while it is shown. */
  buildDebug() {
    if (this.debugGroup) {
      this.scene.remove(this.debugGroup);
      this.debugGroup.traverse((o) => { o.geometry?.dispose(); o.material?.dispose(); });
      this.debugGroup = null;
    }
    const d = this.debug;
    this.onDebugChange?.(this.showDebug ? d : null);
    if (!this.showDebug || !d) return;
    const group = new THREE.Group();
    const colour = (token) => new THREE.Color().setStyle(this.debugColour(token)); // a bad token is a bug: let it throw
    const over = (o, order) => { o.frustumCulled = false; o.renderOrder = order; group.add(o); };
    // then the marks, over everything: they lie on the ground under the blades
    const marks = { depthTest: false, depthWrite: false, transparent: true };
    for (const [cls, trails] of Object.entries(d.trails ?? {})) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(segments(trails), 3));
      over(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: colour(classInfo(cls).token), opacity: cls === 'flow' ? 0.6 : 1, ...marks })), cls === 'flow' ? 20 : 21);
    }
    const pos = [], col = [];
    for (const [cls, xyz] of Object.entries(d.dots ?? {})) {
      const c = colour(classInfo(cls).token);
      for (let k = 0; k < xyz.length; k += 3) { pos.push(xyz[k], xyz[k + 1], xyz[k + 2]); col.push(c.r, c.g, c.b); }
    }
    if (pos.length) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(pos), 3));
      g.setAttribute('color', new THREE.BufferAttribute(new Float32Array(col), 3));
      const size = 5 * Math.min(2, this.renderer.getPixelRatio());
      over(new THREE.Points(g, new THREE.ShaderMaterial({ uniforms: { uSize: { value: size } }, vertexShader: DOT_VERT, fragmentShader: DOT_FRAG, ...marks })), 22);
    }
    this.debugGroup = group;
    this.scene.add(group);
  },

  /**
   * The selected cloud: `sel` and `hov` are stack entries (or null), the
   * one whose `top=` or pin the caret is in or the sheet's top field has
   * focus, and the one under the pointer.  Drawn from the payload's clouds
   * in the accent, as a selected place is: its low and high sheets shaded by
   * how often each height comes up, and a line from low to high at each
   * branch tip or stone with the rolled height marked.  The hovered one is
   * quieter.
   */
  setCloud(sel, hov) {
    if (this.cloudSel === sel && this.cloudHov === hov) return;
    this.cloudSel = sel;
    this.cloudHov = hov;
    this.buildCloud();
    this.draw();
  },

  /** Segments drawn `px` CSS pixels wide (see fatline.js): `pos` six numbers a segment, `rgba` four a vertex (two a segment). */
  fatLines(pos, rgba, px) {
    const f = fatGeometry(pos, rgba);
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(f.position, 3));
    g.setAttribute('other', new THREE.BufferAttribute(f.other, 3));
    g.setAttribute('side', new THREE.BufferAttribute(f.side, 1));
    g.setAttribute('color', new THREE.BufferAttribute(f.color, 4));
    g.setIndex(new THREE.BufferAttribute(f.index, 1));
    const m = new THREE.ShaderMaterial({
      uniforms: { uRes: this.fatRes, uWidth: { value: px * this.fatDpr } },
      vertexShader: FAT_VERT, fragmentShader: FAT_FRAG,
      transparent: true, depthTest: false, depthWrite: false, side: THREE.DoubleSide,
    });
    m.userData.px = px;
    this.fatMaterials.add(m);
    return new THREE.Mesh(g, m);
  },

  buildCloud() {
    if (this.cloudGroup) {
      this.scene.remove(this.cloudGroup);
      this.cloudGroup.traverse((o) => {
        o.geometry?.dispose();
        o.material?.dispose();
        this.fatMaterials.delete(o.material);
      });
      this.cloudGroup = null;
    }
    const d = this.clouds;
    const wanted = [[this.cloudSel, 1], [this.cloudHov !== this.cloudSel ? this.cloudHov : null, CLOUD_HOVER]].filter(([e]) => e != null);
    if (!d || !wanted.length) return;
    const group = new THREE.Group();
    // the cloud's own colour, not the selection's accent: a rock's cloud sits on the rock's outline
    const c = new THREE.Color().setStyle(this.debugColour('--cloud')); // a bad token is a bug: let it throw
    const plate = new THREE.Color(this.debugColour('--plate'));
    const over = (o, order) => { o.frustumCulled = false; o.renderOrder = order; group.add(o); };
    // drawn over the solids, as a selected place is: the low sheet is
    // under the ground it bounds
    const flat = { depthTest: false, depthWrite: false, transparent: true };
    const mpos = [], mcol = [];
    for (const [entry, strength] of wanted) {
      const { sheets, columns } = cloudOf(d, entry);
      for (const s of sheets) {
        const { positions, contour, shade } = sheetGeometry(s);
        const fill = new THREE.BufferGeometry();
        fill.setAttribute('position', new THREE.BufferAttribute(positions, 3));
        const rgba = new Float32Array(shade.length * 4);
        shade.forEach((v, n) => rgba.set([c.r, c.g, c.b, strength * shadeAlpha(v, SHADE_SHEET)], 4 * n));
        fill.setAttribute('color', new THREE.BufferAttribute(rgba, 4));
        over(new THREE.Mesh(fill, new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide, ...flat })), 30);
        const rgbaEdge = new Float32Array((contour.length / 3) * 4);
        for (let n = 0; n < rgbaEdge.length; n += 4) rgbaEdge.set([c.r, c.g, c.b, strength * CONTOUR_ALPHA], n);
        over(this.fatLines(contour, rgbaEdge, CLOUD_CONTOUR_PX), 31);
      }
      for (const col of columns) {
        const g = columnGeometry(col.v);
        const rgba = new Float32Array(g.lineShade.length * 4);
        g.lineShade.forEach((v, n) => rgba.set([c.r, c.g, c.b, strength * shadeAlpha(v, SHADE_COLUMN)], 4 * n));
        over(this.fatLines(g.lines, rgba, CLOUD_SPAN_PX), 32);
        for (let n = 0; n < g.n; n++) {
          mpos.push(g.marks[3 * n], g.marks[3 * n + 1], g.marks[3 * n + 2]);
          // the marker toward the plate where its height is rare, and when only hovered
          const t = strength * shadeAlpha(g.markShade[n], SHADE_COLUMN);
          const m = c.clone().lerp(plate, 1 - t);
          mcol.push(m.r, m.g, m.b);
        }
      }
    }
    if (mpos.length) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(mpos), 3));
      g.setAttribute('color', new THREE.BufferAttribute(new Float32Array(mcol), 3));
      const size = 6 * Math.min(2, this.renderer.getPixelRatio());
      over(new THREE.Points(g, new THREE.ShaderMaterial({ uniforms: { uSize: { value: size } }, vertexShader: DOT_VERT, fragmentShader: DOT_FRAG, ...flat })), 33);
    }
    this.cloudGroup = group;
    this.scene.add(group);
  },

  /**
   * The map's regions in the 3D scene, flat at level zero (the datum).
   * `layer`: {focus (one region is selected: opaque, the rest of the tile
   * fades), z (mm), items: [{positions (Float32Array of triangles), color, alpha}]}; null for none.
   * Seen through the faded tile, depth-tested against it; with the fade
   * off (or a stale mesh) it can't be seen inside the solid, so it is
   * drawn over the tile instead.
   */
  setRegionLayer(layer) {
    if (this.regionLayer?.group) {
      this.scene.remove(this.regionLayer.group);
      for (const m of this.regionLayer.group.children) { m.geometry.dispose(); m.material.dispose(); }
    }
    this.regionLayer = null;
    if (layer?.items?.length) {
      const group = new THREE.Group();
      for (const it of layer.items) {
        const g = new THREE.BufferGeometry();
        g.setAttribute('position', new THREE.BufferAttribute(it.positions, 3));
        const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: it.color, side: THREE.DoubleSide, transparent: true }));
        m.frustumCulled = false;
        m.userData.alpha = it.alpha;
        m.userData.ref = !!it.role; // a place the selected region is made from, not the region
        m.userData.line = it.role === 'line';
        m.position.z = it.dz ?? 0;
        group.add(m);
      }
      group.position.z = layer.z;
      this.scene.add(group);
      this.regionLayer = { focus: !!layer.focus, group };
    }
    this.applyFade();
    this.draw();
  },

  /** Opaque and in depth order before the fade for the selected region; faint and over everything otherwise. */
  styleRegions() {
    const L = this.regionLayer;
    if (!L) return;
    const seen = L.focus && this.fade && !this.stale && this.sketches.size === 0;
    for (const m of L.group.children) {
      const mat = m.material;
      mat.depthTest = seen;
      mat.depthWrite = seen;
      // the region itself is opaque; what it is made from is quieter: a line solid, a region's fill faint
      const solid = !m.userData.ref || m.userData.line;
      mat.opacity = L.focus ? (solid ? (seen ? 1 : 0.7) : m.userData.alpha) : m.userData.alpha;
      mat.transparent = !(L.focus && seen && solid);
      m.renderOrder = seen ? 0 : 9;
      mat.needsUpdate = true;
    }
  },

  /**
   * The ground grid, ruled at the square size (35 mm DungeonBlocks, 25.4
   * OpenLOCK): theme-contrasting lines, bold at every square and fine in between, a
   * plane at the tile's datum (z = 0, where the base meets the tile body;
   * an STL has none, so its lowest point) that fades out toward the
   * horizon.  It is drawn after the tile, without writing depth, a hair
   * below the datum so it never fights the tile's flat underside.
   */
  buildGrid() {
    if (this.grid) { this.scene.remove(this.grid); this.grid.geometry.dispose(); this.grid.material.dispose(); this.grid = null; }
    const b = this.bounds();
    // a map's grid is the map's: its squares over its extent, at the map's ground
    const sg = this.sceneOn ? this.sceneGridSpec : null;
    const sq = sg ? sg.sq : this.plan?.squareMm;
    if (!b || !sq) return;
    const [w, h] = sg ? [sg.w, sg.h] : this.plan.sizeMm;
    const z = (sg ? sg.z : (this.plan.datumZ ?? b.lo[2])) - GRID_BELOW_MM;
    const reach = Math.max(w, h) * 6;
    const mat = new THREE.ShaderMaterial({
      uniforms: {
        uSq: { value: sq }, uDim: { value: this.gridDim ?? 1 }, uCentre: { value: new THREE.Vector2(sg ? sg.cx : w / 2, sg ? sg.cy : h / 2) },
        uR0: { value: Math.max(w, h) * 1.2 }, uR1: { value: reach * 0.5 },
        uInk: { value: this.gridInk?.clone() ?? new THREE.Color(0.17, 0.2, 0.25) },
      },
      vertexShader: 'varying vec2 vXY; void main() { vec4 w = modelMatrix * vec4(position, 1.0); vXY = w.xy; gl_Position = projectionMatrix * viewMatrix * w; }',
      fragmentShader: GRID_FRAG,
      transparent: true, depthWrite: false, side: THREE.DoubleSide,
    });
    this.grid = new THREE.Mesh(new THREE.PlaneGeometry(reach, reach), mat);
    this.grid.position.set(sg ? sg.cx : w / 2, sg ? sg.cy : h / 2, z);
    this.grid.renderOrder = 1;
    this.grid.frustumCulled = false;
    this.grid.visible = this.showGrid && !this.backdrop?.on();
    this.scene.add(this.grid);
    this.buildBed();
  },

  /**
   * A printer's bed, when the mesh shown is a plate (`meta.bed`, mm): a translucent slab the pieces stand on
   * (BED_MM thick, its top at the pieces' zero, the top face lighter than the sides so it reads as a solid
   * without lights), its top edge drawn as a line, and each excluded zone (`meta.excluded`, [x0, y0, x1, y1]
   * from the front-left corner) as a faint filled box with its edge.  Ink is the grid's, so it follows the theme.
   */
  buildBed() {
    if (this.bedGroup) {
      this.scene.remove(this.bedGroup);
      this.bedGroup.traverse((o) => { o.geometry?.dispose(); if (o.material) { this.fatMaterials.delete(o.material); o.material.dispose(); } });
      this.bedGroup = null;
    }
    const bed = this.payload?.meta?.bed;
    if (!bed) return;
    const group = new THREE.Group();
    const ink = this.gridInk?.clone() ?? new THREE.Color(0.17, 0.2, 0.25);
    const z = (this.plan?.datumZ ?? 0) - GRID_BELOW_MM * 0.5;
    const rect = (x0, y0, x1, y1, fill, px, alpha) => {
      // the edge, drawn a few pixels wide whatever the zoom
      const c = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
      const pos = new Float32Array(24), rgba = new Float32Array(32);
      for (let k = 0; k < 4; k++) {
        const [a, b] = [c[k], c[(k + 1) % 4]];
        pos.set([a[0], a[1], z, b[0], b[1], z], 6 * k);
        for (const v of [0, 1]) rgba.set([ink.r, ink.g, ink.b, alpha], 8 * k + 4 * v);
      }
      const edge = this.fatLines(pos, rgba, px);
      edge.renderOrder = 2; edge.frustumCulled = false;
      group.add(edge);
      if (!fill) return;
      const m = new THREE.Mesh(new THREE.PlaneGeometry(x1 - x0, y1 - y0), new THREE.MeshBasicMaterial({ color: ink, transparent: true, opacity: fill, depthWrite: false, side: THREE.DoubleSide }));
      m.position.set((x0 + x1) / 2, (y0 + y1) / 2, z); m.renderOrder = 0; m.frustumCulled = false;
      group.add(m);
    };
    // the slab: under the grid's own plane, so the ruling still reads on it
    const top = z - GRID_BELOW_MM;
    const slab = new THREE.BoxGeometry(bed[0], bed[1], BED_MM);
    const shade = [0.62, 0.62, 0.5, 0.74, 1, 0.35]; // +x, -x, +y, -y (the front), top, underside
    const tint = new THREE.Color(0x9fb3cc);
    const col = new Float32Array(24 * 3);
    for (let f = 0; f < 6; f++) for (let v = 0; v < 4; v++) col.set([tint.r * shade[f], tint.g * shade[f], tint.b * shade[f]], (f * 4 + v) * 3);
    slab.setAttribute('color', new THREE.BufferAttribute(col, 3));
    const sm = new THREE.Mesh(slab, new THREE.MeshBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.4, depthWrite: false }));
    sm.position.set(bed[0] / 2, bed[1] / 2, top - BED_MM / 2);
    sm.renderOrder = 0; sm.frustumCulled = false;
    group.add(sm);
    rect(0, 0, bed[0], bed[1], 0, 2.5, 0.85);
    for (const [x0, y0, x1, y1] of this.payload.meta.excluded ?? []) rect(x0, y0, x1, y1, 0.3, 1.5, 0.7);
    this.bedGroup = group;
    this.scene.add(group);
  },

  /** Set the grid's opacity independently of the tile's paint. */
  setGridDim(v) {
    this.gridDim = v;
    if (this.grid) this.grid.material.uniforms.uDim.value = v;
    this.draw();
  },

  /** The theme's grid ink: dark by day, pale against Moonpool's twilight. */
  setGridColor(css) {
    this.gridInk ??= new THREE.Color(0.17, 0.2, 0.25);
    this.gridInk.setStyle(css); // a bad theme token is a bug: let it throw to the page's error panel
    if (this.grid) this.grid.material.uniforms.uInk.value.copy(this.gridInk);
    if (this.bedGroup) this.buildBed();
    this.draw();
  },

  /** The sketches of a build running: [{id, rough, payload}], replacing those shown. */
  setSketches(list) {
    const want = new Set(list.map((k) => k.id));
    for (const [id, k] of this.sketches) {
      if (want.has(id)) continue;
      this.scene.remove(...k.meshes);
      k.geometry.dispose();
      this.sketches.delete(id);
    }
    for (const { id, rough, payload } of list) {
      if (this.sketches.has(id)) continue;
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(payload.positions, 3));
      g.setAttribute('normal', new THREE.BufferAttribute(payload.normals, 3));
      g.setAttribute('paint', new THREE.BufferAttribute(payload.clean, 3, true));
      g.setAttribute('who', new THREE.BufferAttribute(new Float32Array(2 * payload.triangles * 3), 2));
      const look = rough ? this.sketchLooks.rough : this.sketchLooks.parts;
      const meshes = [look.depth, look.colour].map((m, i) => {
        const gm = new THREE.Mesh(g, m);
        gm.frustumCulled = false; gm.renderOrder = (rough ? 5 : 3) + i;
        return gm;
      });
      this.scene.add(...meshes);
      this.sketches.set(id, { geometry: g, meshes, rough, bounds: payload.meta?.lo && { lo: payload.meta.lo, hi: payload.meta.hi } });
    }
    this.applyFade();
    this.pulse();
    this.refit();
    this.draw();
  },

  /** The bounds of each sketch shown: [{lo, hi}]. */
  sketchBounds() {
    return [...this.sketches.values()].map((k) => k.bounds).filter(Boolean);
  }
};
