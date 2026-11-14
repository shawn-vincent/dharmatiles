// A live example card: the source, the tile, the controls the page picked, the messages and the overlay, once the reader has
// touched it.  Everything it shows is the editor's own code or the program's own words:
//
//   messages     web/preview/js/messages.js (the store, the status line's words) and view/messages-panel.js (the list)
//   status line  web/preview/js/view/status-line.js, the component the editor's bottom bar uses too
//   overlay      overlay-marks.js (marks, states, caret and hover lookups) and overlay-render.js (the flat renderer), over
//                overlay-geometry.js; the same geometry the editor draws into its 3D view
//   the map      dt_eval::map, the program's one producer, through the worker's `check_tile`
//
// The editor's files are read from `<site>/editor/js` and `<site>/editor/vendor`, one URL for web/preview/js and vendor that every place serving
// the site provides (the deploy job, the program's docs route), so the site reads the code the program ships: there is no copy.

import { signal, computed } from '@preact/signals-core';
import { buildStatusLine, paintStatusLine } from '../../editor/js/view/status-line.js';
import { MessagesBody } from '../../editor/js/view/messages-panel.js';
import { html, render } from '../../editor/js/view/html.js';
import { detailsFor, setExplainBase, statusOf, countsText } from '../../editor/js/messages.js';
import { drawn, marksOf, markAtOffset, parsePins, sourceRange } from '../../editor/js/overlay-marks.js';
import { markAtPoint, renderMarks } from '../../editor/js/overlay-render.js';
import { heightAtGrid, heightGrid } from '../../editor/js/overlay-geometry.js';
import { byteToIndex } from '../../editor/js/spans.js';
import { DRAFT_CELLS, DRAFT_TIER } from './engine.js';
import { CardLog } from './site-log.js';
import { SourcePane } from './source-pane.js';
import { findControl, parseControls, rangeOf, readValue, writeValue } from './controls.js';
import { LiteViewer } from './viewer-lite.js';

const TOGGLES = [['lines', 'Lines'], ['points', 'Points'], ['regions', 'Regions'], ['squares', 'Squares']];
let serial = 0;
/** The cards that hold a WebGL context, least recently used first: a page keeps only a few live. */
const holders = [];
const MAX_CONTEXTS = 4;

/** @param {string} tag @param {string} [cls] @param {Record<string, string>} [attrs] */
function el(tag, cls = '', attrs = {}) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
}

