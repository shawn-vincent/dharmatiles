// The printer chooser: brand → model with a search, an AMS switch for a model that has an AMS variant, your own
// profiles in their own group, `verify` profiles marked unchecked, and an import from the slicers installed here.
// One widget for every place a printer is chosen: the title bar's target card and the plates panel.
//
// `PrinterChooser({printers, current, onPick, needBed, disabled, skey, scan, importFrom, say})`: the printers and the
// one chosen are the model's; what is typed in the search, and the import under way, are the chooser's own.
// `scan(slicer?)` and `importFrom(body)` are the program's calls (js/host.js), `say(label, reply)` shows a failure.

import { signal } from '@preact/signals-core';
import { fitWords, groupPrinters, selectionOf, squaresOf } from '../printers.js';
import { html, useEffect, useRef, useState } from './html.js';
import { Icon } from './icons.js';

/** @typedef {import('../printers.js').Printer} Printer */
/** @typedef {import('../api').SlicerFound} Slicer */
/** @typedef {import('../api').ScanJson} Scan  one slicer's models */
/** @typedef {Scan['models'][number]} SlicerModel */
/** @typedef {typeof import('../editor/app.js').host} Host */
/** @typedef {Awaited<ReturnType<Host['printersScan']>>} ScanReply */
/** @typedef {Awaited<ReturnType<Host['printersImport']>>} ImportReply */
/** @typedef {(label: string, reply: { status: number, body: any }) => unknown} Say  where a failed call is shown */
/** What the page gives the chooser (editor/printer-chooser.js adds the rest). @typedef {{ printers: Printer[], current: string, onPick: (name: string) => void, needBed?: boolean, disabled?: boolean, skey: string }} ChooserArgs */
/** @typedef {ChooserArgs & { square: number, scan: Host['printersScan'], importFrom: Host['printersImport'], say: Say, toast: (message: string) => void }} ChooserProps */
/** @typedef {{ slicer: string, label: string, title: string, models: SlicerModel[] | null, problems: string[], chosen: Set<string>, query: string, busy: boolean }} Import  the import under way */

const searches = new Map(); // what was typed, by chooser, so a chooser drawn again keeps it
const slicers = signal(/** @type {Slicer[] | null} */ (null)); // the slicers installed here, asked once
let asking = false;

/** @param {Host['printersScan']} scan @param {Say} say */
function askSlicers(scan, say) {
  if (slicers.peek() || asking) return;
  asking = true;
  scan().then((r) => {
    // no slicer, or a server that cannot say: the chooser simply has no import entry
    slicers.value = r.status === 200 && 'slicers' in r.body ? r.body.slicers : [];
  }).catch((e) => { slicers.value = []; say('Could not look for slicers', { status: 0, body: { error: String(e?.message || e) } }); })
    .finally(() => { asking = false; });
}

const own = (/** @type {any} */ e) => String(e?.message || e);

const BED_SCALE = 0.14; // px per mm, the same for every bed in the list: a bigger printer is a bigger drawing

/** The bed at scale with the squares that fit drawn in it. */
function Bed(/** @type {{ p: Printer, square: number }} */ { p, square }) {
  const n = squaresOf(p, square);
  if (!n) return html`<span class="pc-bed none" aria-hidden="true"></span>`;
  if (!p.bed) return null;
  const w = p.bed[0] * BED_SCALE, h = p.bed[1] * BED_SCALE, s = square * BED_SCALE;
  const cells = [];
  for (let i = 0; i < n[0]; i += 1) for (let j = 0; j < n[1]; j += 1) cells.push(html`<rect key=${`${i}-${j}`} x=${i * s + 0.5} y=${j * s + 0.5} width=${s - 1} height=${s - 1} rx="1" />`);
  return html`<svg class="pc-bed" width=${w} height=${h} viewBox=${`0 0 ${w} ${h}`} aria-hidden="true"><rect class="bedrect" x=".5" y=".5" width=${w - 1} height=${h - 1} rx="2" />${cells}</svg>`;
}

