// The status line and the messages panel: two views of the one stream of
// messages (js/messages.js).  The status line is 26 px under the view: the
// verdict (worst first), the counts, what the text's save is doing and the
// build target, with a thin bar on its top edge while a build runs.  Press it
// or ⌃J and the panel rises over the lower part of the view: the running
// build, then the build's record and every open message as a row with its
// actions.  Neither owns a message: fix the cause and the row, the mark in
// the source and the count go with it.

import { setReporter, serverErrorIn } from '../problems.js';
import { copyButton } from '../copy.js';
import { FILTERS, detailsFor, filterRows, fitMsgHeight, statusOf } from '../messages.js';
import { $, host, viewer } from './app.js';
import { openFile, saveWords, texts } from './document.js';
import { showSourceLine } from './levels.js';
import { showWorst } from './selection.js';
import { doc, entrySel, log, progressOf, server, tile, view } from './stores.js';
import { fmt, secs } from './title.js';

let filter = 'all';
let focused = null; // the seq of the message a mark in the source pointed to, lit for a moment

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};
const tileId = () => tile()?.id ?? view.get().tileId ?? '';
const viewOf = () => log.view(tileId());

/** Show line `line` of `file` (null: the tile's own) in the source: the panel turns to it. */
async function showLine(file, line) {
  view.set({ page: 'edit' });
  await openFile(file, null);
  showSourceLine(line);
}

// ---- the status line ----------------------------------------------------

function renderBar() {
  const t = tile(), v = viewOf();
  const s = statusOf(v, { t, live: server.get().live, p: t ? progressOf(t.id) : null });
  $('sb-verdict').textContent = s.verdict;
  $('sb-counts').textContent = s.counts;
  $('sb-dot').className = s.tone;
  $('statusbar').dataset.tone = s.tone;
  $('sb-bar').style.width = s.progress == null ? '0' : `${Math.round(s.progress * 100)}%`;
  $('sb-bar').hidden = s.progress == null;
  const sv = server.get();
  $('sb-target').textContent = `${sv.system === 'ol' ? 'OpenLOCK' : 'DungeonBlocks'} · ${sv.printer}`;
  const w = saveWords();
  $('sb-saved').textContent = w && w.text !== 'saved' ? w.text : '';
  $('sb-saved').classList.toggle('bad', !!w?.bad);
  $('sb-main').setAttribute('aria-expanded', view.get().panel);
  $('statusbar').title = '';
}

// ---- the panel ----------------------------------------------------------

/** One number of the record: its name and value, as one cell of the grid. */
function pair(dl, k, v, cls = '') {
  const d = el('div', 'pair');
  const dt = el('dt', null, k), dd = el('dd', cls, v);
  d.append(dt, dd);
  dl.append(d);
}

/** The gate's numbers as rows under the record. */
function numbers(gate, t) {
  const a = gate?.data;
  const dl = el('dl', 'nums');
  if (!a) return dl;
  const { overhangDeg, steepestDeg } = server.get();
  const used = (area, budget) => (budget == null ? `${fmt(area)} mm²` : `${fmt(area)} / ${fmt(budget)} mm²`);
  pair(dl, 'Triangles', Number(a.triangles).toLocaleString());
  pair(dl, 'Watertight', a.watertight ? 'Yes' : 'No', a.watertight ? 'ok' : 'bad');
  pair(dl, `Band ${overhangDeg}–${steepestDeg}°`, used(a.band_mm2, a.band_budget_mm2), a.band_mm2 > (a.band_budget_mm2 ?? 0) ? 'bad' : a.band_mm2 > 0 ? 'warn' : 'ok');
  pair(dl, `Past ${steepestDeg}°`, used(a.overhang_mm2, a.overhang_budget_mm2), a.overhang_mm2 > (a.overhang_budget_mm2 ?? 0) ? 'bad' : a.overhang_mm2 > 0 ? 'warn' : 'ok');
  pair(dl, 'Dropped bodies', a.dropped, a.dropped ? 'bad' : '');
  pair(dl, 'Voids filled', a.voids_filled);
  pair(dl, 'Holes', a.holes, a.holes ? 'bad' : '');
  if (t?.record?.record?.seed != null) pair(dl, 'Seed', t.record.record.seed);
  return dl;
}

