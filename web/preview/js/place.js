// The bar and its tools: placing, scattering, painting, drawing lines and
// making regions on the tile (docs/design/editor.html, "Placing and
// drawing").  Each gesture writes one node into the text, through `api`:
//
//   api.elements()                  the element schema
//   api.defs()                      the document's own definitions
//   api.sq()                        mm per square; api.size() [cols, rows]
//   api.map()                       the map (lines, regions) of the text now
//   api.writable()                  may the file be changed? (else api.makeMine(replay))
//   api.insert(place, node)         write `node` into the stack or the tile's body
//   api.replace(start, len, text, end)  rewrite bytes of the text (a handle)
//   api.handles()                   the selection's handles: [{q, span, snap}]
//   api.redraw(scene)               the overlay, with the tool's drawing
//   api.note(text)                  a word in the view's hint line
//
// A tool stays in hand until Esc (placing places again at each click);
// the others put down what they made and select it.

import { pointLabel, placeText, pointText } from './points.js';

const enc = new TextEncoder();
const bytes = (s) => enc.encode(s).length;
const WORDS = [
  ['line', 'draw a line through points on the plate', 'language'],
  ['region', 'click inside the lines: an area to fill', 'language'],
];
const EDGE = 0.05; // squares from an edge that snap onto it
const MID = 0.08;  // squares from an edge's middle or a corner that snap to its word

/** Point `q` (squares) snapped to the tile's edges: {q, word|null}. */
export function snap(q, [cols, rows]) {
  let [x, y] = q;
  const near = (v, to, d) => Math.abs(v - to) <= d;
  // corners, then the middles of edges, as words
  const ns = near(y, 0, MID) ? 'south' : near(y, rows, MID) ? 'north' : null;
  const ew = near(x, 0, MID) ? 'west' : near(x, cols, MID) ? 'east' : null;
  if (ns && ew) return { q: [ew === 'west' ? 0 : cols, ns === 'south' ? 0 : rows], word: `${ns}-${ew}` };
  if (ns && near(x, cols / 2, MID)) return { q: [cols / 2, ns === 'south' ? 0 : rows], word: ns };
  if (ew && near(y, rows / 2, MID)) return { q: [ew === 'west' ? 0 : cols, rows / 2], word: ew };
  // onto an edge, as a pair
  if (near(x, 0, EDGE)) x = 0; else if (near(x, cols, EDGE)) x = cols;
  if (near(y, 0, EDGE)) y = 0; else if (near(y, rows, EDGE)) y = rows;
  return { q: [Math.min(cols, Math.max(0, x)), Math.min(rows, Math.max(0, y))], word: null };
}
/** A snapped point as written: its word, else a pair. */
const written = (s, rows) => s.word || pointText(s.q, rows);

/** The nearest tile edge's word to `q`. */
function edgeOf([x, y], [cols, rows]) {
  const d = [[x, 'west'], [cols - x, 'east'], [y, 'south'], [rows - y, 'north']];
  return d.sort((a, b) => a[0] - b[0])[0][1];
}

/** A name not yet used in `taken`: `base` (`line-1`), then `line-2`… */
function fresh(base, taken) {
  if (!taken.includes(base)) return base;
  const stem = base.replace(/-1$/, '');
  for (let n = 2; ; n++) if (!taken.includes(`${stem}-${n}`)) return `${stem}-${n}`;
}

export class Tools {
  constructor(canvas, viewer, overlay, ui, api) {
    this.canvas = canvas;
    this.viewer = viewer;
    this.overlay = overlay;
    this.ui = ui; // {bar, input, list, chip}
    this.api = api;
    this.tool = null;
    this.drag = null;
    this.list = { items: [], i: 0 };
    this.listen();
  }

  get active() { return !!this.tool; }
  get busy() { return !!this.tool || !!this.drag; }

  // ---- the bar --------------------------------------------------------
  /** What the bar can take, for the word typed. */
  choices(word) {
    const els = this.api.elements().map((e) => [e.name, e.about, e.role]);
    const defs = this.api.defs().map((d) => [d, 'a definition of this document', 'thing']);
    return [...els, ...defs, ...WORDS]
      .filter(([n]) => n.startsWith(word))
      .sort((a, b) => (a[0] === word ? -1 : b[0] === word ? 1 : a[0].length - b[0].length || a[0].localeCompare(b[0])))
      .slice(0, 12);
  }