function Pick(/** @type {ChooserProps & { begin: (s: Slicer) => void }} */ { printers, current, onPick, needBed, disabled, skey, scan, begin, say, square }) {
  const [query, setQuery] = useState(searches.get(skey) || '');
  const list = useRef(/** @type {HTMLElement | null} */ (null));
  useEffect(() => { askSlicers(scan, say); }, []);
  const sel = selectionOf(printers, current);
  // the chosen model is in view when the list opens (the list scrolls itself, not the panel round it)
  useEffect(() => {
    const l = list.current;
    const on = /** @type {HTMLElement | null | undefined} */ (l?.querySelector('.pc-row.on')); // a row is an element
    if (l && on && !query) l.scrollTop = Math.max(0, on.offsetTop - l.clientHeight / 2 + on.offsetHeight / 2);
  }, [current]);
  const groups = groupPrinters(printers, { needBed, query });
  const cur = printers.find((p) => p.name === sel.model);
  const withAms = cur && printers.find((p) => p.name === `${cur.name}-ams`);
  return html`<div class="pc-pick">
    <input type="search" placeholder="Search printers" aria-label="Search printers" value=${query} onInput=${(/** @type {Event} */ e) => { const v = /** @type {HTMLInputElement} */ (e.target).value; searches.set(skey, v); setQuery(v); }} />
    <div class="pc-list" role="listbox" ref=${list}>
      ${groups.length ? null : html`<p class="pc-none">No printer matches.</p>`}
      ${groups.map((g) => html`<div class=${`cap pc-group ${g.kind}`} key=${`g${g.label}`}>${g.label}</div>${g.models.map((m) => html`
        <button type="button" key=${m.name} class=${`pc-row${m.name === sel.model ? ' on' : ''}`} disabled=${disabled || !!m.p.broken} role="option" aria-selected=${String(m.name === sel.model)} data-printer=${m.name}
          onClick=${() => onPick(sel.ams && m.ams ? m.ams.name : m.name)}>
          <${Bed} p=${m.p} square=${square} />
          <span class="pc-text"><span class="pc-top"><span class="pc-name">${m.title}</span>
            ${m.p.verify ? html`<span class="verify" title=${`Not yet checked against the maker: ${m.p.verify}`}>unchecked</span>` : null}
            ${m.p.fromSlicer ? html`<span class="own" title=${`From ${m.p.fromSlicer}`}>imported</span>` : null}</span>
          <span class="pc-meta">${m.p.broken ? `does not read: ${m.p.broken}` : `${fitWords(m.p, square)}${m.ams ? ' · AMS available' : ''}`}</span></span></button>`)}`)}
    </div>
    ${withAms ? html`<div class="seg pc-ams" role="group" aria-label="AMS">${[[cur.name, 'No AMS', 'One filament at a time'], [withAms.name, 'AMS', `${withAms.slots} filaments`]].map(([name, text, title]) => html`
      <button type="button" key=${name} title=${title} disabled=${disabled} aria-pressed=${String(name === current)} onClick=${() => { if (name !== current) onPick(name); }}>${text}</button>`)}</div>` : null}
    ${(slicers.value || []).length ? html`<details class="pc-foot"><summary class="btn pc-import-menu">Import from a slicer<${Icon} name="chevron-down" size=${14} /></summary>${(slicers.value || []).map((s) => html`<button type="button" class="btn pc-import" key=${s.slicer} title=${`Read ${s.label}'s machine profiles into your printers`} data-slicer=${s.slicer} onClick=${() => begin(s)}>${s.title}</button>`)}</details>` : null}
    <p class="pc-scale">Beds drawn at one scale; each square is ${`${square} mm`}.</p>
  </div>`;
}

function ImportView(/** @type {{ im: Import, setIm: (im: Import | null) => void, back: () => void, importFrom: Host['printersImport'], say: Say, toast: (message: string) => void }} */ { im, setIm, back, importFrom, say, toast }) {
  const words = im.query.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = (im.models ?? []).filter((m) => words.every((w) => `${m.brand} ${m.model} ${m.name}`.toLowerCase().includes(w)));
  const go = async () => {
    setIm({ ...im, busy: true });
    try {
      const r = await importFrom({ slicer: im.slicer, names: [...im.chosen] });
      if (r.status !== 200) { say('Could not import the printers', r); setIm({ ...im, busy: false }); return; }
      const done = r.body.written;
      const kept = done.filter((w) => w.kept).length;
      toast(`Imported ${done.length - kept} profile${done.length - kept === 1 ? '' : 's'}${kept ? `, ${kept} already yours` : ''}`);
      back();
    } catch (e) { say('Could not import the printers', { status: 0, body: { error: own(e) } }); setIm({ ...im, busy: false }); }
  };
  if (!im.models) return html`<div class="pc-importview"><div class="pc-ihead"><button type="button" class="btn" onClick=${back}><${Icon} name="chevron-left" size=${14} />Back</button><span class="pc-ititle">${`Import from ${im.label}`}</span></div><p class="pc-none">${`Reading ${im.title}'s profiles…`}</p></div>`;
  /** @type {string | null} */
  let brand = null;
  return html`<div class="pc-importview">
    <div class="pc-ihead"><button type="button" class="btn" onClick=${back}><${Icon} name="chevron-left" size=${14} />Back</button><span class="pc-ititle">${`Import from ${im.label}`}</span></div>
    <p class="note">Your own copies of the bed, height, nozzle, zones and filaments, in your printers folder. The slicer is not changed.</p>
    <input type="search" placeholder=${`Search ${im.models.length} models`} value=${im.query} aria-label="Search the slicer's models" onInput=${(/** @type {Event} */ e) => setIm({ ...im, query: /** @type {HTMLInputElement} */ (e.target).value })} />
    <div class="pc-list">
      ${shown.length ? null : html`<p class="pc-none">No model matches.</p>`}
      ${shown.map((m) => {
        const head = m.brand !== brand ? html`<div class="cap pc-group brand" key=${`b${m.brand}`}>${m.brand}</div>` : null;
        brand = m.brand;
        const meta = `${m.bed[0]} × ${m.bed[1]} mm${m.height ? ` · ${m.height} mm tall` : ''} · ${m.slots === 1 ? '1 filament' : `${m.slots} filaments`}${m.zones ? ` · ${m.zones} zone${m.zones === 1 ? '' : 's'} kept clear` : ''}`;
        return html`${head}<label class=${`pc-irow${m.have ? ' have' : ''}`} key=${m.name}>
          <input type="checkbox" checked=${im.chosen.has(m.name)} disabled=${m.have || im.busy} onChange=${(/** @type {Event} */ e) => { const c = new Set(im.chosen); if (/** @type {HTMLInputElement} */ (e.target).checked) c.add(m.name); else c.delete(m.name); setIm({ ...im, chosen: c }); }} />
          <span class="pc-itext"><span class="pc-top"><span class="pc-name">${m.model}</span>${m.have ? html`<span class="own">in yours</span>` : null}</span><span class="pc-meta">${meta}</span></span></label>`;
      })}
    </div>
    ${im.problems.length ? html`<details class="pc-problems"><summary>${`${im.problems.length} profile${im.problems.length === 1 ? '' : 's'} could not be used`}</summary>
      <ul>${im.problems.slice(0, 40).map((p) => html`<li>${p}</li>`)}${im.problems.length > 40 ? html`<li>${`… and ${im.problems.length - 40} more`}</li>` : null}</ul></details>` : null}
    <button type="button" class="btn on pc-go" disabled=${im.busy || !im.chosen.size} onClick=${go}>${im.busy ? 'Importing…' : im.chosen.size ? `Import ${im.chosen.size} model${im.chosen.size === 1 ? '' : 's'}` : 'Choose models to import'}</button>
  </div>`;
}

export function PrinterChooser(/** @type {ChooserProps} */ props) {
  const { scan, say } = props;
  const [im, setIm] = useState(/** @type {Import | null} */ (null));
  const begin = async (/** @type {Slicer} */ s) => {
    setIm({ slicer: s.slicer, label: s.label, title: s.title, models: null, problems: [], chosen: new Set(), query: '', busy: false });
    try {
      const r = await scan(s.slicer);
      if (r.status !== 200) { say(`Could not read ${s.title}`, r); setIm(null); return; }
      if (!('models' in r.body)) throw new Error('the scan of a slicer answered no models');
      const scanned = r.body;
      setIm((cur) => cur && { ...cur, models: scanned.models, problems: scanned.problems, label: scanned.label });
    } catch (e) { say(`Could not read ${s.title}`, { status: 0, body: { error: own(e) } }); setIm(null); }
  };
  return html`<div class="pc">${im
    ? html`<${ImportView} im=${im} setIm=${setIm} back=${() => setIm(null)} importFrom=${props.importFrom} say=${say} toast=${props.toast} />`
    : html`<${Pick} ...${props} begin=${begin} />`}</div>`;
}
