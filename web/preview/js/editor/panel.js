// @ts-check
// The status line and the messages panel: two views of the one stream of
// messages (js/messages.js).  The status line is 26 px under the view: the
// verdict (worst first), the counts, what the text's save is doing and the
// build target, with a thin bar on its top edge while a build runs.  Press it
// or ⌃J and the panel rises over the lower part of the view: the running
// build, then the build's record and every open message as a row with its
// actions.  Neither owns a message: fix the cause and the row, the mark in
// the source and the count go with it.

import { computed, effect, signal, untracked } from '@preact/signals-core';
import { setReporter, serverErrorIn } from '../problems.js';
import { detailsFor, fitMsgHeight, statusOf } from '../messages.js';
import { html, render } from '../view/html.js';
import { CopyAll, ErrorCard, MessageFilters, MessagesBody } from '../view/messages-panel.js';
import { reveal } from '../slide.js';
import { $, host, viewer } from './app.js';
import { openFile, saveWords, texts } from './document.js';
import { showSourceLine } from './levels.js';
import { showWorst } from './selection.js';
import { doc, entrySel, isMapView, log, progressOf, server, tile, view } from './stores.js';
import { sceneNow, sceneStore, statusLines } from './scene.js';
import { docs } from './docs.js';

const tileId = () => tile()?.id ?? view.get().tileId ?? '';
const viewOf = () => log.view(tileId());

/** What the panel reads: the stream's view of the tile shown, which filter is on, and which message a mark in the source pointed to (lit for a moment). */
const filter = signal('all');
const lit = signal(/** @type {number | null} */ (null));
const model = {
  open: computed(() => !!view.get().panel),
  view: computed(viewOf),
  filter,
  lit,
  clearLit: (/** @type {number} */ seq) => { if (lit.value === seq) lit.value = null; },
  tile: computed(() => tile()),
  progress: computed(() => { const t = tile(); return t ? progressOf(t.id) : null; }),
  angles: computed(() => ({ overhangDeg: server.get().overhangDeg, steepestDeg: server.get().steepestDeg })),
  shown: computed(() => view.get().shown),
  file: computed(() => doc.get().file),
  text: computed(() => doc.get().text),
};

/** Show line `line` of `file` (null: the tile's own) in the source: the panel turns to it. */
export async function showLine(file, line) {
  view.set({ pageAt: 'edit' });
  await openFile(file, null);
  showSourceLine(line);
}

// ---- the status line ----------------------------------------------------

function showBar() {
  const t = tile(), v = viewOf();
  const s = statusOf(v, { t, live: server.get().live, p: t ? progressOf(t.id) : null });
  // on a map the line says what the shelf says of it, and that what is drawn is not yet checked
  const map = isMapView() && sceneNow() ? statusLines() : null;
  // the plates of a map may not lay out (a tile needs more filaments than the printer has): that is the plates' to say
  const platesOnly = (t?.errors ?? []).every((e) => /^dt::(slots|plates|inventory|printer)$/.test(e.code));
  const calm = map && (s.tone === 'ok' || s.tone === 'note' || (s.tone === 'bad' && platesOnly));
  $('sb-verdict').textContent = calm ? map.verdict : s.verdict;
  // "draft · not yet checked" once, whoever knows it: the tile's shown build or the map's pieces
  const draft = s.unchecked || map?.notChecked ? 'draft · not yet checked' : '';
  $('sb-counts').textContent = [draft, s.counts, map?.counts].filter(Boolean).join(' · ');
  // a map whose plates cannot be laid out is not a map in error: its dot is the notes' yellow, or green
  const tone = calm && s.tone === 'bad' ? (map.notes ? 'note' : 'ok') : s.tone;
  $('sb-dot').className = `dot ${tone}`;
  $('statusbar').dataset.tone = tone;
  $('sb-bar').style.width = s.progress == null ? '0' : `${Math.round(s.progress * 100)}%`;
  $('sb-bar').hidden = s.progress == null;
  const w = saveWords();
  $('sb-saved').textContent = w && w.text !== 'saved' ? w.text : '';
  $('sb-saved').classList.toggle('bad', !!w?.bad);
  $('sb-main').setAttribute('aria-expanded', view.get().panel);
  $('statusbar').title = '';
}

// ---- the panel ----------------------------------------------------------

/** What the panel's buttons do. */
const act = {
  setFilter: (/** @type {string} */ id) => { filter.value = id; },
  showLine,
  openPanel: () => togglePanel(true),
  showNode: (/** @type {string} */ node) => view.set({ sel: entrySel(node) }),
  dismiss: (/** @type {number} */ seq) => log.dismiss(seq),
  details: (/** @type {any} */ m, /** @type {string | null} */ source) => detailsFor(m, { source }),
  canShowWorst: () => !!viewer.worstFlagged(),
  showWorst,
  allText: async () => (await host.messagesText(tile()?.id ?? '')) || 'No messages.',
};

/** The panel: open or shut, with its filters, rows and Copy all drawn from the model. */
function bindPanel() {
  let first = true;
  effect(() => {
    const open = view.get().panel;
    untracked(() => showMessages(open, !first));
    first = false;
    $('view').classList.toggle('msgs-open', open);
    $('msgbtn').setAttribute('aria-pressed', String(open));
    $('msgbtn').classList.toggle('on', open);
  });
  render(html`<${MessageFilters} m=${model} act=${act} />`, $('msg-filters'));
  render(html`<${MessagesBody} m=${model} act=${act} />`, $('msg-body'));
  render(html`<${CopyAll} act=${act} />`, $('msg-copy'));
  render(html`<${ErrorCard} m=${model} act=${act} />`, $('doc-error'));
}