export class Card {
  /** @param {HTMLElement} host the <dt-example> @param {import('./engine.js').Engine} engine */
  constructor(host, engine) {
    this.host = host;
    this.engine = engine;
    this.id = `card-${++serial}`;
    this.name = (host.getAttribute('src') ?? 'tile').split('/').pop().replace(/\.dtile$/, '');
    this.system = host.getAttribute('system') ?? 'db';
    setExplainBase(new URL('e/', engine.root).href);

    const code = host.querySelector('.dtx-code code');
    this.original = (code?.textContent ?? '').replace(/\n$/, '\n');
    this.text = this.original;
    this.version = 0;
    this.checked = null;
    this.checkedText = null;
    this.marks = [];
    this.map = null;
    this.mesh = null;
    this.heights = null;
    this.fullAt = null;
    this.wantDownload = false;
    this.status = 'waking';
    this.progress = null;
    this.queuedBehind = 0;
    this.log = new CardLog(this.id);
    this.moment = {
      toggles: new Set((host.getAttribute('overlay') ?? '').split(/[\s,]+/).filter(Boolean)),
      pinned: new Set(), hover: null, caret: null,
    };
    this.pinSpec = host.getAttribute('highlight') ?? '';
    this.controls = parseControls(host.getAttribute('controls') ?? '');
    this.opened = false;
    this.seenErrors = new Set();
    this.debounce = 0;

    this.#build();
    this.handle = engine.register(this.id, (ev) => this.#engine(ev));
    this.handle.focus();
    this.#edit();
  }

  // ---- the page ----------------------------------------------------------

  #build() {
    const host = this.host;
    host.dataset.state = 'waking';
    const grid = host.querySelector('.dtx-grid') ?? host.appendChild(el('div', 'dtx-grid'));
    this.view = grid.querySelector('.dtx-view') ?? grid.appendChild(el('figure', 'dtx-view'));
    this.poster = this.view.querySelector('.dtx-poster');
    host.querySelector('.dtx-wake')?.remove();
    // the card draws its own marks now: the ones on the poster are put away
    host.querySelector('.dtx-pre')?.remove();
    this.gl = el('canvas', 'dtx-gl', { 'aria-label': 'The tile, in 3D: drag to turn it', role: 'img', tabindex: '0' });
    this.ov = el('canvas', 'dtx-ov', { 'aria-hidden': 'true' });
    this.badge = el('div', 'dtx-badge', { role: 'status' });
    this.view.append(this.gl, this.ov, this.badge);

    const side = el('div', 'dtx-side');
    // the toggles: what to show on the tile
    const tg = el('div', 'dtx-toggles', { role: 'group', 'aria-label': 'Show on the tile' });
    const lab = el('span', 'dtx-label');
    lab.textContent = 'Show on the tile';
    tg.append(lab);
    this.toggleButtons = new Map();
    for (const [name, label] of TOGGLES) {
      const b = el('button', 'btn sm', { type: 'button', 'aria-pressed': String(this.moment.toggles.has(name)) });
      b.textContent = label;
      b.addEventListener('click', () => {
        if (this.moment.toggles.has(name)) this.moment.toggles.delete(name); else this.moment.toggles.add(name);
        b.setAttribute('aria-pressed', String(this.moment.toggles.has(name)));
        this.#paint();
      });
      this.toggleButtons.set(name, b);
      tg.append(b);
    }
    this.controlsBox = el('div', 'dtx-controls');
    this.sourceBox = el('div');
    this.pane = new SourcePane(this.sourceBox, {
      text: this.text, label: `Source of ${this.name}`,
      onInput: (t) => { this.text = t; this.#edited(); },
      onCaret: (o) => { this.moment.caret = o < 0 ? null : markAtOffset(this.marks, this.text, o); this.#paint(); },
      onHover: (o) => { this.moment.hover = o == null ? null : markAtOffset(this.marks, this.text, o); this.#paint(); },
    });
    const actions = el('div', 'dtx-actions');
    this.fullBtn = this.#button('Full detail', 'Build what the printer is given (slower)', () => this.#full(false));
    this.stlBtn = this.#button('Download STL', 'The STL the printer gets: always built at full detail', () => this.#download());
    this.openLink = el('a', 'btn sm', { href: '#', target: '_blank', rel: 'noopener', title: 'Open this source in the playground' });
    this.openLink.textContent = 'Open in editor';
    const refresh = () => { this.openLink.href = this.#playgroundHref(); };
    for (const ev of ['pointerdown', 'focus', 'mouseover']) this.openLink.addEventListener(ev, refresh);
    this.resetBtn = this.#button('Reset', 'Back to the source as the page wrote it', () => { this.pane.set(this.original); this.text = this.original; this.#edited(); });
    actions.append(this.fullBtn, this.stlBtn, this.openLink, this.resetBtn);

    // the status line, and the messages it folds
    this.sl = buildStatusLine({ onToggle: () => this.#fold(!this.model.open.value), controls: `${this.id}-msgs` });
    this.msgs = el('div', 'dtx-msgs msglist', { id: `${this.id}-msgs`, role: 'region', 'aria-label': 'Messages' });
    this.msgs.hidden = true;
    this.model = this.#model();
    render(html`<${MessagesBody} m=${this.model} act=${this.#act()} />`, this.msgs);

    side.append(tg, this.controlsBox, this.sourceBox, actions, this.sl.root, this.msgs);
    grid.append(side);
    host.querySelector('.dtx-code')?.remove();

    this.#sizeObserver();
    this.#pointer();
  }

  #button(label, title, on) {
    const b = el('button', 'btn sm', { type: 'button', title });
    b.textContent = label;
    b.addEventListener('click', on);
    return b;
  }

  /** The model the editor's messages list draws from: signals over this card's own log. */
  #model() {
    this.rev = signal(0);
    this.tileState = signal({ id: this.id, name: this.name, status: 'idle', path: 'tile.dtile', shown: null, errors: [] });
    this.progressSig = signal(null);
    return {
      open: signal(false),
      view: computed(() => { void this.rev.value; return this.log.view(); }),
      filter: signal('all'),
      lit: signal(null),
      clearLit: () => {},
      tile: computed(() => this.tileState.value),
      progress: computed(() => this.progressSig.value),
      angles: signal({ overhangDeg: 55, steepestDeg: 65 }),
      shown: signal(null),
      file: signal('tile.dtile'),
      text: signal(this.text),
    };
  }

  #act() {
    return {
      setFilter: (id) => { this.model.filter.value = id; },
      showLine: (_file, line) => this.pane.showLine(line),
      openPanel: () => this.#fold(true),
      showNode: () => {},
      dismiss: (seq) => { this.log.log.dismiss(seq); this.#touch(); },
      details: (m, source) => detailsFor(m, { source }),
      canShowWorst: () => false,
      underside: () => null,
      showWorst: () => {},
      applyFix: (m) => this.#applyFix(m),
      allText: async () => this.model.view.value.rows.map((r) => `${r.m.severity}: ${r.m.code ?? ''} ${r.m.text}`).join('\n') || 'No messages.',
    };
  }

  /** The source with the program's fix laid in, by the engine (`Op::Fix`): the edit goes through the source like any other. */
  async #applyFix(m) {
    if (!m.fix) return;
    const was = this.text;
    let next;
    try {
      next = await this.engine.fix(was, m.fix);
    } catch (e) {
      this.log.failed(`The fix could not be made: ${e.message}`, e.stack ?? '');
      this.#touch();
      return;
    }
    // the source was edited while the engine made the fix: the fix was for the text it read
    if (this.text !== was) {
      this.log.failed('The source has changed since the program read it: the fix waits for the next check.', '');
      this.#touch();
      return;
    }
    this.text = next;
    this.pane.set(next);
    this.#edited();
  }

  /** Fold the list open or shut. */
  #fold(open) {
    this.model.open.value = open;
    this.msgs.hidden = !open;
    this.#status();
  }

  #sizeObserver() {
    const ro = new ResizeObserver(() => this.#resize());
    ro.observe(this.view);
    this.ro = ro;
  }

  #resize() {
    const r = this.view.getBoundingClientRect();
    const dpr = Math.min(devicePixelRatio || 1, 2);
    this.ov.width = Math.round(r.width * dpr);
    this.ov.height = Math.round(r.height * dpr);
    this.size = { w: r.width, h: r.height };
    this.viewer?.resize(r.width, r.height);
    this.#paint();
  }

  // ---- the tile ------------------------------------------------------------

  /** Where a point of the tile falls on the overlay: the camera's, or the poster's own top view before there is a mesh. */
  #projector() {
    if (this.viewer && this.mesh) return (x, y, z = 0) => this.viewer.project(x, y, z);
    const [fw, fh] = this.mesh?.footprint ?? this.#guessFootprint();
    const { w, h } = this.size ?? { w: 1, h: 1 };
    const s = Math.min(w / fw, h / fh);
    const ox = (w - fw * s) / 2, oy = (h - fh * s) / 2;
    return (x, y) => ({ x: ox + x * s, y: oy + (fh - y) * s });
  }

