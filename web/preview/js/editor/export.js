// Export: the same build as `dharmatiles build`, written to out/.

import { failed, showError } from '../problems.js';
import { $, host } from './app.js';
import { dirty, doc, isInventory, isMapView, isPlates, renameable, server, stem, tile, view } from './stores.js';
import { chip as mapChip, sceneStore } from './scene.js';
import { targetLabel } from './target.js';

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
  $('export').hidden = !t || isPlates(t) || isInventory(t); // a print list exports from its own panel (plates.js)
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

/** The tile's name and size in the crumb and the title; the verdict, the progress and the record are the status line's and the messages panel's (panel.js). */
function renderTitle() {
  const t = tile();
  $('empty').hidden = !!t;
  renderExport();
  if (!t) {
    $('tilename').textContent = ''; $('tileplan').textContent = '';
    $('forward').hidden = true;
    document.title = 'dharmatiles';
    return;
  }
  $('tilename').textContent = stem(t);
  // the folder crumb (its name and tooltip) is the panel's: editor/levels.js
  $('forward').hidden = false; $('forward-name').textContent = stem(t);
  $('tilebtn').title = renameable(t) ? `${t.path}\nClick for the tile; double-click or F2 to rename` : `${t.path}\nClick for the tile`;
  document.title = `${stem(t)} · dharmatiles`;
  const p = t.plan;
  $('tileplan').textContent = isInventory(t) ? 'what you have printed' : isPlates(t) ? (isMapView() ? mapChip() : p?.plates?.label ?? '') : p ? `${p.cols}×${p.rows} · ${p.square_mm} mm squares` : '';
}

/** Wire this part of the page (once, at load). */
export function init() {
  server.subscribe(['tiles', 'roots', 'live', 'ever', 'system', 'printer'], renderTitle);
  view.subscribe(['tileId', 'dmode'], renderTitle);
  sceneStore.subscribe(['pieces'], renderTitle);
  doc.subscribe(['base', 'writable', 'saving'], renderTitle);
  doc.subscribe(['text'], renderExport); // the export button waits for a save
  renderTitle();
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
