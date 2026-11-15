// @ts-check
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

/** @typedef {import('../../../web/preview/js/api').TileMap} TileMap (the repo's path; the site serves the page's code at `../../editor/js`, where api.d.ts is not published, so `tsc` reads it from here) */
/** @typedef {import('../../../web/preview/js/api').BuildData} BuildData */
/** @typedef {import('./engine-wire').Checked} Checked */
/** @typedef {import('./engine.js').CardEvent} CardEvent */
/** @typedef {import('./engine.js').Built} Built */
/** @typedef {import('../../../web/preview/js/api').MessageJson} MessageJson */

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
import { messageOf } from '../../editor/js/problems.js';
import { CardLog } from './site-log.js';
import { SourcePane } from './source-pane.js';
import { findControl, parseControls, rangeOf, readValue, writeValue } from './controls.js';
import { LiteViewer } from './viewer-lite.js';
import { costWords, deviceVerdict } from './limits.js';

const TOGGLES = [['lines', 'Lines'], ['points', 'Points'], ['regions', 'Regions'], ['squares', 'Squares']];
let serial = 0;
/** The cards that hold a WebGL context, least recently used first: a page keeps only a few live. */
/** @type {Card[]} */
const holders = [];
const MAX_CONTEXTS = 4;

/**
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag @param {string} [cls] @param {Record<string, string>} [attrs]
 * @returns {HTMLElementTagNameMap[K]}
 */
function el(tag, cls = '', attrs = {}) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
}

export class Card {
  // the parts the constructor makes, named here so their types hold everywhere
  /** @type {HTMLElement} */ view;
  /** @type {HTMLElement | null} */ poster;
  /** @type {HTMLCanvasElement} */ gl;
  /** @type {HTMLCanvasElement} */ ov;
  /** @type {HTMLElement} */ badge;
  /** @type {HTMLElement} */ msgs;
  /** @type {HTMLElement} */ controlsBox;
  /** @type {HTMLElement} */ sourceBox;
  /** @type {SourcePane} */ pane;
  /** @type {HTMLAnchorElement} */ openLink;
  /** @type {import('../../../web/preview/js/view/status-line.js').StatusParts} */ sl;
  /** @type {import('@preact/signals-core').Signal<number>} */ rev;
  /** @type {import('@preact/signals-core').Signal<any>} */ tileState;
  /** @type {import('@preact/signals-core').Signal<any>} */ progressSig;
  /** @type {any} the signals the messages list draws from (`#model`) */ model;
  /** @type {LiteViewer | null} */ viewer = null;

