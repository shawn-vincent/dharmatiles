// Export: the same build as `dharmatiles build`, written to out/.

import { $, host, state, tile, viewer } from './app.js';
import { browsing, showBrowser } from './chrome.js';
import { dirty, doc, drafts, loadText, texts } from './document.js';
import { renameable, rootOf, stem } from './tiles.js';
import { fmt, row, showProgress } from './title.js';

let timer = 0;

// ---- export: the same build as `dharmatiles build`, written to out/ ----------

const exporting = { system: null, busy: false, files: null, error: null, out: null };
/** Why export is not possible now, or null. */
function exportBlock() {
  const t = tile();
  if (!t) return 'Choose a tile first.';
  if (t.status === 'building' || t.status === 'idle') return 'Wait for the build to finish.';
  if (doc.writable && dirty()) return 'Save your changes first: export builds the file as saved.';
  if (!t.record) return 'The tile has no good build yet: fix the mistakes first.';
  if (t.record.audit.failures.length) return `It fails the printability gate: ${t.record.audit.failures[0]}. Fix the geometry first.`;
  return null;
}
function renderExport() {
  const t = tile();
  $('export').hidden = !t;
  const why = exporting.busy ? null : exportBlock();
  const sys = exporting.system || state.system || 'db';
  for (const b of $('export-system').children) b.classList.toggle('on', b.dataset.sys === sys);
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

export function renderTitle() {
  const t = tile();
  $('empty').hidden = !!t;
  const status = $('status'), errs = $('errors'), dl = $('record'), gate = $('gate');
  dl.replaceChildren();
  $('failures').replaceChildren();
  clearInterval(timer);
  $('verdict').hidden = true;
  $('show-worst').hidden = true;
  // waiting has one look, whether for a build or for the server: the chip's
  // pulsing dot and the status strip (and in the view, the faded tile)
  const away = !state.live;
  const awayLabel = state.ever ? 'reconnecting…' : 'connecting…';
  if (!t) {
    status.hidden = true; errs.hidden = true; $('export').hidden = true;
    gate.hidden = !away;
    if (away) { gate.className = 'busy'; $('gatetext').textContent = awayLabel; }
    $('tilename').textContent = ''; $('tilewhere').textContent = 'Tiles'; $('tileplan').textContent = '';
    $('forward').hidden = true;
    if (!browsing()) showBrowser();
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
  $('tileplan').textContent = p ? `${p.cols}×${p.rows} · ${p.square_mm} mm squares · ${state.system === 'ol' ? 'OpenLOCK' : 'DungeonBlocks'}` : '';
  // building…
  const building = t.status === 'building' || t.status === 'idle';
  const busy = building || away, busyLabel = away ? awayLabel : 'building';
  if (away) {
    status.hidden = false;
    $('bar-fill').style.width = '0%';
    $('status-text').textContent = 'Waiting for dharmatiles preview…';
  } else if (building) {
    if (!state.buildStart.has(t.index)) state.buildStart.set(t.index, performance.now());
    // before the first progress event there is only the clock
    const tick = () => {
      if (t.progress) return showProgress(t.progress);
      $('bar-fill').style.width = '0%';
      $('status-text').textContent = t.status === 'idle'
        ? 'Waiting to build…'
        : `Building… ${((performance.now() - state.buildStart.get(t.index)) / 1000).toFixed(1)} s`;
    };
    tick(); timer = setInterval(tick, 100);
    status.hidden = false;
  } else {
    status.hidden = true;
    $('progress').style.width = '0';
    state.buildStart.delete(t.index);
  }
  // errors: file:line:col, the old mesh stays behind them; a click shows the line
  errs.hidden = !t.errors.length;
  errs.replaceChildren();
  for (const e of t.errors) {
    const d = document.createElement('div'); d.className = 'e';
    const w = document.createElement('div'); w.className = 'where';
    w.textContent = e.file ? `${e.file}:${e.line}:${e.column}  ${e.code}` : e.code;
    const m = document.createElement('div'); m.textContent = e.message;
    d.append(w, m);
    if (e.help) { const h = document.createElement('div'); h.className = 'help'; h.textContent = e.help; d.append(h); }
    if (e.file && e.line) d.onclick = () => loadText(e.file === t.path ? null : e.file).then(() => texts.reveal(e.line));
    errs.append(d);
  }
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
  if (drafts.shown) {
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
      ? `Inspect largest face past ${state.steepestDeg}°`
      : `Inspect largest face at ${state.overhangDeg}–${state.steepestDeg}°`;
  }
  row(dl, 'Triangles', a.triangles.toLocaleString());
  row(dl, 'Watertight', a.watertight ? 'Yes' : 'No', a.watertight ? 'ok' : 'bad');
  const lo = state.overhangDeg, hi = state.steepestDeg;
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
  try { exporting.system = localStorage.getItem('dt-export-system'); } catch { /* ignore */ }
  try { $('export-3mf').checked = localStorage.getItem('dt-export-3mf') === '1'; } catch { /* ignore */ }
  for (const b of $('export-system').children) {
    b.onclick = () => { exporting.system = b.dataset.sys; exporting.files = null; try { localStorage.setItem('dt-export-system', b.dataset.sys); } catch { /* ignore */ } renderExport(); };
  }
  $('export-3mf').onchange = (e) => { try { localStorage.setItem('dt-export-3mf', e.target.checked ? '1' : '0'); } catch { /* ignore */ } exporting.files = null; renderExport(); };
  $('export').addEventListener('click', async () => {
    exporting.files = null; exporting.error = null; exporting.out = null; renderExport();
    const t = tile();
    if (!t) return;
    try {
      const r = await host.post(`/api/tiles/${t.index}/export`, { where: true });
      if (r.status === 200 && t === tile()) { exporting.out = r.body; renderExport(); }
    } catch { /* said when exporting */ }
  });
  $('export-go').onclick = async () => {
    const t = tile();
    if (!t || exportBlock()) return renderExport();
    exporting.busy = true; exporting.error = null; exporting.files = null; renderExport();
    try {
      const r = await host.post(`/api/tiles/${t.index}/export`, { system: exporting.system || state.system || 'db', threemf: $('export-3mf').checked });
      if (r.status !== 200) exporting.error = r.body?.error || `could not export (${r.status})`;
      else if (!r.body.ok) exporting.error = r.body.errors?.[0]?.message || (r.body.failures?.length ? `Fails the gate: ${r.body.failures[0]}` : 'the build failed');
      else exporting.files = r.body.files || [];
      if (r.status === 200 && r.body.files && !r.body.ok) exporting.files = r.body.files;
    } catch { exporting.error = 'cannot export while the editor is disconnected'; }
    exporting.busy = false;
    renderExport();
  };
  $('export-reveal').onclick = () => { const f = exporting.files?.find((x) => /\.(stl|3mf)$/.test(x)); if (f) host.post('/api/reveal', { path: f }); };
}