/** The build under way: stage, elapsed against the estimate, the bar.  Updated on every tick without redrawing the panel. */
function runningRow() {
  const t = tile();
  const box = el('div', 'msg run');
  box.id = 'msg-run';
  box.append(el('i', 'dot busy'));
  const body = el('div', 'body');
  body.append(el('div', 'line', `Building ${t?.name ?? ''}`), el('div', 'sub'), el('div', 'rbar'));
  body.lastChild.append(el('i'));
  box.append(body);
  return box;
}
function tickRunning() {
  const box = $('msg-run');
  const t = tile();
  if (!box || !t) return;
  const p = progressOf(t.id);
  const stage = !p ? 'starting' : !p.count ? p.label : p.count[2] === 0 ? `${p.label}: ${p.count[0]}` : `${p.label}: ${p.count[0]} ${p.count[1]}/${p.count[2]}`;
  box.querySelector('.sub').textContent = p ? `${stage} · ${secs(p.elapsed_s)} of ~${secs(p.estimate_s)}` : 'starting';
  box.querySelector('.rbar i').style.width = `${p ? Math.floor(p.fraction * 100) : 0}%`;
}

function recordRow(rec, t) {
  const box = el('div', `msg rec${rec.dim ? ' dim' : ''}`);
  const gate = rec.gate, fails = gate?.severity === 'error';
  box.append(el('i', `dot ${fails ? 'bad' : 'ok'}`));
  const body = el('div', 'body');
  // "Built in 2.1 s · passes the printability gate": the numbers below, and each reason the gate fails on its own line
  const verdict = gate?.text.split(':')[0].replace(/^(Passes|Fails)/, (w) => w.toLowerCase());
  const target = rec.build?.data?.target ? ` · for ${rec.build.data.target}` : '';
  body.append(el('div', 'line', `${[rec.build?.text, verdict].filter(Boolean).join(' · ')}${target}`));
  for (const f of gate?.data?.failures ?? []) body.append(el('div', 'fail', f));
  if (gate) body.append(numbers(gate, t));
  const acts = el('div', 'acts');
  if (gate && viewer.worstFlagged()) {
    const b = el('button', 'btn', 'Show worst face');
    b.type = 'button'; b.title = 'Take the camera to the largest face the gate painted';
    b.onclick = showWorst;
    acts.append(b);
  }
  const copy = copyButton(() => `${rec.build?.text ?? ''}\n${gate?.text ?? ''}\n${JSON.stringify(gate?.data ?? {}, null, 2)}`, { title: 'Copy the record' });
  acts.append(copy);
  body.append(acts);
  box.append(body);
  return box;
}

function messageRow({ m, dim }, t) {
  const box = el('div', `msg ${m.severity}${dim ? ' dim' : ''}${focused === m.seq ? ' lit' : ''}`);
  box.dataset.seq = m.seq;
  box.append(el('i', `dot ${m.severity}`));
  const body = el('div', 'body');
  const head = el('div', 'head');
  if (m.code) head.append(el('code', 'code', m.code));
  head.append(el('span', 'text', m.kind === 'server' ? `${m.data?.origin === 'page' ? 'The page: ' : ''}${m.text}` : m.text));
  body.append(head);
  if (m.help) body.append(el('div', 'help', m.help));
  const where = [];
  if (m.span) where.push(`${m.span.file.split('/').slice(-1)[0]}:${m.span.line}:${m.span.col}`);
  if (where.length) { const w = el('div', 'where', where[0]); w.title = m.span.file; body.append(w); }
  const acts = el('div', 'acts');
  if (m.span?.line) {
    const go = el('button', 'btn', 'Go to line');
    go.type = 'button'; go.title = 'Show the line in the source';
    go.onclick = () => showLine(m.span.file === t?.path ? null : m.span.file, m.span.line);
    acts.append(go);
  }
  if (m.node && t) {
    const show = el('button', 'btn', 'Show on tile');
    show.type = 'button'; show.title = 'Select the stack entry this is about';
    show.onclick = () => view.set({ sel: entrySel(m.node) });
    acts.append(show);
  }
  const source = m.span && m.span.file === doc.get().file ? doc.get().text ?? null : null;
  acts.append(copyButton(() => detailsFor(m, { source }), { title: 'Copy details: the code, where it is, its source line and this environment' }));
  if (m.kind === 'server') {
    const x = el('button', 'btn icon', '×');
    x.type = 'button'; x.title = 'Put this away'; x.setAttribute('aria-label', 'Put this message away');
    x.onclick = () => log.dismiss(m.seq);
    acts.append(x);
  }
  box.append(body, acts);
  return box;
}