  /** @param {HTMLElement} host the <dt-example> @param {import('./engine.js').Engine} engine */
  constructor(host, engine) {
    this.host = host;
    this.engine = engine;
    this.id = `card-${++serial}`;
    this.name = /** @type {string} */ ((host.getAttribute('src') ?? 'tile').split('/').pop()).replace(/\.dtile$/, ''); // split always yields a piece
    this.system = host.getAttribute('system') ?? 'db';
    setExplainBase(new URL('e/', engine.root).href);

    const code = host.querySelector('.dtx-code code');
    this.original = (code?.textContent ?? '').replace(/\n$/, '\n');
    this.text = this.original;
    this.version = 0;
    /** @type {Checked | null} the worker's check (dt-web's `Checked`, generated into engine-wire.d.ts) */
    this.checked = null;
    this.checkedText = null;
    /** @type {ReturnType<typeof marksOf>} */
    this.marks = [];
    /** @type {TileMap | null} the program's map, one type for the page, the cards and the posters */
    this.map = null;
    /** @type {number | null} mm per square of the map shown: the check's that made the map, kept with it */
    this.sq = null;
    this.mesh = null;
    this.heights = null;
    this.fullAt = null;
    this.wantDownload = false;
    this.status = 'waking';
    /** @type {any} the worker's progress event, as it last said */
    this.progress = null;
    this.queuedBehind = 0;
    this.log = new CardLog(this.id, () => engine.version);
    this.moment = {
      toggles: new Set((host.getAttribute('overlay') ?? '').split(/[\s,]+/).filter(Boolean)),
      pinned: new Set(),
      hover: /** @type {string | null} */ (null),
      caret: /** @type {string | null} */ (null),
    };
    this.pinSpec = host.getAttribute('highlight') ?? '';
    this.controls = parseControls(host.getAttribute('controls') ?? '');
    this.opened = false;
    this.seenErrors = new Set();
    this.debounce = 0;

    this.rev = signal(0);
    this.tileState = signal({ id: this.id, name: this.name, status: 'idle', path: 'tile.dtile', shown: /** @type {{ checked: boolean, draft: boolean } | null} */ (null), errors: [] });
    this.progressSig = signal(/** @type {any} */ (null));

    // the page: every part named above is made here, so the checker sees each one assigned
    host.dataset.state = 'waking';
    const grid = host.querySelector('.dtx-grid') ?? host.appendChild(el('div', 'dtx-grid'));
    this.view = /** @type {HTMLElement} */ (grid.querySelector('.dtx-view') ?? grid.appendChild(el('figure', 'dtx-view')));
    this.poster = this.view.querySelector('.dtx-poster');
    host.querySelector('.dtx-wake')?.remove();
    // the card draws its own marks now: the ones on the poster are put away
    host.querySelector('.dtx-pre')?.remove();
    this.gl = el('canvas', 'dtx-gl', { 'aria-label': 'The tile, in 3D: drag to turn it (on a touch screen, press Turn it first)', role: 'img', tabindex: '0' });
    this.ov = el('canvas', 'dtx-ov', { 'aria-hidden': 'true' });
    this.badge = el('div', 'dtx-badge', { role: 'status' });
    this.view.append(this.gl, this.ov, this.badge);
    // a touch screen scrolls the page under a finger on a card and turns the tile only when asked (Interact): one finger is the page's
    // before it is the card's, and the card's own gesture (turning) is chosen with a button, not by counting fingers, which a browser's
    // pinch-zoom and a thumb resting on the glass both get wrong.  A mouse or trackpad turns it by dragging, as ever.
    this.coarse = matchMedia('(pointer: coarse)').matches;
    host.dataset.pointer = this.coarse ? 'coarse' : 'fine';
    this.interact = false;
    if (this.coarse) {
      this.interactBtn = el('button', 'btn sm dtx-interact', { type: 'button', 'aria-pressed': 'false', title: 'Turn the tile with one finger (the page stops scrolling under it)' });
      this.interactBtn.textContent = 'Turn it';
      this.interactBtn.addEventListener('click', () => this.#interact(!this.interact));
      this.view.append(this.interactBtn);
    }

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
    // when this device cannot carry a full build, the card says so here (never hiding the cost) and points to the poster and the program
    this.limitNote = el('p', 'dtx-limit');
    this.limitNote.hidden = true;

    // the status line, and the messages it folds
    this.sl = buildStatusLine({ onToggle: () => this.#fold(!this.model.open.value), controls: `${this.id}-msgs` });
    this.msgs = el('div', 'dtx-msgs msglist', { id: `${this.id}-msgs`, role: 'region', 'aria-label': 'Messages' });
    this.msgs.hidden = true;
    this.model = this.#model();
    render(html`<${MessagesBody} m=${this.model} act=${this.#act()} />`, this.msgs);

    side.append(tg, this.controlsBox, this.sourceBox, actions, this.limitNote, this.sl.root, this.msgs);
    grid.append(side);
    host.querySelector('.dtx-code')?.remove();

    this.#sizeObserver();
    this.#pointer();
  
    this.handle = engine.register(this.id, (ev) => this.#engine(ev));
    this.handle.focus();
    this.#edit();
  }

  // ---- the page ----------------------------------------------------------

  /** @param {string} label @param {string} title @param {() => void} on */
  #button(label, title, on) {
    const b = el('button', 'btn sm', { type: 'button', title });
    b.textContent = label;
    b.addEventListener('click', on);
    return b;
  }

  /** The model the editor's messages list draws from: signals over this card's own log. */
  #model() {
    return {
      open: signal(false),
      view: computed(() => { void this.rev.value; return this.log.view(); }),
      filter: signal('all'),
      lit: signal(null),
      clearLit: () => {},
      tile: computed(() => this.tileState.value),
      progress: computed(() => this.progressSig.value),
      angles: signal({ overhangDeg: 55, steepestDeg: 65 }),
      shown: signal(/** @type {boolean | null} */ (null)),
      file: signal('tile.dtile'),
      text: signal(this.text),
    };
  }

  #act() {
    return {
      setFilter: (/** @type {string} */ id) => { this.model.filter.value = id; },
      showLine: (/** @type {string | null} */ _file, /** @type {number} */ line) => this.pane.showLine(line),
      openPanel: () => this.#fold(true),
      showNode: () => {},
      dismiss: (/** @type {number} */ seq) => { this.log.log.dismiss(seq); this.#touch(); },
      details: (/** @type {MessageJson} */ m, /** @type {string | null} */ source) => detailsFor(m, { source }),
      canShowWorst: () => false,
      underside: () => null,
      showWorst: () => {},
      applyFix: (/** @type {MessageJson} */ m) => this.#applyFix(m),
      allText: async () => this.model.view.value.rows.map((/** @type {{ m: MessageJson }} */ r) => `${r.m.severity}: ${r.m.code ?? ''} ${r.m.text}`).join('\n') || 'No messages.',
    };
  }

  /** The source with the program's fix laid in, by the engine (`Op::Fix`): the edit goes through the source like any other. */
  /** @param {MessageJson} m */
  async #applyFix(m) {
    if (!m.fix) return;
    const was = this.text;
    let next;
    try {
      next = await this.engine.fix(was, m.fix);
    } catch (e) {
      this.log.failed(`The fix could not be made: ${messageOf(e)}`, e instanceof Error ? e.stack ?? '' : '');
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
  /** @param {boolean} open */
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
    const viewer = this.viewer;
    if (viewer && this.mesh) return (/** @type {number} */ x, /** @type {number} */ y, z = 0) => viewer.project(x, y, z);
    const foot = this.mesh?.footprint ?? this.#mapFootprint();
    if (!foot) return null;
    const [fw, fh] = foot;
    const { w, h } = this.size ?? { w: 1, h: 1 };
    const s = Math.min(w / fw, h / fh);
    const ox = (w - fw * s) / 2, oy = (h - fh * s) / 2;
    return (/** @type {number} */ x, /** @type {number} */ y) => ({ x: ox + x * s, y: oy + (fh - y) * s });
  }

  /** The footprint the check's map says, mm; none before a check has made a map. */
  #mapFootprint() {
    if (!this.map || this.sq == null) return null;
    return [this.map.cols * this.sq, this.map.rows * this.sq];
  }

  #zAt = (/** @type {number} */ x, /** @type {number} */ y) => (this.viewer && this.heights ? heightAtGrid(this.heights, x, y, 4) : 0);

  #paint() {
    if (!this.ov || !this.size) return;
    const dpr = this.ov.width / (this.size.w || 1);
    const ctx = this.ov.getContext('2d');
    if (!ctx) throw new Error('This browser gave no 2D canvas for the overlay.');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const moment = { toggles: this.moment.toggles, pinned: this.moment.pinned, hover: this.moment.hover, caret: this.moment.caret };
    this.drawnNow = drawn(this.marks, moment);
    const squares = this.moment.toggles.has('squares');
    // nothing is drawn over a tile no check has mapped: there is no square size to draw it at
    const project = this.#projector();
    if (this.sq == null || !project) return void this.#markSource();
    renderMarks(ctx, {
      map: this.map, sq: this.sq, drawn: this.drawnNow, squares,
      project, zAt: this.#zAt, size: this.size,
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
    const over = (/** @type {PointerEvent} */ e) => {
      if (e.buttons) return;
      const r = this.gl.getBoundingClientRect();
      const x = e.clientX - r.left, y = e.clientY - r.top;
      const project = this.#projector();
      if (this.sq == null || !project) return;
      const o = { map: this.map, sq: this.sq, drawn: this.drawnNow ?? [], project, zAt: this.#zAt };
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

  /** @param {Built['positions']} positions @param {Built['colours']} colours @param {Built['meta']['footprint']} footprint */
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
    this.viewer.setInteractive(this.interact || !this.coarse);
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
    this.#stlState();
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
    // only the page's own text is kept between visits (an edited one is the reader's and is not worth keeping)
    this.handle.edit({ text: this.text, system: this.system, seed: null, cells: DRAFT_CELLS, tier: DRAFT_TIER, version: this.version, persist: this.text === this.original });
    this.#touch();
  }

  /** @param {boolean} download */
  #full(download) {
    if (this.#device()?.fits === false) return; // the button says why it is off; a stray call does nothing
    this.wantDownload = download;
    this.status = 'building';
    this.log.begin();
    this.tileState.value = { ...this.tileState.value, status: 'building' };
    this.handle.focus();
    this.handle.full({ text: this.text, system: this.system, seed: null, version: this.version, cpuSecs: this.checked?.fullCost?.cpuSecs });
    this.#touch();
  }

  /** Whether this device can carry a full build of the checked tile: null before the check says what it costs. */
  #device() {
    const cost = this.checked?.fullCost;
    if (!cost) return null;
    // boundary: `deviceMemory` is in no DOM lib; it is used for the default and the words only
    const memory = /** @type {{deviceMemory?: number}} */ (navigator).deviceMemory;
    return deviceVerdict(cost, { deviceMemory: memory, coarse: this.coarse, smallScreen: Math.min(screen.width, screen.height) <= 820 });
  }

  /** The Full detail button says what it costs, and when this device cannot carry that, says so and turns off. */
  #fullState() {
    const cost = this.checked?.fullCost;
    const verdict = this.#device();
    this.fullBtn.textContent = cost ? `Full detail · ${costWords(cost)}` : 'Full detail';
    const off = verdict?.fits === false;
    this.fullBtn.disabled = off;
    this.fullBtn.title = off ? 'Too big for this device' : 'Build what the printer is given (slower, and it needs the memory above)';
    this.limitNote.hidden = !off;
    if (off && cost) {
      this.limitNote.textContent = '';
      const where = new URL('index.html#install', this.engine.root).href;
      const a = el('a', '', { href: where });
      a.textContent = 'the program on a desktop';
      this.limitNote.append(`A full build needs ${costWords(cost)}, and ${verdict.why}. The picture is the draft the page makes (the poster's tile); `, a, ' makes the full tile and its STL.');
    }
    this.#stlState();
  }

  /** @param {boolean} on */
  #interact(on) {
    this.interact = on;
    this.host.dataset.interact = String(on);
    this.interactBtn?.setAttribute('aria-pressed', String(on));
    this.viewer?.setInteractive(on || !this.coarse);
  }

  #download() {
    if (this.fullAt !== this.version) return this.#full(true);
    // a full build of this text is shown: it has its STL, or it failed the gate and has none (the button says so)
    if (this.stl) this.#save();
  }