/**
 * Show or hide the panel with slide.js's one reveal: its height grows from nothing (or back), and --msgh, which the view's canvas
 * and the display menu take their room from, follows it frame by frame (the observer in `msgSash`).
 */
function showMessages(open, animate) {
  const p = $('msgpanel');
  if (open) {
    p.hidden = false;
    reveal({ target: p, property: 'height', from: '0px', to: `${p.offsetHeight}px`, animate });
  } else if (!p.hidden) {
    reveal({ target: p, property: 'height', from: `${p.offsetHeight}px`, to: '0px', animate, done: () => { p.hidden = true; } });
  }
}

/** Open or close the panel. */
export function togglePanel(open = !view.get().panel) {
  view.set({ panel: open });
}

// ---- failures of the page's own join the stream ---------------------------

/**
 * A failure the page met (problems.js) is told to the program, which puts it in the stream beside its
 * own failures (kind server, origin page), so the panel shows it and an assistant reads it.  A failure
 * the program itself reported (a 5xx, a build that failed) is already there.  If the program cannot be
 * told (it is away) the page keeps the message itself, so it is never only on the console.
 */
async function report(what, err, entry) {
  const se = serverErrorIn(err);
  if (se && (se.status == null || se.status >= 500)) return; // the program's own words: already in the stream
  const page = entry.chain.find((l) => l.side === 'page');
  const body = {
    tile: tileId(), what, text: entry.detail ?? entry.message, help: entry.help ?? null,
    causes: entry.chain.slice(1).map((l) => l.message), backtrace: (page?.trace ?? []).join('\n'),
  };
  try {
    const r = await host.reportMessage(body);
    if (r.status === 200) return;
  } catch { /* the program cannot be told: kept here below, with the reason it is the page's */ }
  log.addLocal({ tile: body.tile, text: body.text, help: body.help, data: { origin: 'page', kind: 'bug', route: what, causes: body.causes, backtrace: body.backtrace } });
}

/** The messages panel's top edge is a sash: drag it up or down, double-click to reset.  The height is remembered in this browser (dt-msgh). */
function msgSash() {
  const p = $('msgpanel');
  const s = document.createElement('div');
  s.className = 'sash';
  s.id = 'msg-sash';
  s.setAttribute('role', 'separator');
  s.setAttribute('aria-orientation', 'horizontal');
  s.title = 'Drag to resize; double-click to reset';
  p.prepend(s);
  const room = () => p.offsetParent?.clientHeight ?? innerHeight;
  const setH = (h, save) => {
    if (h == null) p.style.height = '';
    else p.style.height = `${fitMsgHeight(h, room() - 26)}px`;
    if (!save) return;
    try {
      if (h == null) localStorage.removeItem('dt-msgh'); else localStorage.setItem('dt-msgh', String(parseInt(p.style.height, 10)));
    } catch { /* private mode: the height is not kept */ }
  };
  // the messages take their room from the 3D view (not over it): the view's canvas ends above them
  new ResizeObserver(() => $('view').style.setProperty('--msgh', `${p.hidden ? 0 : p.offsetHeight}px`)).observe(p);
  let kept = null;
  try { kept = Number(localStorage.getItem('dt-msgh')) || null; } catch { /* private mode, or nothing kept */ }
  if (kept) setH(kept, false);
  addEventListener('resize', () => { if (p.style.height) setH(parseInt(p.style.height, 10), false); });
  s.addEventListener('dblclick', () => setH(null, true));
  s.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    s.setPointerCapture(e.pointerId);
    s.classList.add('on');
    document.body.classList.add('resizing-rows');
    const bottom = p.getBoundingClientRect().bottom;
    const move = (ev) => setH(bottom - ev.clientY, false);
    const up = () => {
      s.classList.remove('on');
      document.body.classList.remove('resizing-rows');
      s.removeEventListener('pointermove', move);
      setH(parseInt(p.style.height, 10) || null, true);
    };
    s.addEventListener('pointermove', move);
    s.addEventListener('pointerup', up, { once: true });
    s.addEventListener('pointercancel', up, { once: true });
  });
}

/** Wire this part of the page (once, at load). */
export function init() {
  msgSash();
  setReporter((what, err, entry) => { report(what, err, entry).catch((e) => console.error('could not report a failure to the stream', e)); });
  $('sb-main').onclick = () => togglePanel();
  $('msg-close').onclick = () => togglePanel(false);
  $('msgbtn').onclick = () => togglePanel();
  effect(showBar);
  bindPanel();
  // a mark in the source's gutter points to its message: open the panel on it
  texts.cb.onMarkClick = (line) => {
    const id = tileId();
    const hit = log.inFile(id, texts.file).find((m) => m.span.line === line);
    if (!hit) return;
    lit.value = hit.seq;
    if (!view.get().panel) togglePanel(true);
  };
  addEventListener('keydown', (e) => {
    if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && e.code === 'KeyJ') { e.preventDefault(); togglePanel(); return; }
    if (e.key === 'Escape' && view.get().panel && !e.defaultPrevented && document.activeElement?.closest?.('#msgpanel')) { togglePanel(false); e.preventDefault(); }
  });
}
