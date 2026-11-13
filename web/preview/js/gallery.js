// PARKED: nothing in the page uses this; the weighted-range gallery is off until the sheet writes units.  When it is switched on it is ported to a Preact component (view/) first, not switched on as it is.
// The weighting gallery under a range being edited (the value pop-up in
// lens.js): the weighting functions as small curve thumbnails, a pick that
// rewrites the source, and a slider for each number the function takes.
//
// Every curve and every piece of text comes from the program
// (`GET /api/weightings`, weightings.rs): the page sends text, the program
// reads it as one range with its odds and answers with the density on a
// grid.  Nothing here works out odds; this file only draws what it is given.
//
//   weightGallery({api, input, change, refuse, resized}) -> {node, show(on), refresh(), close()}
//
//   api.weighting({text} | {range} | {fn, range, args}) -> Promise<reply|null>
//       the program's answer (null when the call failed: already shown)
//   input     the field holding the source; a pick writes it
//   change()  the field was written here: the pop-up checks it again
//   refuse(m) the program refused the value: the pop-up's check line says so
//   resized() the gallery grew or shrank: the pop-up places itself again

/** A slider's travel in steps. */
export const STEPS = 1000;
/** A number with no top limit (a ratio) is slid between these, on a log scale. */
export const RATIO_SPAN = [0.1, 100];

const NS = 'http://www.w3.org/2000/svg';

/** Where along a slider (0 to STEPS) `value` of `arg` sits; clamped to the ends. */
export function sliderOf(arg, value) {
  const [lo, hi] = span(arg);
  const t = arg.most == null ? Math.log(value / lo) / Math.log(hi / lo) : (value - lo) / (hi - lo);
  return Math.round(Math.min(1, Math.max(0, Number.isFinite(t) ? t : 0)) * STEPS);
}

/** The number a slider position gives `arg`: two figures on the log scale, hundredths on the straight ones. */
export function valueOfSlider(arg, at) {
  const [lo, hi] = span(arg);
  const t = Math.min(1, Math.max(0, at / STEPS));
  if (arg.most == null) return Number((lo * (hi / lo) ** t).toPrecision(2));
  const v = Math.round((lo + (hi - lo) * t) * 100) / 100;
  return Math.min(hi, Math.max(lo, v));
}

function span(arg) {
  return arg.most == null ? RATIO_SPAN : [arg.least, arg.most];
}

/**
 * A curve (the program's, `{kind, values}`) as drawing: for a density, the
 * outline, its filled area and the height of the flat range's line; for a
 * list, one bar per value.  `w` by `h` with `pad` round it.  The height is
 * the curve's own top (at least a little over the flat line), so a gentle
 * curve is not squashed by a sharp one.
 */
export function curveShape(curve, w, h, pad = 2) {
  const base = h - pad;
  const room = h - 2 * pad;
  if (curve.kind === 'masses') {
    const top = Math.max(...curve.values.map((m) => m[1]));
    const bars = curve.values.map(([t, share]) => ({
      x: pad + t * (w - 2 * pad),
      y: base - (share / top) * room,
      h: (share / top) * room,
    }));
    return { bars };
  }
  const v = curve.values;
  const top = Math.max(1.15, ...v);
  const pts = v.map((d, i) => [pad + ((i + 0.5) / v.length) * (w - 2 * pad), base - (d / top) * room]);
  const f = (n) => Math.round(n * 100) / 100;
  const line = pts.map(([x, y], i) => `${i ? 'L' : 'M'}${f(x)} ${f(y)}`).join(' ');
  const area = `${line} L${f(pts[pts.length - 1][0])} ${f(base)} L${f(pts[0][0])} ${f(base)} Z`;
  return { line, area, flat: f(base - (1 / top) * room) };
}

const svg = (tag, attrs = {}) => {
  const n = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
  return n;
};
const html = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text != null) n.textContent = text;
  return n;
};