  #guessFootprint() {
    const sq = this.checked?.square_mm ?? 35;
    return [(this.map?.cols ?? 1) * sq, (this.map?.rows ?? 1) * sq];
  }

  #zAt = (x, y) => (this.viewer && this.heights ? heightAtGrid(this.heights, x, y, 4) : 0);

  #paint() {
    if (!this.ov || !this.size) return;
    const dpr = this.ov.width / (this.size.w || 1);
    const ctx = this.ov.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const moment = { toggles: this.moment.toggles, pinned: this.moment.pinned, hover: this.moment.hover, caret: this.moment.caret };
    this.drawnNow = drawn(this.marks, moment);
    const squares = this.moment.toggles.has('squares');
    renderMarks(ctx, {
      map: this.map, sq: this.checked?.square_mm ?? 35, drawn: this.drawnNow, squares,
      project: this.#projector(), zAt: this.#zAt, size: this.size,
    });
    this.#markSource();
  }

  /** The marks in the source: an underline for each open message, and the lit and pinned spans. */
  #markSource() {
    const text = this.text;
    const ranges = [];
    if (this.checkedText === text) {
      for (const { m } of this.log.view().rows) {
        if (!m.span || m.scope !== 'check') continue;
        const from = byteToIndex(text, m.span.offset);
        ranges.push({ from, to: byteToIndex(text, m.span.offset + Math.max(1, m.span.len)), cls: m.severity === 'error' ? 'err' : 'note' });
      }
      for (const { mark, state } of this.drawnNow ?? []) {
        if (state !== 'lit') continue;
        const r = sourceRange(mark, text);
        if (r) ranges.push({ from: r[0], to: r[1], cls: this.moment.pinned.has(mark.id) ? 'pin' : 'lit' });
      }
    }
    this.pane.mark(ranges);
  }

  #pointer() {
    const over = (e) => {
      if (e.buttons) return;
      const r = this.gl.getBoundingClientRect();
      const x = e.clientX - r.left, y = e.clientY - r.top;
      const o = { map: this.map, sq: this.checked?.square_mm ?? 35, drawn: this.drawnNow ?? [], project: this.#projector(), zAt: this.#zAt };
      // a shown point or line under the pointer lights its source (a region is lit from the source or its toggle)
      this.moment.hover = markAtPoint(o, x, y);
      this.#paint();
    };
    this.gl.addEventListener('pointermove', over);
    this.gl.addEventListener('pointerleave', () => { this.moment.hover = null; this.#paint(); });
    this.gl.addEventListener('pointerdown', () => this.handle.focus());
    this.pane.area.addEventListener('focus', () => this.handle.focus());
    this.host.addEventListener('pointerdown', () => this.handle.focus(), true);
  }

  #showMesh(positions, colours, footprint) {
    if (!this.viewer) {
      // the canvas is shown before its context is made: a context made on a hidden canvas has nothing to draw into
      this.host.dataset.view = 'live';
      this.#resize();
      this.viewer = new LiteViewer(this.gl, () => this.#paint());
      holders.push(this);
      while (holders.length > MAX_CONTEXTS) holders.shift()?.sleep();
      this.viewer.resize(this.size?.w ?? 300, this.size?.h ?? 300);
    }
    this.mesh = { positions, colours, footprint };
    this.heights = heightGrid(positions, positions.length / 9, footprint);
    this.viewer.setMesh(positions, colours, footprint);
    this.host.dataset.view = 'live';
  }

  /** Give the WebGL context back for another card: the poster shows again until this one is touched. */
  sleep() {
    this.viewer?.release();
    this.viewer = null;
    this.host.dataset.view = 'poster';
    this.#paint();
  }

  // ---- editing and building -------------------------------------------------

  #edited() {
    this.version++;
    this.model.text.value = this.text;
    this.checkedText = null;
    this.#markSource();
    clearTimeout(this.debounce);
    this.debounce = setTimeout(() => this.#edit(), 160);
  }

  #edit() {
    this.status = 'building';
    this.log.begin();
    this.tileState.value = { ...this.tileState.value, status: 'building' };
    this.handle.edit({ text: this.text, system: this.system, seed: -1, cells: DRAFT_CELLS, tier: DRAFT_TIER, version: this.version });
    this.#touch();
  }

  #full(download) {
    this.wantDownload = download;
    this.status = 'building';
    this.log.begin();
    this.tileState.value = { ...this.tileState.value, status: 'building' };
    this.handle.focus();
    this.handle.full({ text: this.text, system: this.system, seed: -1, version: this.version });
    this.#touch();
  }

  #download() {
    if (this.fullAt === this.version && this.stl) this.#save();
    else this.#full(true);
  }

  #save() {
    const url = URL.createObjectURL(new Blob([this.stl], { type: 'model/stl' }));
    const a = el('a', '', { href: url, download: `${this.name}-${this.system}.stl` });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }

  #playgroundHref() {
    const b64 = btoa(String.fromCharCode(...new TextEncoder().encode(this.text))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    return `${new URL('playground.html', this.engine.root).href}#src=${b64}`;
  }

  // ---- what the engine says --------------------------------------------------

  #engine(ev) {
    // a result for an older edit is dropped, never painted (the engine keeps it in its cache)
    if (ev.version !== this.version && ev.type !== 'queued') return;
    switch (ev.type) {
      case 'queued':
        if (ev.version !== this.version) return;
        this.buildKind = ev.kind;
        this.status = 'queued';
        this.queuedBehind = ev.behind;
        break;
      case 'started':
        this.buildKind = ev.kind;
        this.status = 'building';
        this.progress = null;
        break;
      case 'progress':
        this.progress = { what: 'building', fraction: ev.fraction, label: ev.label, key: '', count: ev.total ? ['stage', ev.done + 1, ev.total] : null, elapsedS: 0, etaS: null, estimateS: null };
        break;
      case 'checked':
        this.#checked(ev.result);
        break;
      case 'built':
        this.#built(ev);
        break;
      case 'stopped':
        this.status = 'stopped';
        this.log.end();
        break;
      case 'failed':
        this.log.failed(ev.message, ev.stack);
        this.status = 'failed';
        this.progress = null;
        this.log.end();
        this.#fold(true);
        break;
      case 'idle':
        if (this.status !== 'failed' && this.status !== 'stopped') this.status = 'ready';
        this.progress = null;
        this.log.end();
        break;
      default:
        throw new Error(`a card was told "${ev.type}", which it does not know`);
    }
    this.#touch();
  }

  #checked(result) {
    this.checked = result;
    this.checkedText = this.text;
    this.log.checked(result);
    if (result.map) {
      this.map = result.map;
      this.marks = marksOf(result.map);
      const pins = parsePins(this.pinSpec, this.marks);
      this.moment.pinned = pins.ids;
      for (const e of pins.errors) this.log.failed(`${e}.`, '');
    }
    // the strip opens by itself only for a new error; when the last is fixed it folds again
    const errors = new Set(result.errors.map((d) => `${d.code}|${d.message}`));
    const fresh = [...errors].some((k) => !this.seenErrors.has(k));
    this.seenErrors = errors;
    if (fresh) this.#fold(true);
    else if (!errors.size && this.model.open.value && !this.opened) this.#fold(false);
    this.tileState.value = { ...this.tileState.value, status: result.ok ? 'building' : 'failed', errors: result.errors };
    this.#controls();
    this.#paint();
  }

  #built(ev) {
    const draft = ev.kind !== 'full';
    const meta = ev.meta;
    if (!meta.built) return;
    this.#showMesh(ev.positions, ev.colours, meta.footprint);
    if (!draft) { this.stl = ev.stl; this.fullAt = this.version; }
    this.log.built(meta, { draft, cached: ev.cached, ms: ev.ms });
    this.tileState.value = { ...this.tileState.value, status: 'ready', shown: { checked: !draft, draft }, record: { record: { seed: null } } };
    this.model.shown.value = !draft;
    this.draftNote = draft;
    this.host.dataset.detail = draft ? 'draft' : 'full';
    if (!draft && this.wantDownload) { this.wantDownload = false; if (this.stl?.byteLength) this.#save(); }
    this.#paint();
  }

  // ---- controls -------------------------------------------------------------------

  #controls() {
    if (!this.controls.length) return;
    this.controlsBox.textContent = '';
    for (const c of this.controls) {
      const at = findControl(this.map, c.name);
      if (!at) {
        const note = el('div', 'dtx-control missing');
        note.textContent = `${c.name}: not written in this source`;
        this.controlsBox.append(note);
        continue;
      }
      const read = readValue(at.text);
      if (!read) continue;
      const { min, max, step } = rangeOf(c, read.value);
      const row = el('label', 'dtx-control');
      const name = el('span', 'dtx-cname');
      name.textContent = c.name;
      const input = el('input', '', { type: 'range', min: String(min), max: String(max), step: String(step), value: String(read.value), 'aria-label': c.name });
      const out = el('output');
      out.textContent = `${read.value}${read.unit}`;
      let span = at.span, textNow = at.text;
      input.addEventListener('input', () => {
        const w = writeValue(this.text, { text: textNow, span }, Number(input.value));
        span = w.span; textNow = w.written;
        this.text = w.text;
        this.pane.set(w.text);
        out.textContent = `${Number(input.value)}${read.unit}`;
        this.#edited();
      });
      row.append(name, input, out);
      if (c.name === 'seed') {
        const roll = el('button', 'btn sm', { type: 'button', title: 'Draw another seed' });
        roll.textContent = 'Roll';
        roll.addEventListener('click', (e) => {
          e.preventDefault();
          input.value = String(1 + Math.floor(Math.random() * 998));
          input.dispatchEvent(new Event('input'));
        });
        row.append(roll);
      }
      this.controlsBox.append(row);
    }
  }

  // ---- status ------------------------------------------------------------------------

  #touch() {
    this.rev.value++;
    this.progressSig.value = this.progress;
    this.#status();
    this.#paint();
  }

  #status() {
    const view = this.log.view();
    const t = this.tileState.value;
    const counts = countsText(view.counts);
    const has = !!this.mesh;
    let words;
    if (this.status === 'queued') {
      words = { verdict: `queued · behind ${this.queuedBehind} other${this.queuedBehind === 1 ? '' : 's'}`, tone: 'idle', counts, progress: null };
    } else if (this.status === 'stopped') {
      words = { verdict: 'full detail stopped · press Full detail to resume', tone: 'idle', counts, progress: null };
    } else {
      words = statusOf(view, { t: { ...t, status: this.status === 'ready' && view.counts.errors ? 'failed' : t.status === 'failed' ? 'failed' : this.status === 'ready' ? 'ready' : this.status === 'building' ? 'building' : 'idle' }, p: this.status === 'building' ? this.progress : null });
      if (this.status === 'building' && !this.progress) words.verdict = this.mesh ? 'rebuilding · starting' : 'building · starting';
      if (this.status === 'ready' && t.shown && !t.shown.checked && !view.counts.errors) words.verdict = `draft detail · ${view.record?.build?.data?.cache === 'hit' ? 'cached' : `${(view.record?.build?.data?.build_s ?? 0).toFixed(2)} s`} · not yet through the gate`;
      if (view.counts.errors && has) words.counts = [counts, 'last good build shown'].filter(Boolean).join(' · ');
    }
    paintStatusLine(this.sl, { ...words, counts: words.counts ? `· ${words.counts}` : '', expanded: this.model.open.value });
    this.host.dataset.state = this.status;
    const busy = this.status === 'building' || this.status === 'queued';
    this.badge.dataset.kind = this.status;
    this.badge.textContent = this.status === 'queued' ? words.verdict : this.status === 'building' ? words.verdict.replace(/^(re)?building /, `Building at ${this.buildKind === 'full' ? 'full' : 'draft'} detail `) : '';
    this.view.dataset.busy = String(busy);
  }
}
