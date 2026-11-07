// The editor's panes: which are shown, where, and how big.
//
// Two arrangements: Edit (one sidebar, the text over the properties) and
// Inspect (the text, the view, the properties side by side).  The sidebar
// is two pages: the tile browser, and beside it the editor page it slides
// to when a tile is chosen (main.js moves between them).  Look hides
// both panes for the whole view.  Every pane edge is a sash: drag it, or
// double-click it to reset.  [ hides the text, ] the properties, \ both.
// The choice is remembered in this browser (dt-layout); below 900 px the
// stylesheet stacks the panes under the view and none of this applies.

export const SASH = 6;
export const MIN = { text: 240, props: 220, view: 360, rows: 120 };
export const DEFAULTS = Object.freeze({
  mode: 'edit', side: 'left', hideText: false, hideProps: false,
  sidew: 520, texth: 0.6, textw: 400, propw: 300,
});

/** A layout from anything stored: unknown or broken fields fall back to the defaults. */
export function normalise(v) {
  const o = { ...DEFAULTS };
  if (!v || typeof v !== 'object') return o;
  if (v.mode === 'edit' || v.mode === 'inspect') o.mode = v.mode;
  if (v.side === 'left' || v.side === 'right') o.side = v.side;
  o.hideText = v.hideText === true;
  o.hideProps = v.hideProps === true;
  for (const k of ['sidew', 'textw', 'propw']) if (Number.isFinite(v[k]) && v[k] > 0) o[k] = v[k];
  if (Number.isFinite(v.texth) && v.texth > 0 && v.texth < 1) o.texth = v.texth;
  return o;
}

/** Pane sizes clamped so the view keeps its minimum in a window `w` wide. */
export function fit(o, w) {
  const r = { ...o };
  const room = (n) => Math.max(0, w - MIN.view - n * SASH);
  if (r.mode === 'edit') {
    r.sidew = Math.min(Math.max(r.sidew, MIN.text), Math.max(MIN.text, room(1)));
  } else {
    const shown = (!r.hideText) + (!r.hideProps);
    let tw = r.hideText ? 0 : Math.max(r.textw, MIN.text);
    let pw = r.hideProps ? 0 : Math.max(r.propw, MIN.props);
    const over = tw + pw - room(shown);
    if (over > 0) { // take it from the wider pane first
      const cut = (a, b, n) => { const t = Math.min(n, Math.max(0, a - b)); return [a - t, n - t]; };
      let rest;
      if (tw >= pw) { [tw, rest] = cut(tw, r.hideText ? 0 : MIN.text, over); [pw] = cut(pw, r.hideProps ? 0 : MIN.props, rest); }
      else { [pw, rest] = cut(pw, r.hideProps ? 0 : MIN.props, over); [tw] = cut(tw, r.hideText ? 0 : MIN.text, rest); }
    }
    if (!r.hideText) r.textw = tw;
    if (!r.hideProps) r.propw = pw;
  }
  return r;
}

/**
 * The grid for a layout: {columns, rows, areas} for the page as CSS
 * strings, {rows, areas} for the editor page inside the sidebar, and
 * where the properties go ('editor': under the text; 'app': their own
 * column).  The sidebar (`side`) holds the tile browser and, slid in
 * beside it, the editor page.
 */