/** A curve as an SVG of `w` by `h`. */
export function drawCurve(curve, w, h) {
  const s = svg('svg', { viewBox: `0 0 ${w} ${h}`, class: 'gal-curve', 'aria-hidden': 'true', preserveAspectRatio: 'none' });
  const shape = curveShape(curve, w, h);
  if (shape.bars) {
    for (const b of shape.bars) s.append(svg('rect', { class: 'gal-bar', x: b.x - 1.5, y: b.y, width: 3, height: b.h }));
    return s;
  }
  s.append(
    svg('path', { class: 'gal-area', d: shape.area }),
    svg('line', { class: 'gal-flat', x1: 0, x2: w, y1: shape.flat, y2: shape.flat }),
    svg('path', { class: 'gal-line', d: shape.line }),
  );
  return s;
}

/** A slider (see `.slide` in theme.css): pointer and keys; `onSlide(position)` on every move. */
function slider(label, onSlide) {
  const root = html('div', 'slide');
  root.setAttribute('role', 'slider');
  root.tabIndex = -1; // the field keeps the focus; the pointer drives it
  const thumb = html('span', 'slide-thumb');
  root.append(html('span', 'slide-rail'), thumb);
  root.setAttribute('aria-label', label);
  root.setAttribute('aria-valuemin', '0');
  root.setAttribute('aria-valuemax', String(STEPS));
  const at = (e) => {
    const r = root.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * STEPS);
  };
  // the drag is followed on the window, so it goes on past the rail's ends
  const move = (e) => { const pos = at(e); set(pos); onSlide(pos, true); };
  const up = () => {
    root.classList.remove('on');
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('pointercancel', up);
    onSlide(null, false);
  };
  root.addEventListener('pointerdown', (e) => {
    root.classList.add('on');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
    move(e);
  });
  function set(position) {
    thumb.style.left = `${(position / STEPS) * 100}%`;
    root.setAttribute('aria-valuenow', String(position));
  }
  return { root, set };
}