function renderFilters(v) {
  const seg = $('msg-filters');
  seg.replaceChildren();
  const all = v.rows;
  for (const [id, label, keep] of FILTERS) {
    const n = id === 'all' ? all.length : all.filter((r) => keep(r.m)).length;
    const b = el('button', filter === id ? 'on' : '');
    b.type = 'button'; b.dataset.filter = id; b.setAttribute('aria-pressed', filter === id);
    b.append(label);
    if (n) b.append(' ', el('span', 'n', n));
    b.onclick = () => { filter = id; renderPanel(); };
    seg.append(b);
  }
}

function renderPanel() {
  const open = view.get().panel;
  $('msgpanel').hidden = !open;
  $('view').classList.toggle('msgs-open', open);
  if (!open) return;
  const t = tile(), v = viewOf();
  renderFilters(v);
  const body = $('msg-body');
  const keep = body.scrollTop;
  body.replaceChildren();
  if (v.running) {
    body.append(runningRow());
    tickRunning();
  } else if (v.record) body.append(recordRow(v.record, t));
  if (v.cleared.length) {
    const c = el('div', 'cleared', `${v.cleared.length} cleared by this build`);
    c.title = v.cleared.map((m) => `${m.code ?? m.kind}: ${m.text}`).join('\n');
    body.append(c);
  }
  const shown = filterRows(v.rows, filter);
  const fresh = shown.filter((r) => !r.dim), stale = shown.filter((r) => r.dim);
  for (const r of fresh) body.append(messageRow(r, t));
  if (stale.length || (v.running && v.record)) {
    body.append(el('div', 'from-last', 'From the last build · shown until the new one finishes'));
    if (v.running && v.record) body.append(recordRow(v.record, t));
    for (const r of stale) body.append(messageRow(r, t));
  }
  if (!shown.length && !v.record && !v.running) body.append(el('div', 'empty-msg', t ? 'Nothing has been said about this tile yet.' : 'Choose a tile.'));
  body.scrollTop = keep;
  const lit = body.querySelector('.lit');
  if (lit) { lit.scrollIntoView({ block: 'nearest' }); focused = null; setTimeout(() => lit.classList.remove('lit'), 1600); }
  // Copy all: the program's own text of the tile's open messages (one renderer), never the page's
  $('msg-copy').replaceChildren(copyButton(async () => (await host.messagesText(tile()?.id ?? '')) || 'No messages.', { label: 'Copy all', title: 'Copy every open message of this tile as text' }));
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
  log.subscribe(() => { renderBar(); renderPanel(); });
  server.subscribe(['tiles', 'live', 'system', 'printer', 'overhangDeg', 'steepestDeg'], () => { renderBar(); renderPanel(); });
  server.subscribe(['progress'], () => { renderBar(); tickRunning(); });
  view.subscribe(['tileId'], () => { renderBar(); renderPanel(); });
  view.subscribe(['panel'], () => { renderBar(); renderPanel(); });
  view.subscribe(['draft', 'shown'], renderPanel); // the worst face is known once the mesh is
  doc.subscribe(['base', 'writable', 'saving', 'note', 'conflict', 'ok', 'checked', 'text'], renderBar);
  // a mark in the source's gutter points to its message: open the panel on it
  texts.cb.onMarkClick = (line) => {
    const id = tileId();
    const hit = log.inFile(id, texts.file).find((m) => m.span.line === line);
    if (!hit) return;
    focused = hit.seq;
    if (!view.get().panel) togglePanel(true); else renderPanel();
  };
  addEventListener('keydown', (e) => {
    if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && e.code === 'KeyJ') { e.preventDefault(); togglePanel(); return; }
    if (e.key === 'Escape' && view.get().panel && !e.defaultPrevented && document.activeElement?.closest?.('#msgpanel')) { togglePanel(false); e.preventDefault(); }
  });
  renderBar();
}
