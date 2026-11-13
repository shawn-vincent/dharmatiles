// @ts-check
// Export: the same build as `dharmatiles build`, written to out/; and the title: the tile's name and size in the crumb and
// the title, the page's title, and the card's words.  Each is bound to what it says (an `effect` that reads it), so it
// follows the tile, the document and the target without a subscription to each.

import { effect, signal } from '@preact/signals-core';
import { failed, showError } from '../problems.js';
import { html, render } from '../view/html.js';
import { ExportWhy } from '../view/title-bar.js';
import { $, host } from './app.js';
import { dirty, doc, isInventory, isPlates, renameable, server, stem, tile } from './stores.js';
import { targetLabel } from './target.js';

// ---- export: the same build as `dharmatiles build`, written to out/ ----------

/**
 * What the export has done for the target `target`: it says what was written only while the target is the one it was
 * written for (a new target makes it no longer the answer), so nothing has to clear it.
 */
const exporting = signal({ busy: false, files: null, error: null, out: null, target: null });
const with3mf = signal(false); // the card's "Also a painted 3MF"
const set = (/** @type {any} */ patch) => { exporting.value = { ...exporting.value, ...patch }; };
const targetNow = () => targetLabel(server.get());

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

/** The card and its button, from the export's state and the tile. */
function showExport() {
  const t = tile();
  const x = exporting.value;
  const mine = x.target === targetNow();
  const files = mine ? x.files : null;
  $('export').hidden = !t || isPlates(t) || isInventory(t); // a print list exports from its own panel (plates.js)
  const why = x.busy ? null : exportBlock();
  $('export-target').textContent = `For ${targetNow()}: change it from the chip in the title bar.`;
  $('export-go').disabled = !!why || x.busy;
  $('export-go').textContent = x.busy ? 'Exporting…' : with3mf.value ? 'Export STL and 3MF' : 'Export STL';
  const p = $('export-why');
  p.classList.toggle('bad', !!(why || x.error));
  p.title = !x.busy && !x.error && !files ? (mine ? x.out?.out : '') || '' : '';
  render(html`<${ExportWhy} busy=${x.busy} error=${x.error} files=${files} why=${why} out=${mine ? x.out : null} />`, p);
  $('export-reveal').hidden = !files;
}

/** The tile's name in the crumb and the title (its size is the tile level's row); the verdict, the progress and the record are the status line's and the messages panel's (panel.js). */
function showTitle() {
  const t = tile();
  $('empty').hidden = !!t;
  if (!t) {
    $('tilename').textContent = '';
    $('forward').hidden = true;
    document.title = 'dharmatiles';
    return;
  }
  $('tilename').textContent = stem(t);
  // the folder crumb (its name and tooltip) is the panel's: editor/levels.js
  $('forward').hidden = false; $('forward-name').textContent = stem(t);
  $('tilebtn').title = renameable(t) ? `${t.path}\nClick for the tile; double-click or F2 to rename` : `${t.path}\nClick for the tile`;
  document.title = `${stem(t)} · dharmatiles`;
}

/** Wire this part of the page (once, at load). */
export function init() {
  try { with3mf.value = localStorage.getItem('dt-export-3mf') === '1'; } catch { /* private mode: the default stands */ }
  $('export-3mf').checked = with3mf.value;
  effect(showTitle);
  effect(showExport);
  $('export-3mf').onchange = (/** @type {any} */ e) => { try { localStorage.setItem('dt-export-3mf', e.target.checked ? '1' : '0'); } catch { /* private mode: the default stands */ } with3mf.value = e.target.checked; set({ files: null }); };
  $('export').addEventListener('click', async () => {
    set({ files: null, error: null, out: null, target: targetNow() });
    const t = tile();
    if (!t) return;
    try {
      const r = await host.exportTile(t.id, { where: true });
      if (r.status === 200 && t.id === tile()?.id) set({ out: r.body, target: targetNow() });
      else failed(`Could not find where ${stem(t)} would be written`, r);
    } catch (e) { showError(`Could not find where ${stem(t)} would be written`, e); }
  });
  $('export-go').onclick = async () => {
    const t = tile();
    if (!t || exportBlock()) return;
    set({ busy: true, error: null, files: null, target: targetNow() });
    try {
      const r = await host.exportTile(t.id, { threemf: with3mf.value });
      if (r.status !== 200) {
        set({ error: r.body?.error || `could not export (${r.status})` });
        if (r.status !== 0) failed(`Could not export ${stem(t)}`, r);
      } else if ('files' in r.body) {
        // written; a tile that fails the printability gate is written too, and says so
        set({ files: r.body.files, ...(r.body.ok ? {} : { error: r.body.failures.length ? `Fails the gate: ${r.body.failures[0]}` : 'the build failed' }) });
      }
    } catch (e) {
      set({ error: 'the export failed' });
      showError(`Could not export ${stem(t)}`, e);
    }
    set({ busy: false });
  };
  $('export-reveal').onclick = async () => {
    const f = exporting.value.files?.find((/** @type {string} */ x) => /\.(stl|3mf)$/.test(x));
    if (!f) return;
    try { failed('Could not show the file', await host.reveal(f)); } catch (e) { showError('Could not show the file', e); }
  };
}