export function grid(o) {
  const q = (row) => `"${row.join(' ')}"`;
  if (o.hideText && o.hideProps) {
    return { columns: 'minmax(0, 1fr)', rows: '48px minmax(0, 1fr)', areas: [q(['title']), q(['view'])].join(' '), editor: { rows: 'minmax(0, 1fr)', areas: q(['text']) }, props: 'editor' };
  }
  if (o.mode === 'edit') {
    const left = o.side === 'left';
    const cols = left ? [`${o.sidew}px`, `${SASH}px`, 'minmax(0, 1fr)'] : ['minmax(0, 1fr)', `${SASH}px`, `${o.sidew}px`];
    const row = left ? ['side', 'sash-side', 'view'] : ['view', 'sash-side', 'side'];
    let editor;
    if (o.hideText || o.hideProps) editor = { rows: 'minmax(0, 1fr)', areas: q([o.hideText ? 'properties' : 'text']) };
    else {
      const pct = Math.round(o.texth * 1000) / 10;
      editor = {
        rows: `minmax(${MIN.rows}px, ${pct}fr) ${SASH}px minmax(${MIN.rows}px, ${Math.round((100 - pct) * 10) / 10}fr)`,
        areas: [q(['text']), q(['sash-split']), q(['properties'])].join(' '),
      };
    }
    return { columns: cols.join(' '), rows: '48px minmax(0, 1fr)', areas: [q(['title', 'title', 'title']), q(row)].join(' '), editor, props: 'editor' };
  }
  // inspect: the sidebar (browser, text) on its side, the properties opposite
  const [first, last] = o.side === 'left' ? ['side', 'properties'] : ['properties', 'side'];
  const width = { side: `${o.textw}px`, properties: `${o.propw}px` };
  const hidden = { side: o.hideText, properties: o.hideProps };
  const areas = [], cols = [];
  if (!hidden[first]) { areas.push(first, 'sash-side'); cols.push(width[first], `${SASH}px`); }
  areas.push('view'); cols.push('minmax(0, 1fr)');
  if (!hidden[last]) { areas.push('sash-props', last); cols.push(`${SASH}px`, width[last]); }
  return {
    columns: cols.join(' '), rows: '48px minmax(0, 1fr)', areas: [q(areas.map(() => 'title')), q(areas)].join(' '),
    editor: { rows: 'minmax(0, 1fr)', areas: q(['text']) }, props: 'app',
  };
}

export class Layout {
  /** `app` is the grid; `onChange` runs after every change (the view resizes itself). */
  constructor(app, onChange = () => {}) {
    this.app = app;
    this.onChange = onChange;
    this.narrow = matchMedia('(max-width: 900px)');
    let stored = null;
    try { stored = JSON.parse(localStorage.getItem('dt-layout') || 'null'); } catch { /* private mode, or nothing kept */ }
    this.o = normalise(stored);
    this.side = app.querySelector('#side');
    this.editor = app.querySelector('#editor');
    this.props = app.querySelector('#properties-pane');
    this.sashes = {};
    for (const name of ['sash-side', 'sash-split', 'sash-props']) {
      const s = document.createElement('div');
      s.className = 'sash';
      s.id = name;
      s.setAttribute('role', 'separator');
      s.setAttribute('aria-orientation', name === 'sash-split' ? 'horizontal' : 'vertical');
      s.title = 'Drag to resize; double-click to reset';
      s.style.gridArea = name;
      (name === 'sash-split' ? this.editor : app).append(s);
      this.sashes[name] = s;
      this.drag(s, name);
    }
    this.narrow.addEventListener('change', () => this.apply());
    addEventListener('resize', () => this.apply(false));
    this.apply(false);
  }

  get state() { return { ...this.o }; }

  set(patch) { this.o = normalise({ ...this.o, ...patch }); this.apply(); }
  toggle(pane) {
    if (pane === 'both') {
      const all = this.o.hideText && this.o.hideProps;
      this.set({ hideText: !all, hideProps: !all });
    } else this.set(pane === 'text' ? { hideText: !this.o.hideText } : { hideProps: !this.o.hideProps });
  }
  preset(name) {
    if (name === 'look') this.set({ hideText: true, hideProps: true });
    else this.set({ mode: name, hideText: false, hideProps: false });
  }
  /** Whether the sidebar (the tile browser, the text) is showing. */
  get sideShown() {
    if (this.narrow.matches) return true;
    return this.o.mode === 'edit' ? !(this.o.hideText && this.o.hideProps) : !this.o.hideText;
  }
  /** Show the sidebar, if a layout hid it. */
  showSide() { if (!this.sideShown) this.set({ hideText: false }); }
  /** Which preset the layout is now, if any. */
  get presetName() {
    if (this.o.hideText && this.o.hideProps) return 'look';
    if (this.o.hideText || this.o.hideProps) return null;
    return this.o.mode;
  }