  /**
   * The Download button as the text's last full build leaves it: a build that fails the printability gate makes no STL
   * (dt-web writes none for a failed build), so the button is off and says why, until the text changes.
   */
  #stlState() {
    // the button is made with the card's controls, after the first edits a page's script may make
    const b = this.stlBtn;
    if (!b) return;
    const refused = this.fullAt === this.version && !this.stl;
    const tooBig = this.#device()?.fits === false;
    b.disabled = refused || tooBig;
    b.title = tooBig ? 'Too big for this device to build: the program on a desktop makes the STL' : refused
      ? 'This tile fails the printability gate, so no STL is made: the messages say what to change'
      : 'The STL the printer gets: always built at full detail';
  }

  #save() {
    const stl = this.stl;
    if (!stl) throw new Error('A card was asked to save an STL it does not hold.'); // the callers check first
    const url = URL.createObjectURL(new Blob([new Uint8Array(stl)], { type: 'model/stl' }));
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

  /** @param {CardEvent} ev */
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
        this.progress = { what: 'building', fraction: ev.fraction, label: ev.label, key: '', count: ev.total ? /** @type {[string, number, number]} */ (['stage', ev.done + 1, ev.total]) : null, elapsedS: 0, etaS: null, estimateS: null };
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
        throw new Error(`a card was told "${/** @type {{ type: string }} */ (ev).type}", which it does not know`); // every known type is handled above
    }
    this.#touch();
  }

  /** @param {Checked} result */
  #checked(result) {
    this.checked = result;
    this.checkedText = this.text;
    this.log.checked(result);
    if (result.map) {
      this.map = result.map;
      this.sq = result.squareMm;
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
    this.#fullState();
    this.#paint();
  }

  /** @param {Extract<CardEvent, { type: 'built' }>} ev */
  #built(ev) {
    const draft = ev.kind !== 'full';
    const meta = ev.meta;
    if (!meta.built) return;
    this.#showMesh(ev.positions, ev.colours, meta.footprint);
    if (!draft) { this.stl = ev.stl.byteLength ? ev.stl : null; this.fullAt = this.version; this.#stlState(); }
    this.log.built(meta, { draft, cached: ev.cached, ms: ev.ms });
    this.tileState.value = { ...this.tileState.value, status: 'ready', shown: { checked: !draft, draft } };
    this.model.shown.value = !draft;
    this.draftNote = draft;
    this.host.dataset.detail = draft ? 'draft' : 'full';
    if (!draft && this.wantDownload) { this.wantDownload = false; if (this.stl) this.#save(); }
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
    // the build message's data is the program's `BuildData` (site-log.js makes it so)
    const built = /** @type {BuildData | undefined} */ (view.record?.build?.data);
    let words;
    if (this.status === 'queued') {
      words = { verdict: `queued · behind ${this.queuedBehind} other${this.queuedBehind === 1 ? '' : 's'}`, tone: 'idle', counts, progress: null };
    } else if (this.status === 'stopped') {
      words = { verdict: 'full detail stopped · press Full detail to resume', tone: 'idle', counts, progress: null };
    } else {
      words = statusOf(view, { t: { ...t, status: this.status === 'ready' && view.counts.errors ? 'failed' : t.status === 'failed' ? 'failed' : this.status === 'ready' ? 'ready' : this.status === 'building' ? 'building' : 'idle' }, p: this.status === 'building' ? this.progress : null });
      if (this.status === 'building' && !this.progress) words.verdict = this.mesh ? 'rebuilding · starting' : 'building · starting';
      if (this.status === 'ready' && t.shown && !t.shown.checked && !view.counts.errors) words.verdict = `draft detail · ${built?.cache === 'hit' ? 'cached' : `${(built?.build_s ?? 0).toFixed(2)} s`} · not yet through the gate`;
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
