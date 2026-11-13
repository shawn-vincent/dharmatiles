// The printer chooser: brand → model with a search, an AMS switch for a model that has an AMS variant, your own
// profiles in their own group, `verify` profiles marked unchecked, and an import from the slicers installed here.
// One widget for every place a printer is chosen: the title bar's target card and the plates panel.
//
// `PrinterChooser({printers, current, onPick, needBed, disabled, skey, scan, importFrom, say})`: the printers and the
// one chosen are the model's; what is typed in the search, and the import under way, are the chooser's own.
// `scan(slicer?)` and `importFrom(body)` are the program's calls (js/host.js), `say(label, reply)` shows a failure.

import { signal } from '@preact/signals-core';
import { groupPrinters, selectionOf, sizeWords } from '../printers.js';
import { html, useEffect, useRef, useState } from './html.js';
import { Icon } from './icons.js';

const searches = new Map(); // what was typed, by chooser, so a chooser drawn again keeps it
const slicers = signal(/** @type {any[] | null} */ (null)); // the slicers installed here, asked once
let asking = false;

function askSlicers(scan, say) {
  if (slicers.peek() || asking) return;
  asking = true;
  scan().then((r) => {
    // no slicer, or a server that cannot say: the chooser simply has no import entry
    slicers.value = r.status === 200 && 'slicers' in r.body ? r.body.slicers : [];
  }).catch((e) => { slicers.value = []; say('Could not look for slicers', { status: 0, body: { error: String(e?.message || e) } }); })
    .finally(() => { asking = false; });
}

const own = (e) => String(e?.message || e);

function Pick({ printers, current, onPick, needBed, disabled, skey, scan, begin, say }) {
  const [query, setQuery] = useState(searches.get(skey) || '');
  const list = useRef(/** @type {HTMLElement | null} */ (null));
  useEffect(() => { askSlicers(scan, say); }, []);
  const sel = selectionOf(printers, current);
  // the chosen model is in view when the list opens (the list scrolls itself, not the panel round it)
  useEffect(() => {
    const l = list.current;
    const on = l?.querySelector('.pc-row.on');
    if (l && on && !query) l.scrollTop = Math.max(0, on.offsetTop - l.clientHeight / 2 + on.offsetHeight / 2);
  }, [current]);
  const groups = groupPrinters(printers, { needBed, query });
  const cur = printers.find((p) => p.name === sel.model);
  const withAms = cur && printers.find((p) => p.name === `${cur.name}-ams`);
  return html`<div class="pc-pick">
    <input type="search" placeholder="Search printers" aria-label="Search printers" value=${query} onInput=${(e) => { searches.set(skey, e.target.value); setQuery(e.target.value); }} />
    <div class="pc-list" role="listbox" ref=${list}>
      ${groups.length ? null : html`<p class="pc-none">No printer matches.</p>`}
      ${groups.map((g) => html`<div class=${`cap pc-group ${g.kind}`} key=${`g${g.label}`}>${g.label}</div>${g.models.map((m) => html`
        <button type="button" key=${m.name} class=${`pc-row${m.name === sel.model ? ' on' : ''}`} disabled=${disabled || !!m.p.broken} role="option" aria-selected=${String(m.name === sel.model)} data-printer=${m.name}
          onClick=${() => onPick(sel.ams && m.ams ? m.ams.name : m.name)}>
          <span class="pc-top"><span class="pc-name">${m.title}</span>
            ${m.p.verify ? html`<span class="verify" title=${`Not yet checked against the maker: ${m.p.verify}`}>unchecked</span>` : null}
            ${m.p.fromSlicer ? html`<span class="own" title=${`From ${m.p.fromSlicer}`}>imported</span>` : null}</span>
          <span class="pc-meta">${m.p.broken ? `does not read: ${m.p.broken}` : `${sizeWords(m.p)}${m.ams ? ' · AMS available' : ''}`}</span></button>`)}`)}
    </div>
    ${withAms ? html`<div class="seg pc-ams" role="group" aria-label="AMS">${[[cur.name, 'No AMS', 'One filament at a time'], [withAms.name, 'AMS', `${withAms.slots} filaments`]].map(([name, text, title]) => html`
      <button type="button" key=${name} title=${title} disabled=${disabled} aria-pressed=${String(name === current)} onClick=${() => { if (name !== current) onPick(name); }}>${text}</button>`)}</div>` : null}
    <div class="pc-foot">${(slicers.value || []).map((s) => html`<button type="button" class="btn pc-import" key=${s.slicer} title=${`Read ${s.label}'s machine profiles into your printers`} data-slicer=${s.slicer} onClick=${() => begin(s)}>${`Import from ${s.title}…`}</button>`)}</div>
  </div>`;
}