  renderList() {
    const { list } = this.ui;
    const word = this.ui.input.value.trim().split(/\s+/)[0] || '';
    this.list.items = this.choices(word);
    if (this.list.i >= this.list.items.length) this.list.i = 0;
    list.replaceChildren();
    list.hidden = !this.list.items.length || document.activeElement !== this.ui.input;
    this.list.items.forEach(([name, about, role], i) => {
      const li = document.createElement('div');
      li.className = `it fam-${role}${i === this.list.i ? ' on' : ''}`;
      li.setAttribute('role', 'option');
      li.setAttribute('aria-selected', i === this.list.i);
      li.innerHTML = '<i class="sw"></i><b></b><span></span>';
      li.querySelector('b').textContent = name;
      li.querySelector('span').textContent = about || '';
      li.onpointerdown = (e) => { e.preventDefault(); this.ui.input.value = name; this.take(); };
      list.append(li);
    });
  }

  /** Take what the bar says as the tool in hand. */
  take() {
    const text = this.ui.input.value.trim();
    if (!text) return;
    const [word, ...rest] = text.split(/\s+/);
    const pick = this.list.items[this.list.i];
    const name = this.choices(word).some(([n]) => n === word) ? word : pick?.[0];
    if (!name) { this.api.note(`Nothing is called ${word}`); return; }
    if (!this.api.writable()) { this.api.makeMine({ run: () => this.take() }); return; }
    const el = this.api.elements().find((e) => e.name === name);
    const extra = rest.join(' ');
    let kind;
    if (name === 'line') kind = 'line';
    else if (name === 'region') kind = 'region';
    else if (name === 'scatter') kind = 'scatter';
    else if (!el) kind = 'place'; // a definition: a thing
    else if (el.role === 'run') kind = 'run';
    else if (el.takes.includes('point')) kind = 'place';
    else kind = 'paint';
    this.ui.input.value = '';
    this.ui.input.blur();
    this.ui.list.hidden = true;
    this.start({ kind, name, el, extra, points: [] });
  }

  start(tool) {
    this.tool = tool;
    this.canvas.classList.add('tool');
    const say = {
      place: `Click to place ${tool.name}`,
      scatter: 'Click or drag over a region to scatter in it',
      paint: `Click a region to lay ${tool.name} there`,
      run: `Click a line or near an edge to run ${tool.name} along it`,
      line: 'Click points; Enter or a double-click ends the line',
      region: 'Click inside the lines',
    }[tool.kind];
    this.ui.chip.hidden = false;
    this.ui.chip.querySelector('b').textContent = tool.name + (tool.extra ? ` ${tool.extra}` : '');
    this.ui.chip.querySelector('span').textContent = say;
    this.paint();
  }

  stop() {
    this.tool = null;
    this.canvas.classList.remove('tool');
    this.ui.chip.hidden = true;
    this.hover = null;
    this.paint();
  }

  // ---- the pointer ------------------------------------------------------
  /** Squares under canvas point (x, y), or null. */
  at(x, y, planeZ = null) {
    const hit = this.viewer.pointAt(x, y, planeZ);
    if (!hit) return null;
    const sq = this.api.sq();
    return { q: [hit.p[0] / sq, hit.p[1] / sq], z: hit.p[2] };
  }

  xy(e) { const r = this.canvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; }

