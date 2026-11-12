// The editor's panel: where it is, and how wide.
//
// The left panel is one column that shows one level at a time (js/nav.js,
// editor/levels.js); the tile browser slides in beside it.  Here is only
// where that column sits: on the left or the right of the view, how wide, or
// hidden (Look: the view alone).  Its edge is a sash: drag it, or
// double-click it to reset.  \ hides or shows it.  The choice is remembered
// in this browser (dt-layout); below 900 px the stylesheet stacks the panel
// under the view and none of this applies.  (A wide-screen arrangement with two
// levels side by side may come; this is the narrow one.)

export const SASH = 6;
export const MIN = { panel: 300, view: 360 };
export const DEFAULTS = Object.freeze({ mode: 'edit', side: 'left', hidePanel: false, sidew: 440 });

/** A layout from anything stored: unknown or broken fields fall back to the defaults. */
export function normalise(v) {
  const o = { ...DEFAULTS };
  if (!v || typeof v !== 'object') return o;
  if (v.side === 'left' || v.side === 'right') o.side = v.side;
  // layouts from before the one panel: both panes hidden was Look; the sidebar's width is its own
  o.hidePanel = v.hidePanel === true || (v.hideSource === true && v.hideProps === true);
  if (Number.isFinite(v.sidew) && v.sidew > 0) o.sidew = v.sidew;
  return o;
}

/** The panel's width clamped so the view keeps its minimum in a window `w` wide. */
export function fit(o, w) {
  const r = { ...o };
  r.sidew = Math.min(Math.max(r.sidew, MIN.panel), Math.max(MIN.panel, w - MIN.view - SASH));
  return r;
}

/** The grid for a layout: `{columns, rows, areas}` as CSS strings. */
export function grid(o) {
  const q = (row) => `"${row.join(' ')}"`;
  if (o.hidePanel) return { columns: 'minmax(0, 1fr)', rows: '48px minmax(0, 1fr)', areas: [q(['title']), q(['view'])].join(' ') };
  const left = o.side === 'left';
  const cols = left ? [`${o.sidew}px`, `${SASH}px`, 'minmax(0, 1fr)'] : ['minmax(0, 1fr)', `${SASH}px`, `${o.sidew}px`];
  const row = left ? ['side', 'sash-side', 'view'] : ['view', 'sash-side', 'side'];
  return { columns: cols.join(' '), rows: '48px minmax(0, 1fr)', areas: [q(['title', 'title', 'title']), q(row)].join(' ') };
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
    const s = document.createElement('div');
    s.className = 'sash';
    s.id = 'sash-side';
    s.setAttribute('role', 'separator');
    s.setAttribute('aria-orientation', 'vertical');
    s.title = 'Drag to resize; double-click to reset';
    s.style.gridArea = 'sash-side';
    app.append(s);
    this.sash = s;
    this.drag(s);
    this.narrow.addEventListener('change', () => this.apply());
    addEventListener('resize', () => this.apply(false));
    this.apply(false);
  }

  get state() { return { ...this.o }; }

  set(patch) { this.o = normalise({ ...this.o, ...patch }); this.apply(); }
  /** Hide or show the panel. */
  toggle() { this.set({ hidePanel: !this.o.hidePanel }); }
  preset(name) { this.set({ hidePanel: name === 'look' }); }
  /** Whether the panel (the tile browser, the levels, the source) is showing. */
  get sideShown() { return this.narrow.matches || !this.o.hidePanel; }
  /** Show the panel, if Look hid it. */
  showSide() { if (!this.sideShown) this.set({ hidePanel: false }); }
  /** Which preset the layout is now. */
  get presetName() { return this.o.hidePanel ? 'look' : 'edit'; }

  apply(save = true) {
    const st = this.app.style;
    const narrow = this.narrow.matches;
    const o = fit(this.o, this.app.clientWidth || innerWidth);
    this.app.dataset.layout = o.hidePanel ? 'look' : 'edit';
    this.app.classList.toggle('hide-panel', o.hidePanel);
    const g = grid(o);
    if (narrow) {
      st.gridTemplateColumns = st.gridTemplateRows = st.gridTemplateAreas = '';
      this.side.hidden = false;
      this.sash.hidden = true;
    } else {
      st.gridTemplateColumns = g.columns;
      st.gridTemplateRows = g.rows;
      st.gridTemplateAreas = g.areas;
      this.side.hidden = o.hidePanel;
      this.sash.hidden = o.hidePanel;
    }
    if (save) { try { localStorage.setItem('dt-layout', JSON.stringify(this.o)); } catch { /* private mode: the default stands */ } }
    this.onChange(this.o);
  }

  drag(s) {
    s.addEventListener('dblclick', () => this.set({ sidew: DEFAULTS.sidew }));
    s.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      s.setPointerCapture(e.pointerId);
      s.classList.add('on');
      document.body.classList.add('resizing');
      const rect = this.app.getBoundingClientRect();
      const move = (ev) => {
        const fromLeft = ev.clientX - rect.left, fromRight = rect.right - ev.clientX;
        this.o.sidew = Math.round((this.o.side === 'left' ? fromLeft : fromRight) - SASH / 2);
        Object.assign(this.o, fit(this.o, rect.width));
        this.apply(false);
      };
      const up = () => {
        s.classList.remove('on');
        document.body.classList.remove('resizing');
        s.removeEventListener('pointermove', move);
        this.apply();
      };
      s.addEventListener('pointermove', move);
      s.addEventListener('pointerup', up, { once: true });
      s.addEventListener('pointercancel', up, { once: true });
    });
  }
}