function ImportView({ im, setIm, back, importFrom, say, toast }) {
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
  let brand = null;
  return html`<div class="pc-importview">
    <div class="pc-ihead"><button type="button" class="btn" onClick=${back}><${Icon} name="chevron-left" size=${14} />Back</button><span class="pc-ititle">${`Import from ${im.label}`}</span></div>
    <p class="note">Your own copies of the bed, height, nozzle, zones and filaments, in your printers folder. The slicer is not changed.</p>
    <input type="search" placeholder=${`Search ${im.models.length} models`} value=${im.query} aria-label="Search the slicer's models" onInput=${(e) => setIm({ ...im, query: e.target.value })} />
    <div class="pc-list">
      ${shown.length ? null : html`<p class="pc-none">No model matches.</p>`}
      ${shown.map((m) => {
        const head = m.brand !== brand ? html`<div class="cap pc-group brand" key=${`b${m.brand}`}>${m.brand}</div>` : null;
        brand = m.brand;
        const meta = `${m.bed[0]} × ${m.bed[1]} mm${m.height ? ` · ${m.height} mm tall` : ''} · ${m.slots === 1 ? '1 filament' : `${m.slots} filaments`}${m.zones ? ` · ${m.zones} zone${m.zones === 1 ? '' : 's'} kept clear` : ''}`;
        return html`${head}<label class=${`pc-irow${m.have ? ' have' : ''}`} key=${m.name}>
          <input type="checkbox" checked=${im.chosen.has(m.name)} disabled=${m.have || im.busy} onChange=${(e) => { const c = new Set(im.chosen); if (e.target.checked) c.add(m.name); else c.delete(m.name); setIm({ ...im, chosen: c }); }} />
          <span class="pc-itext"><span class="pc-top"><span class="pc-name">${m.model}</span>${m.have ? html`<span class="own">in yours</span>` : null}</span><span class="pc-meta">${meta}</span></span></label>`;
      })}
    </div>
    ${im.problems.length ? html`<details class="pc-problems"><summary>${`${im.problems.length} profile${im.problems.length === 1 ? '' : 's'} could not be used`}</summary>
      <ul>${im.problems.slice(0, 40).map((p) => html`<li>${p}</li>`)}${im.problems.length > 40 ? html`<li>${`… and ${im.problems.length - 40} more`}</li>` : null}</ul></details>` : null}
    <button type="button" class="btn on pc-go" disabled=${im.busy || !im.chosen.size} onClick=${go}>${im.busy ? 'Importing…' : im.chosen.size ? `Import ${im.chosen.size} model${im.chosen.size === 1 ? '' : 's'}` : 'Choose models to import'}</button>
  </div>`;
}

export function PrinterChooser(props) {
  const { scan, say } = props;
  const [im, setIm] = useState(/** @type {any} */ (null));
  const begin = async (s) => {
    setIm({ slicer: s.slicer, label: s.label, title: s.title, models: null, problems: [], chosen: new Set(), query: '', busy: false });
    try {
      const r = await scan(s.slicer);
      if (r.status !== 200) { say(`Could not read ${s.title}`, r); setIm(null); return; }
      if (!('models' in r.body)) throw new Error('the scan of a slicer answered no models');
      setIm((cur) => cur && { ...cur, models: r.body.models, problems: r.body.problems, label: r.body.label });
    } catch (e) { say(`Could not read ${s.title}`, { status: 0, body: { error: own(e) } }); setIm(null); }
  };
  return html`<div class="pc">${im
    ? html`<${ImportView} im=${im} setIm=${setIm} back=${() => setIm(null)} importFrom=${props.importFrom} say=${say} toast=${props.toast} />`
    : html`<${Pick} ...${props} begin=${begin} />`}</div>`;
}