  listen() {
    const c = this.canvas;
    const opt = { capture: true };
    c.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      const [x, y] = this.xy(e);
      // a handle under the pointer: drag it
      const h = this.overlay.handleAt(x, y);
      if (!this.tool && h >= 0 && this.api.writable()) {
        e.stopImmediatePropagation(); e.preventDefault();
        this.grab(h, e);
        return;
      }
      if (!this.tool) return;
      e.stopImmediatePropagation(); e.preventDefault();
      c.setPointerCapture(e.pointerId);
      const p = this.at(x, y);
      this.press = p && { q: p.q, x, y, moved: false };
    }, opt);
    c.addEventListener('pointermove', (e) => {
      if (this.drag) { this.dragTo(e); return; }
      if (!this.tool) {
        // a handle shows it can be taken
        const [x, y] = this.xy(e);
        const h = this.overlay.handleAt(x, y);
        c.classList.toggle('grab', h >= 0);
        if (h !== this.hot) { this.hot = h; this.paint(); }
        return;
      }
      e.stopImmediatePropagation();
      const [x, y] = this.xy(e);
      const p = this.at(x, y);
      if (this.press && Math.hypot(x - this.press.x, y - this.press.y) > 6) this.press.moved = true;
      this.hover = p && { ...p, x, y };
      this.paint();
    }, opt);
    c.addEventListener('pointerup', (e) => {
      if (this.drag) { this.letGo(); e.stopImmediatePropagation(); return; }
      if (!this.tool || e.button !== 0) return;
      e.stopImmediatePropagation();
      const press = this.press;
      this.press = null;
      const [x, y] = this.xy(e);
      const p = this.at(x, y);
      if (!press || !p) return;
      this.act(press, p, x, y);
    }, opt);
    c.addEventListener('dblclick', (e) => {
      if (this.tool?.kind !== 'line') return;
      e.stopImmediatePropagation();
      this.endLine();
    }, opt);

    const inp = this.ui.input;
    inp.addEventListener('input', () => { this.list.i = 0; this.renderList(); });
    inp.addEventListener('focus', () => this.renderList());
    inp.addEventListener('blur', () => { this.ui.list.hidden = true; });
    inp.addEventListener('keydown', (e) => {
      const n = this.list.items.length;
      if (e.key === 'ArrowDown' && n) { this.list.i = (this.list.i + 1) % n; this.renderList(); e.preventDefault(); }
      else if (e.key === 'ArrowUp' && n) { this.list.i = (this.list.i + n - 1) % n; this.renderList(); e.preventDefault(); }
      else if (e.key === 'Tab' && n) {
        e.preventDefault();
        const rest = inp.value.trim().split(/\s+/).slice(1).join(' ');
        inp.value = this.list.items[this.list.i][0] + (rest ? ` ${rest}` : ' ');
        this.renderList();
      } else if (e.key === 'Enter') { e.preventDefault(); this.take(); }
      else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); inp.value = ''; inp.blur(); }
      e.stopPropagation(); // the page's keys are not for the bar
    });
  }

  /** A key for the tool in hand: true when taken. */
  key(e) {
    if (!this.tool) return false;
    if (e.key === 'Escape') {
      if (this.tool.kind === 'line' && this.tool.points.length) { this.tool.points = []; this.paint(); return true; }
      this.stop();
      return true;
    }
    if (this.tool.kind === 'line' && e.key === 'Enter') { this.endLine(); return true; }
    if (this.tool.kind === 'line' && e.key === 'Backspace') { this.tool.points.pop(); this.paint(); return true; }
    return false;
  }

  // ---- what each tool does -----------------------------------------------
  footprint() {
    const t = this.tool;
    const size = t.el?.controls.find((c) => c.name === 'size')?.default;
    const own = t.extra.match(/size="?([^"\s]+)/);
    const nums = (own ? own[1] : size || '').match(/\d+(?:\.\d+)?/g)?.map(Number);
    if (nums?.length) return Math.max(...nums) / 2;
    return t.name === 'tree' ? 12 : t.el ? 5 : 8;
  }

  async act(press, p, x, y) {
    const t = this.tool;
    const node = (more) => [t.name, t.extra, more].filter(Boolean).join(' ');
    const size = this.api.size();
    if (t.kind === 'place') {
      const at = placeText(p.q, size[1]);
      await this.api.insert('stack', node(at));
      this.paint();
    } else if (t.kind === 'scatter') {
      const r = this.overlay.regionAt(press.q);
      const child = t.extra && !t.extra.includes('=') ? t.extra.split(/\s+/)[0] : 'rock';
      const more = t.extra && t.extra.includes('=') ? ` ${t.extra}` : '';
      this.stop();
      await this.api.insert('stack', `scatter${r ? ` at=${r}` : ''} count=3${more} {\n    ${child}\n}`, { lens: 'count' });
    } else if (t.kind === 'paint') {
      const r = this.overlay.regionAt(p.q);
      this.stop();
      await this.api.insert('stack', node(r ? `at=${r}` : ''));
    } else if (t.kind === 'run') {
      const l = this.overlay.lineAt(x, y, 14);
      const along = l || edgeOf(p.q, size);
      this.stop();
      await this.api.insert('stack', node(`at=${along}`));
    } else if (t.kind === 'line') {
      t.points.push(snap(p.q, size));
      this.paint();
    } else if (t.kind === 'region') {
      const lines = (this.api.map()?.lines || []).map((l) => l.name);
      if (!lines.length) { this.api.note('Draw a line first: a region is an area the lines enclose'); return; }
      const name = fresh('area-1', (this.api.map()?.regions || []).map((r) => r.name));
      this.stop();
      const fill = pointText(p.q, size[1]);
      await this.api.insert('tile', `region ${name} { boundedBy ${lines.join(' ')}; fill ${fill} }`, { select: { kind: 'region', id: name } });
    }
  }

  async endLine() {
    const t = this.tool;
    // a double-click's two clicks put the last point down twice
    const rows = this.api.size()[1];
    const w = (p) => written(p, rows);
    const pts = t.points.filter((p, i) => i === 0 || w(p) !== w(t.points[i - 1]));
    if (pts.length < 2) { this.api.note('A line needs two points at least'); return; }
    const name = fresh(t.extra && !t.extra.includes('=') ? t.extra.split(/\s+/)[0] : 'line-1', (this.api.map()?.lines || []).map((l) => l.name));
    const extra = t.extra.includes('=') ? ` ${t.extra}` : '';
    this.stop();
    await this.api.insert('tile', `line ${name}${extra} { through ${pts.map(w).join(' ')} }`, { select: { kind: 'line', id: name } });
  }

  // ---- handles ------------------------------------------------------------
  grab(i, e) {
    const h = this.api.handles()[i];
    if (!h) return;
    this.canvas.setPointerCapture(e.pointerId);
    const sq = this.api.sq();
    this.drag = { h, len: h.span[1], start: h.span[0], z: this.overlay.heightAt(h.q[0] * sq, h.q[1] * sq), moved: false };
  }

  dragTo(e) {
    const d = this.drag;
    const [x, y] = this.xy(e);
    const p = this.at(x, y, d.z);
    if (!p) return;
    const s = d.h.snap ? snap(p.q, this.api.size()) : { q: p.q, word: null };
    const rows = this.api.size()[1];
    const text = d.h.snap ? written(s, rows) : pointText(s.q, rows);
    if (text === d.text) return;
    d.text = text;
    d.moved = true;
    d.h.q = s.q;
    d.h.move?.(s.q);
    this.api.replace(d.start, d.len, text, false);
    d.len = bytes(text);
    this.paint();
  }

  letGo() {
    const d = this.drag;
    this.drag = null;
    if (d.moved) this.api.replace(d.start, d.len, d.text, true);
  }

  // ---- drawing --------------------------------------------------------------
  /** The overlay's tool drawing for now. */
  scene() {
    const s = {};
    const t = this.tool, h = this.hover;
    s.handles = this.api.handles().map((x, i) => ({ q: x.q, hot: i === this.hot || x === this.drag?.h }));
    if (t && h) {
      const size = this.api.size();
      if (t.kind === 'place') {
        s.ring = { q: h.q, r: this.footprint() };
        s.label = { q: h.q, text: pointLabel(h.q, size[1]) };
      } else if (t.kind === 'scatter') {
        s.selRegion = this.overlay.regionAt(this.press?.q || h.q);
        if (this.press?.moved) s.box = [this.press.q, h.q];
      } else if (t.kind === 'paint') {
        s.selRegion = this.overlay.regionAt(h.q);
        s.label = { q: h.q, text: s.selRegion ? `at=${s.selRegion}` : 'the whole tile' };
      } else if (t.kind === 'run') {
        s.selLine = this.overlay.lineAt(h.x, h.y, 14);
        s.label = { q: h.q, text: `at=${s.selLine || edgeOf(h.q, size)}` };
      } else if (t.kind === 'line') {
        const now = snap(h.q, size);
        s.path = [...t.points.map((p) => p.q), now.q];
        s.handles = [...s.handles, ...t.points.map((p) => ({ q: p.q })), { q: now.q, hot: true }];
        s.label = { q: now.q, text: now.word || pointLabel(now.q, size[1]) };
      } else if (t.kind === 'region') {
        s.ring = { q: h.q, r: 2 };
        s.label = { q: h.q, text: 'fill here' };
      }
    } else if (t?.kind === 'line' && t.points.length) {
      s.path = t.points.map((p) => p.q);
    }
    return s;
  }

  paint() { this.api.redraw(this.scene()); }
}
