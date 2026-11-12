// Export: the same build as `dharmatiles build`, written to out/.

import { dismiss, failed, onProblems, problems, showError } from '../problems.js';
import { $, host, viewer } from './app.js';
import { openFile } from './document.js';
import { dirty, doc, progressOf, renameable, rootOf, server, stem, tile, view } from './stores.js';
import { fmt, row, showProgress } from './title.js';
import { targetLabel } from './target.js';
import { copyButton } from '../copy.js';

let timer = 0;
const buildStart = new Map(); // tile id -> when its build was first seen under way

// ---- export: the same build as `dharmatiles build`, written to out/ ----------

const exporting = { busy: false, files: null, error: null, out: null };
/** Why export is not possible now, or null. */
function exportBlock() {
  const t = tile();
  if (!t) return 'Choose a tile first.';
  if (t.status === 'building' || t.status === 'idle') return 'Wait for the build to finish.';
  if (doc.get().writable && dirty()) return 'Save your changes first: export builds the file as saved.';
  if (!t.record) return 'The tile has no good build yet: fix the mistakes first.';
  if (t.record.audit.failures.length) return `It fails the printability gate: ${t.record.audit.failures[0]}. Fix the geometry first.`;
  return null;
}
function renderExport() {
  const t = tile();
  $('export').hidden = !t;
  const why = exporting.busy ? null : exportBlock();
  $('export-target').textContent = `For ${targetLabel(server.get())}: change it from the chip in the title bar.`;
  $('export-go').disabled = !!why || exporting.busy;
  $('export-go').textContent = exporting.busy ? 'Exporting…' : $('export-3mf').checked ? 'Export STL and 3MF' : 'Export STL';
  const p = $('export-why');
  p.classList.toggle('bad', !!(why || exporting.error));
  p.replaceChildren();
  if (exporting.busy) p.textContent = 'Building through the gate…';
  else if (exporting.error) p.textContent = exporting.error;
  else if (exporting.files) {
    p.append('Written: ');
    exporting.files.filter((f) => /\.(stl|3mf)$/.test(f)).forEach((f, i) => {
      if (i) p.append(', ');
      const c = document.createElement('code');
      c.textContent = f.split(/[\\/]/).slice(-3).join('/'); c.title = f; // out/db/name-db.stl; the whole path on hover
      p.append(c);
    });
  } else {
    p.textContent = why || (exporting.out ? `Writes to ${exporting.out.shown}` : 'Writes to out/, as dharmatiles build does.');
    p.title = exporting.out?.out || '';
  }
  $('export-reveal').hidden = !exporting.files;
}

/**
 * The error panel: the tile's own (file:line:col; the old mesh stays behind
 * them, a click shows the line), then the failures the program or the page
 * hit that are not a mistake in the tile (the standard: errors.html), each
 * with what was being done, the words, its details to copy, and a way to
 * dismiss it.
 */
function drawErrors(t) {
  const errs = $('errors'), own = t?.errors || [], bugs = problems();
  errs.hidden = !own.length && !bugs.length;
  errs.replaceChildren();
  const line = (cls, text) => { const x = document.createElement('div'); if (cls) x.className = cls; x.textContent = text; return x; };
  // a build's failure (a panic) is in the list below with its stacks: its
  // one-line "bug" here would say it twice
  for (const e of own) {
    if (e.unexpected && t.failure) continue;
    const d = document.createElement('div'); d.className = e.unexpected ? 'e bug' : 'e';
    d.append(line('where', e.file ? `${e.file}:${e.line}:${e.column}  ${e.code}` : e.code));
    d.append(line('', e.unexpected ? `A bug, not a mistake in your tile: ${e.message}` : e.message));
    if (e.help) d.append(line('help', e.help));
    if (e.file && e.line) d.onclick = () => openFile(e.file === t.path ? null : e.file, e.line);
    errs.append(d);
  }
  for (const p of bugs) errs.append(problemEntry(p, line));
}

/**
 * One failure in the panel (problems.js says it): what was being done, the
 * headline (the program's own words for a failure or a refusal; "something
 * went wrong inside dharmatiles" for a bug), and a disclosure into the whole
 * chain: the page's error and its stack, then the server's with its causes and
 * backtrace, each level indented, long traces scrolling.  "Copy details"
 * copies the report (what, the chain with both stacks, the environment).
 */