  apply(save = true) {
    const st = this.app.style;
    const narrow = this.narrow.matches;
    const o = fit(this.o, this.app.clientWidth || innerWidth);
    this.app.dataset.layout = o.mode;
    this.app.classList.toggle('hide-text', o.hideText);
    this.app.classList.toggle('hide-props', o.hideProps);
    const g = grid(o);
    const place = (where) => {
      const parent = where === 'app' ? this.app : this.editor;
      if (this.props.parentElement !== parent) parent.append(this.props);
    };
    const es = this.editor.style;
    if (narrow) {
      st.gridTemplateColumns = st.gridTemplateRows = st.gridTemplateAreas = '';
      es.gridTemplateRows = 'minmax(0, 1fr)'; es.gridTemplateAreas = '"text"';
      place('app');
      this.side.hidden = false;
      for (const s of Object.values(this.sashes)) s.hidden = true;
    } else {
      st.gridTemplateColumns = g.columns;
      st.gridTemplateRows = g.rows;
      st.gridTemplateAreas = g.areas;
      es.gridTemplateRows = g.editor.rows;
      es.gridTemplateAreas = g.editor.areas;
      place(g.props);
      this.side.hidden = !g.areas.includes('side');
      for (const [name, s] of Object.entries(this.sashes)) s.hidden = !(name === 'sash-split' ? g.editor.areas : g.areas).includes(name);
    }
    if (save) { try { localStorage.setItem('dt-layout', JSON.stringify(this.o)); } catch { /* ignore */ } }
    this.onChange(this.o);
  }

  drag(s, name) {
    s.addEventListener('dblclick', () => {
      const d = DEFAULTS;
      if (name === 'sash-split') this.set({ texth: d.texth });
      else if (this.o.mode === 'edit') this.set({ sidew: d.sidew });
      else this.set(this.paneAt(name) === 'text' ? { textw: d.textw } : { propw: d.propw });
    });
    s.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      s.setPointerCapture(e.pointerId);
      s.classList.add('on');
      document.body.classList.add(name === 'sash-split' ? 'resizing-rows' : 'resizing');
      const rect = this.app.getBoundingClientRect();
      const ed = this.editor.getBoundingClientRect();
      const move = (ev) => {
        if (name === 'sash-split') {
          this.o.texth = Math.min(0.9, Math.max(0.1, (ev.clientY - ed.top) / (ed.height - SASH)));
        } else {
          const fromLeft = ev.clientX - rect.left, fromRight = rect.right - ev.clientX;
          const pane = this.paneAt(name);
          // in Edit the sidebar's sash is on its side; in Inspect sash-side is always the left one
          const onLeft = this.o.mode === 'edit' ? this.o.side === 'left' : name === 'sash-side';
          const w = Math.round((onLeft ? fromLeft : fromRight) - SASH / 2);
          if (this.o.mode === 'edit') this.o.sidew = w;
          else if (pane === 'text') this.o.textw = w;
          else this.o.propw = w;
          Object.assign(this.o, fit(this.o, rect.width));
        }
        this.apply(false);
      };
      const up = () => {
        s.classList.remove('on');
        document.body.classList.remove('resizing', 'resizing-rows');
        s.removeEventListener('pointermove', move);
        this.apply();
      };
      s.addEventListener('pointermove', move);
      s.addEventListener('pointerup', up, { once: true });
      s.addEventListener('pointercancel', up, { once: true });
    });
  }

  /** In Inspect, the pane a sash sizes. */
  paneAt(name) {
    const left = this.o.side === 'left';
    if (this.o.mode === 'edit') return 'side';
    return (name === 'sash-side') === left ? 'text' : 'properties';
  }
}
