// The title, the gate and the build record.

import { $, isEntry, state, tile, viewer } from './app.js';
import { loadText, markText } from './document.js';
import { renderTitle } from './export.js';
import { hovForView } from './selection.js';
import { renderList } from './tiles.js';

// ---- the title, the gate and the record ---------------------------------------

const secs = (s) => (s >= 60 ? `${Math.floor(s / 60)}m${String(Math.round(s % 60)).padStart(2, '0')}s` : `${s.toFixed(1)} s`);
/** The bar: percentage, the stage (with an element's own count), elapsed against the estimate. */
export function showProgress(p) {
  const pct = Math.floor(p.fraction * 100);
  $('bar-fill').style.width = `${pct}%`;
  $('progress').style.width = `${pct}%`;
  const stage = !p.count ? p.label
    : p.count[2] === 0 ? `${p.label}: ${p.count[0]}`
    : `${p.label}: ${p.count[0]} ${p.count[1]}/${p.count[2]}`;
  $('status-text').textContent = `${pct}% · ${stage} · ${secs(p.elapsed_s)} of ~${secs(p.estimate_s)}`;
}

export const fmt = (x, d = 2) => Number(x).toFixed(d);
export function row(dl, k, v, cls = '') {
  const dt = document.createElement('dt'); dt.textContent = k;
  const dd = document.createElement('dd'); dd.textContent = v; if (cls) dd.className = cls;
  dl.append(dt, dd);
}
export function renderAll() {
  renderList();
  renderTitle();
  viewer.setHighlight(isEntry(state.sel) ? state.sel : null, hovForView());
  const t = tile();
  if (t) loadText(state.textFor && state.textFor[0] === t.index && state.textFor[1] !== t.path ? state.textFor[1] : null);
  markText();
}