function problemEntry(p, line) {
  const el = (tag, cls, text) => { const x = document.createElement(tag); if (cls) x.className = cls; if (text != null) x.textContent = text; return x; };
  const d = el('div', `e problem ${p.kind}`);
  d.append(line('where', p.count > 1 ? `${p.what} (${p.count} times)` : p.what));
  d.append(el('div', 'headline', p.headline));
  if (p.detail) d.append(el('div', 'detail', p.detail));
  if (p.help) d.append(el('div', 'help', p.help));
  const more = el('details', 'chain');
  more.open = p.open;
  more.addEventListener('toggle', () => { p.open = more.open; });
  more.append(el('summary', null, 'Details'));
  p.chain.forEach((l, i) => {
    const lvl = el('div', `lvl ${l.side}`);
    lvl.style.setProperty('--depth', i);
    lvl.append(el('div', 'lvl-title', l.title), el('div', 'lvl-msg', l.message));
    if (l.help) lvl.append(el('div', 'lvl-line', `help: ${l.help}`));
    if (l.context) lvl.append(el('div', 'lvl-line', `while: ${l.context}`));
    if (l.causes.length) {
      lvl.append(el('div', 'lvl-line', 'caused by:'));
      const ul = el('ul', 'lvl-causes');
      for (const c of l.causes) ul.append(el('li', null, c));
      lvl.append(ul);
    }
    if (l.trace.length) {
      const pre = el('pre', 'trace', l.trace.join('\n'));
      pre.tabIndex = 0; // a long trace scrolls: the keyboard can reach it
      pre.setAttribute('aria-label', l.side === 'server' ? 'Server backtrace' : 'Page stack');
      lvl.append(pre);
    }
    more.append(lvl);
  });
  const acts = el('div', 'acts');
  const close = el('button', 'btn', 'Dismiss'); close.type = 'button';
  close.onclick = () => dismiss(p);
  const note = el('span', 'copy-note'); note.setAttribute('role', 'status');
  acts.append(copyButton(p.details, { label: 'Copy details', title: 'Copy details: the error, both stacks and this environment, to paste into a bug report', note }), close, note);
  // the buttons above the chain: with the chain open they stay in reach
  d.append(acts, more);
  return d;
}

function renderTitle() {
  const t = tile(), { live, ever, overhangDeg, steepestDeg } = server.get();
  $('empty').hidden = !!t;
  const status = $('status'), dl = $('record'), gate = $('gate');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  $('verdict').hidden = true;
  $('show-worst').hidden = true;
  // waiting has one look, whether for a build or for the server: the chip's
  // pulsing dot and the status strip (and in the view, the faded tile)
  const away = !live;
  const awayLabel = ever ? 'reconnecting…' : 'connecting…';
  if (!t) {
    status.hidden = true; $('export').hidden = true;
    drawErrors(null);
    gate.hidden = !away;
    if (away) { gate.className = 'busy'; $('gatetext').textContent = awayLabel; }
    $('tilename').textContent = ''; $('tilewhere').textContent = 'Tiles'; $('tileplan').textContent = '';
    $('forward').hidden = true;
    $('progress').style.width = '0';
    document.title = 'dharmatiles';
    return;
  }
  $('tilename').textContent = stem(t);
  $('tilewhere').textContent = rootOf(t.root).label || 'Tiles';
  $('back').title = `All of ${rootOf(t.root).label || 'the tiles'}`;
  $('forward').hidden = false; $('forward-name').textContent = stem(t);
  $('tilebtn').title = renameable(t) ? `${t.path}\nClick for the tile's properties; double-click or F2 to rename` : `${t.path}\nClick for the tile's properties`;
  document.title = `${stem(t)} · dharmatiles`;
  const p = t.plan;
  $('tileplan').textContent = p ? `${p.cols}×${p.rows} · ${p.square_mm} mm squares` : '';
  // building…
  const building = t.status === 'building' || t.status === 'idle';
  const busy = building || away, busyLabel = away ? awayLabel : 'building';
  if (away) {
    status.hidden = false;
    $('bar-fill').style.width = '0%';
    $('status-text').textContent = 'Waiting for dharmatiles preview…';
  } else if (building) {
    if (!buildStart.has(t.id)) buildStart.set(t.id, performance.now());
    // before the first progress event there is only the clock
    const tick = () => {
      const p = progressOf(t.id);
      if (p) return showProgress(p);
      $('bar-fill').style.width = '0%';
      $('status-text').textContent = t.status === 'idle'
        ? 'Waiting to build…'
        : `Building… ${((performance.now() - buildStart.get(t.id)) / 1000).toFixed(1)} s`;
    };
    tick(); timer = setInterval(tick, 100);
    status.hidden = false;
  } else {
    status.hidden = true;
    $('progress').style.width = '0';
    buildStart.delete(t.id);
  }
  drawErrors(t);
  const r = t.record;
  gate.hidden = false;
  renderExport();
  gate.className = busy ? 'busy' : '';
  if (!r) {
    $('gatetext').textContent = busy ? busyLabel : t.status === 'failed' ? 'no build' : '–';
    if (t.status === 'failed') gate.className = 'fail';
    row(dl, 'State', { idle: 'Not built yet', building: 'Building', failed: 'Failed', ready: 'Ready' }[t.status] || t.status);
    return;
  }
  const a = r.audit;
  const stale = t.status === 'failed' ? ' (last good build)' : '';
  const fails = a.failures.length > 0;
  if (!busy) gate.className = fails ? 'fail' : (a.band_mm2 > 0 || a.overhang_mm2 > 0) ? 'warn' : '';
  $('gatetext').textContent = busy ? busyLabel : fails ? 'fails the gate' : 'passes';
  if (view.get().draft) {
    const tag = document.createElement('span');
    tag.className = 'tag'; tag.textContent = 'draft';
    tag.title = 'The view shows a quick draft; the verdict is the last full build\u2019s';
    $('gatetext').append(' ', tag);
  }
  const v = $('verdict');
  v.hidden = false;
  v.className = fails ? 'fail' : 'pass';
  v.textContent = fails ? `Fails the printability gate${stale}` : `Passes the printability gate${stale}`;
  const worst = viewer.worstFlagged();
  if (worst) {
    const show = $('show-worst');
    show.hidden = false;
    show.textContent = worst.kind === 'red'
      ? `Inspect largest face past ${steepestDeg}°`
      : `Inspect largest face at ${overhangDeg}–${steepestDeg}°`;
  }
  row(dl, 'Triangles', a.triangles.toLocaleString());
  row(dl, 'Watertight', a.watertight ? 'Yes' : 'No', a.watertight ? 'ok' : 'bad');
  const lo = overhangDeg, hi = steepestDeg;
  const used = (area, budget) => budget == null ? `${fmt(area)} mm²` : `${fmt(area)} / ${fmt(budget)} mm²`;
  row(dl, `Band ${lo}–${hi}° (orange)`, used(a.band_mm2, a.band_budget_mm2),
      a.band_mm2 > (a.band_budget_mm2 ?? 0) ? 'bad' : a.band_mm2 > 0 ? 'warn' : 'ok');
  row(dl, `Past ${hi}° (red)`, used(a.overhang_mm2, a.overhang_budget_mm2),
      a.overhang_mm2 > (a.overhang_budget_mm2 ?? 0) ? 'bad' : a.overhang_mm2 > 0 ? 'warn' : 'ok');
  const dropped = Array.isArray(a.dropped) ? a.dropped.length : a.dropped;
  row(dl, 'Dropped bodies', dropped, dropped ? 'bad' : '');
  row(dl, 'Voids filled', a.voids_filled);
  row(dl, 'Holes', a.holes, a.holes ? 'bad' : '');
  row(dl, 'Build time', `${fmt(r.build_s)} s`);
  if (r.record.seed != null) row(dl, 'Seed', r.record.seed);
  for (const f of a.failures) {
    const d = document.createElement('div'); d.className = 'f'; d.textContent = f;
    $('failures').append(d);
  }
}