export function weightGallery({ api, input, change, refuse, resized }) {
  const node = html('div', 'vp-gallery');
  node.hidden = true;
  const grid = html('div', 'gal-grid');
  const now = html('div', 'gal-now');
  const sliders = html('div', 'gal-sliders');
  const note = html('div', 'gal-note');
  node.append(grid, now, sliders, note);

  let open = false;
  let functions = [];   // the program's functions over the loaded range, each with its text and curve
  let loaded = null;    // the range they are over, as the program wrote it
  let current = null;   // the program's reading of the field
  let shownFn = null;   // the function the sliders are for
  let dragging = false;
  let asked = 0;
  const rows = [];      // [{arg, slide, out}]

  const fmt = (x) => String(Number(x.toPrecision(4)));
  const refusal = (r) => `${r.message}${r.help ? `: ${r.help}` : ''}`;

  function drawGrid() {
    grid.replaceChildren();
    for (const f of functions) {
      const b = html('button', `gal-item${current?.function === f.name ? ' on' : ''}`);
      b.type = 'button';
      b.title = `${f.call}: ${f.about}`;
      b.setAttribute('aria-label', f.name);
      b.setAttribute('aria-pressed', String(current?.function === f.name));
      b.append(drawCurve(f.curve, 64, 34), html('span', 'gal-name', f.name === 'list' ? 'list' : f.name));
      b.onclick = () => pick(f);
      grid.append(b);
    }
  }

  function drawNow() {
    now.replaceChildren();
    if (!current) return;
    const bigger = drawCurve(current.curve, 220, 56);
    now.append(bigger, html('div', 'gal-axis', `${fmt(current.range[0])} to ${fmt(current.range[1])}`));
  }

  function drawSliders() {
    const f = functions.find((x) => x.name === current?.function);
    if (!f?.args?.length) {
      sliders.replaceChildren();
      rows.length = 0;
      shownFn = f?.name ?? null;
      return;
    }
    const numbers = f.args.map((a, i) => current.numbers[i] ?? a.default);
    if (shownFn !== f.name) {
      shownFn = f.name;
      sliders.replaceChildren();
      rows.length = 0;
      f.args.forEach((arg, i) => {
        const out = html('span', 'gal-out');
        const s = slider(`${f.name} ${arg.name}`, (pos, down) => {
          dragging = down;
          if (pos != null) slide(f, arg, i, pos);
        });
        sliders.append(html('span', 'gal-arg', arg.name), s.root, out);
        rows.push({ arg, s, out });
      });
    }
    if (!dragging) rows.forEach((r, i) => { r.s.set(sliderOf(r.arg, numbers[i])); });
    rows.forEach((r, i) => { r.out.textContent = fmt(numbers[i]); });
  }

  function render() {
    drawGrid();
    drawNow();
    drawSliders();
    const f = functions.find((x) => x.name === current?.function);
    note.textContent = f ? `${f.call}: ${f.about}` : '';
    note.hidden = !f;
    resized();
  }

  async function load(range) {
    const r = await api.weighting({ range });
    if (!r) return false;
    if (!r.ok) { note.textContent = r.message; note.hidden = false; return false; }
    functions = r.functions;
    loaded = `${r.range[0]}..${r.range[1]}`;
    return true;
  }

  /** Read the field through the program and draw what it says. */
  async function refresh() {
    if (!open) return;
    const text = input.value.trim();
    const ask = ++asked;
    const r = text ? await api.weighting({ text }) : null;
    if (ask !== asked) return;
    if (!r || !r.ok) {
      current = null;
      // a number past its limits is the check line's (the program's message
      // there); anything else (a number, a sum) is said here
      if (r && ['dt::range', 'dt::weighting'].includes(r.code)) refuse(refusal(r));
      grid.replaceChildren();
      now.replaceChildren();
      sliders.replaceChildren();
      rows.length = 0;
      shownFn = null;
      functions = [];
      loaded = null;
      // a value that is not one range (a name, a sum) has nothing to weight: the gallery steps aside
      const mine = r && ['dt::range', 'dt::weighting'].includes(r.code);
      node.hidden = !mine;
      note.textContent = mine ? 'fix the numbers to see the curve' : '';
      note.hidden = !mine;
      resized();
      return;
    }
    if (r.range[0] === r.range[1]) {
      current = null;
      functions = [];
      loaded = null;
      grid.replaceChildren();
      now.replaceChildren();
      sliders.replaceChildren();
      node.hidden = false;
      note.textContent = 'a range with some width can be weighted';
      note.hidden = false;
      resized();
      return;
    }
    const key = `${r.range[0]}..${r.range[1]}`;
    if (key !== loaded && !(await load(key))) return;
    if (ask !== asked) return;
    current = r;
    node.hidden = false;
    render();
  }

  /** Pick a function: the source becomes its text over the range. */
  function pick(f) {
    input.value = f.text;
    refresh().then(change);
  }

  /** A slider moved: the program writes the function with that number and draws it. */
  async function slide(f, arg, i, position) {
    const numbers = f.args.map((a, k) => (k === i ? valueOfSlider(arg, position) : current.numbers[k] ?? a.default));
    const ask = ++asked;
    const r = await api.weighting({ fn: f.name, range: loaded, args: numbers.join(',') });
    if (!r || ask !== asked) return;
    if (!r.ok) { refuse(refusal(r)); return; }
    input.value = r.text;
    current = r;
    change();
    drawNow();
    drawSliders();
  }

  return {
    node,
    /** Whether the gallery may show (the value is a range); the program's reading still decides. */
    open(on) {
      if (open === on) return;
      open = on;
      node.hidden = !on;
      if (!on) asked += 1;
      resized();
    },
    refresh,
    /** The function the field holds now, for the pop-up's form buttons: 'flat', 'low', ... or null. */
    get function() { return current?.function ?? null; },
    /** Pick the first weighted function over the loaded range (the pop-up's "weighted"). */
    pickWeighted() {
      const f = functions.find((x) => x.name === 'low');
      if (f) pick(f);
    },
    close() { open = false; asked += 1; node.remove(); },
  };
}