/** Wire this part of the page (once, at load). */
export function init() {
  server.subscribe(['tiles', 'roots', 'live', 'ever', 'system', 'printer', 'overhangDeg', 'steepestDeg'], renderTitle);
  view.subscribe(['tileId', 'draft', 'shown'], renderTitle);
  doc.subscribe(['base', 'writable', 'saving'], renderTitle);
  doc.subscribe(['text'], renderExport); // the export button waits for a save
  server.subscribe(['progress'], () => {
    const t = tile(), p = t && progressOf(t.id);
    if (p && t.status === 'building') showProgress(p);
  });
  renderTitle();
  onProblems(() => drawErrors(tile()));
  try { $('export-3mf').checked = localStorage.getItem('dt-export-3mf') === '1'; } catch { /* private mode: the default stands */ }
  // a new target makes what was written the target before no longer the answer
  server.subscribe(['system', 'printer'], () => { exporting.files = null; renderExport(); });
  $('export-3mf').onchange = (e) => { try { localStorage.setItem('dt-export-3mf', e.target.checked ? '1' : '0'); } catch { /* private mode: the default stands */ } exporting.files = null; renderExport(); };
  $('export').addEventListener('click', async () => {
    exporting.files = null; exporting.error = null; exporting.out = null; renderExport();
    const t = tile();
    if (!t) return;
    try {
      const r = await host.exportTile(t.id, { where: true });
      if (r.status === 200 && t.id === tile()?.id) { exporting.out = r.body; renderExport(); }
      else failed(`Could not find where ${stem(t)} would be written`, r);
    } catch (e) { showError(`Could not find where ${stem(t)} would be written`, e); }
  });
  $('export-go').onclick = async () => {
    const t = tile();
    if (!t || exportBlock()) return renderExport();
    exporting.busy = true; exporting.error = null; exporting.files = null; renderExport();
    try {
      const r = await host.exportTile(t.id, { threemf: $('export-3mf').checked });
      if (r.status !== 200) {
        exporting.error = r.body?.error || `could not export (${r.status})`;
        if (r.status !== 0) failed(`Could not export ${stem(t)}`, r);
      }
      else if (!r.body.ok) exporting.error = r.body.errors?.[0]?.message || (r.body.failures?.length ? `Fails the gate: ${r.body.failures[0]}` : 'the build failed');
      else exporting.files = r.body.files || [];
      if (r.status === 200 && r.body.files && !r.body.ok) exporting.files = r.body.files;
    } catch (e) {
      exporting.error = 'the export failed';
      showError(`Could not export ${stem(t)}`, e);
    }
    exporting.busy = false;
    renderExport();
  };
  $('export-reveal').onclick = async () => {
    const f = exporting.files?.find((x) => /\.(stl|3mf)$/.test(x));
    if (!f) return;
    try { failed('Could not show the file', await host.reveal(f)); } catch (e) { showError('Could not show the file', e); }
  };
}
