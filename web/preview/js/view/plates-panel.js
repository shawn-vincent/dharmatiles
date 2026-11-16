// @ts-check
// The plates panel (docs/plates.html): a print list opens on the printer's bed; this panel holds the settings that
// decide the layout, the plates with what is on each, the pins, the list's own pieces, what the inventory says, the
// sheet after a print and Export.  A setting is a word of the list's `plates` line (act.set writes it).
//
// `PlatesPanel({m, act})` is a function of the model (js/editor/model.js `platesPanel`: signals) and of the actions
// (what the buttons do).  What the user is in the middle of (the sheet after a print, the export under way) is in
// the model's `ui`, keyed by the list; a field keeps only its draft (field.js).  Nothing is drawn by signature: a new
// state from the program is drawn over the panel as it is.

import { exists, nameOf, plateTotals, systemName, wantLine } from '../inventory.js';
import { Field } from './field.js';
import { html } from './html.js';
import { Icon } from './icons.js';
import { Thumb } from '../thumbs.js';
import { Button, Note, Seg, Section, plural } from './widgets.js';

/** @typedef {typeof import('../editor/model.js').platesPanel} Model  what the panel reads: signals */
/** @typedef {typeof import('../editor/plates.js').act} Act  what its buttons do */
/** @typedef {import('../api').PlatesPanel} Panel  what the program says of the list: planned, written but not planned, or unread */
/** @typedef {import('../api').PlatesPlanned} Planned */
/** @typedef {import('../api').PlanPlate} Plate */
/** @typedef {{ m: Model, act: Act }} PanelProps */
/** @typedef {PanelProps & { p: Panel, ro: boolean }} SectionProps */
/** @typedef {PanelProps & { p: Planned }} PlannedProps */

const mm = (/** @type {number} */ n) => `${Number(n).toFixed(1).replace(/\.0$/, '')}`;

const Printer = (/** @type {SectionProps} */ { m, p, ro, act }) => {
  // only a list that was read says its printer's bed; an unread one has the printers to choose from and nothing else
  const bed = 'bed' in p ? p.bed : null;
  const slots = 'slots' in p ? p.slots : null;
  const excluded = 'excluded' in p ? p.excluded : [];
  const verify = 'verify' in p ? p.verify : null;
  return html`<${Section} title="Printer">
  ${act.chooser({ printers: m.printers.value, current: m.currentPrinter.value, onPick: (/** @type {string} */ name) => act.set('printer', name), needBed: true, disabled: ro, skey: 'plates' })}
  ${bed && slots != null ? html`<p class="note">${`${mm(bed[0])} × ${mm(bed[1])} mm bed${excluded.length ? `, ${plural(excluded.length, 'zone')} kept clear` : ''} · ${plural(slots, 'filament slot')}`}</p>` : null}
  ${verify ? html`<p class="note verify">${`Not yet checked against the maker: ${verify}`}</p>` : null}
<//>`;
};

const ColourSection = (/** @type {Omit<SectionProps, 'm'>} */ { p, ro, act }) => {
  const sc = p.planned ? p.slotCheck : null;
  const colour = 'colour' in p ? p.colour : null;
  const group = 'group' in p ? p.group : null;
  return html`<${Section} title="Colour and grouping">
    <${Seg} label="Colour" options=${[['single', 'Single colour', 'One filament, no paint'], ['multicolour', 'Multicolour', 'Each material in its own filament']]} current=${colour} on=${(/** @type {string | null} */ v) => act.set('colour', v)} disabled=${ro} />
    ${colour === 'multicolour' ? html`<${Seg} label="Group" options=${[['filament', 'By filament', 'Pieces that share filaments go together, then large to small'], ['size', 'By size', 'Large to small, whatever the filaments']]} current=${group} on=${(/** @type {string | null} */ v) => act.set('group', v)} disabled=${ro} />` : null}
    ${colour === 'multicolour' && sc ? html`<${Note} cls=${sc.ok ? 'ok' : 'bad'}>${sc.ok ? `The most any piece needs is ${plural(sc.need, 'filament')}; the printer has ${sc.have}.` : `A piece needs ${sc.need} filaments; the printer has ${sc.have}. Print in single colour or choose a printer with more.`}<//>` : null}
  <//>`;
};

const NumberField = (/** @type {{ label: string, name: string, value: number, ro: boolean, title: string, act: Act }} */ { label, name, value, ro, title, act }) => html`<label class="pl-field"><span class="pl-lab">${label}</span>
  <span class="pl-num"><${Field} type="number" min="0" max="100" step="0.5" value=${String(value)} disabled=${ro} aria-label=${`${label}, millimetres`} title=${title} commit="change"
    onCommit=${(/** @type {string} */ text) => { const v = Number(text); if (Number.isFinite(v) && v >= 0 && v <= 100 && v !== value) act.set(name, `${v}mm`); }} /><span class="unit">mm</span></span></label>`;

const Spacing = (/** @type {Omit<PlannedProps, 'm'> & { ro: boolean }} */ { p, ro, act }) => html`<${Section} title="Spacing">
  <${NumberField} label="Between pieces" name="gap" value=${p.gap} ro=${ro} title="The gap between two pieces on a plate" act=${act} />
  <${NumberField} label="To the edge" name="margin" value=${p.margin} ro=${ro} title="Between a piece and the edge of the bed: the brim, the skirt, the purge line" act=${act} />
  <${Seg} label="Turns" options=${[['allowed', 'Allowed', 'A piece may turn a quarter to fit'], ['off', 'Off']]} current=${p.turn ? 'allowed' : 'off'} on=${(/** @type {string | null} */ v) => act.set('turn', v)} disabled=${ro} />
<//>`;

/**
 * One plate as a small picture of its bed (docs/plates.html): the bed to scale, every piece where the packer put it, as its
 * own top-down picture (the page's one picture service, js/thumbs.js; `m.picture`) turned as it is turned on the bed.  Every
 * plate is drawn at the same width, so the beds are one scale.  The numbers are the layout's (`x`, `y` the piece's front edge
 * and left, `w` by `d` its footprint as placed, `turned` a quarter counter-clockwise): the page works out only where on the
 * card they fall.
 */
export function Bed(/** @type {{ m: Model, p: Planned, pl: Plate }} */ { m, p, pl }) {
  if (!p.bed) return null; // a printer with no bed size has no picture of one (the panel draws none either)
  const [bw, bd] = p.bed;
  return html`<span class="pl-bed" style=${`aspect-ratio: ${bw} / ${bd}`} aria-hidden="true">${pl.pieces.map((q, i) => {
    const kind = p.kinds[q.kind];
    const pic = m.picture ? m.picture(kind) : null;
    const [sw, sd] = kind.size;
    return html`<span key=${i} class=${`pl-bp${q.pinned ? ' pin' : ''}`} style=${`left:${(q.x / bw) * 100}%;top:${((bd - q.y - q.d) / bd) * 100}%;width:${(q.w / bw) * 100}%;height:${(q.d / bd) * 100}%`}>
      ${pic ? html`<span class="pl-bimg" style=${`width:${(sw / (q.turned ? q.d : q.w)) * 100}%;height:${(sd / (q.turned ? q.w : q.d)) * 100}%;transform:translate(-50%,-50%) rotate(${q.turned ? -90 : 0}deg)`}><${Thumb} svc=${pic.svc} spec=${pic.spec} className="pl-bpic" onError=${pic.onError} /></span>` : null}</span>`;
  })}</span>`;
}

const Plates = (/** @type {PlannedProps} */ { m, p, act }) => {
  const inv = m.inventory.value;
  const totals = plateTotals(p);
  const low = p.kinds.filter((k) => !k.exact).length;
  return html`<${Section} title="Plates">
    ${exists(inv) && p.checkInventory !== false ? html`<p class="inv-sum" title="Pieces your inventory already holds are left off the plates; the switch under Inventory prints them anyway"><b>${`${totals.held} in inventory`}</b>${` · ${totals.toPrint} to print`}</p>` : null}
    <div class="pl-plates">${p.layout.map((pl) => {
      const i = pl.plate - 1;
      const on = i === p.view;
      const counts = new Map();
      for (const q of pl.pieces) counts.set(q.name, (counts.get(q.name) ?? 0) + 1);
      const what = [...counts].map(([n, c]) => `${c} × ${n}`).join(', ');
      return html`<button type="button" key=${pl.plate} class=${`pl-plate${on ? ' on' : ''}`} aria-pressed=${String(on)} title=${`${on ? 'On the bed in the view' : 'Show this plate on the bed'}: ${what}`} onClick=${() => { if (!on) act.show(i); }}>
        ${p.bed ? html`<${Bed} m=${m} p=${p} pl=${pl} />` : null}
        <b>${`Plate ${pl.plate}`}</b><span class="pl-n">${plural(pl.pieces.length, 'piece')}</span>${p.colour === 'multicolour' && pl.filaments.length ? html`<span class="pl-fil">${pl.filaments.join(' · ')}</span>` : null}</button>`;
    })}</div>
    ${p.groupingCost ? html`<p class="note warn">${`Grouping by filament takes ${plural(p.groupingCost, 'more plate')} than packing by size. `}<button type="button" class="linkish" onClick=${() => act.set('group', 'size')}>Pack by size only</button></p>` : null}
    <p class=${`note${low ? ' warn' : ' ok'}`}>${low
      ? `${plural(low, 'tile')} shown as draft meshes, not yet checked: full builds start for the plate in view; the rest build when you export. The plates are laid out again by what the full builds cover.`
      : 'Every piece is a full build, checked by the printability gate.'}</p>
  <//>`;
};

const Pins = (/** @type {Omit<PlannedProps, 'm'> & { ro: boolean }} */ { p, ro, act }) => {
  /** @type {{ plate: number, q: import('../api').PlanPlaced }[]} */
  const pinned = [];
  for (const pl of p.layout) for (const q of pl.pieces) if (q.pinned) pinned.push({ plate: pl.plate, q });
  return html`<${Section} title="Pins" cls="pl-pins">
    <p class="note">${pinned.length ? 'A pinned piece stays where it was dropped; the rest are packed around it.' : 'Look down on the bed (Top) and drag a piece: it stays where you drop it, and the rest are packed around it.'}</p>
    ${pinned.map(({ plate, q }) => {
      const line = p.pieces.find((x) => x.id === q.id);
      return html`<div class="pl-pc" key=${`${plate}-${q.copy}-${q.kind}`} style="grid-template-columns: minmax(0, 1fr) auto"><span class="pl-pn"><b>${q.name}</b><span class="pl-ps">${`${line?.id ? `${line.id} · ` : ''}plate ${plate}`}</span></span>
        <button type="button" class="linkish" disabled=${ro || q.id == null} title=${q.id == null ? 'This piece has no id in the list, so an edit cannot name it' : undefined} onClick=${() => { if (q.id != null) act.unpin(q.id, q.name); }}>Unpin</button></div>`;
    })}
    ${pinned.length > 1 ? html`<button type="button" class="linkish" disabled=${ro} onClick=${() => act.unpinAll()}>Unpin all</button>` : null}
  <//>`;
};

const Pieces = (/** @type {SectionProps} */ { m, p, ro, act }) => {
  const ps = 'pieces' in p ? p.pieces : [];
  const t = m.tile.value;
  return html`<${Section} title="Pieces" cls="pl-pieces">
    ${!ps.length ? html`<p class="note">No pieces yet. Open a tile and press Add to print list, or use a tile row’s menu; or write piece lines in Source.</p>` : ps.map((q) => {
      const name = nameOf(q.tile).replace(/\.dtile$/i, '');
      const title = q.tile + (q.seed != null ? `, seed ${q.seed}` : '') + (q.system ? `, ${systemName(q.system)}` : '') + (q.print !== 'missing' ? `, print=${q.print}` : '');
      const sub = [q.seed != null ? `seed ${q.seed}` : '', q.system ? systemName(q.system) : '', q.print !== 'missing' ? q.print : ''].filter(Boolean).join(' · ');
      const step = (/** @type {any} */ text, /** @type {string} */ tip, /** @type {() => void} */ run, /** @type {boolean} */ off) => html`<button type="button" class="pl-step" title=${tip} disabled=${ro || off} onClick=${run}>${text}</button>`;
      return html`<div class=${`pl-pc${q.print === 'skip' ? ' skip' : ''}`} key=${q.id}><span class="pl-pn"><b title=${title}>${name}</b>${sub ? html`<span class="pl-ps">${sub}</span>` : null}</span>
        ${step(html`<${Icon} name="minus" size=${14} />`, 'One fewer', () => act.setCount(t.id, q.id, q.count - 1), q.count <= 1)}<span class="pl-cnt">${String(q.count)}</span>${step(html`<${Icon} name="plus" size=${14} />`, 'One more', () => act.setCount(t.id, q.id, q.count + 1), false)}${step(html`<${Icon} name="x" size=${14} />`, `Take ${name} out of the list`, () => act.removePiece(t.id, q.id, name), false)}</div>`;
    })}
  <//>`;
};

const Inventory = (/** @type {SectionProps} */ { m, p, ro, act }) => {
  const inv = m.inventory.value;
  const check = !('checkInventory' in p) || p.checkInventory !== false;
  const body = [];
  if (!exists(inv)) body.push(html`<p class="note" key="none">No inventory yet, so every piece prints. The sheet after a print, or I have this on a tile, makes it.</p>`);
  else if (p.planned) {
    const { held, toPrint } = plateTotals(p);
    body.push(html`<p class="inv-sum" key="sum"><b>${check ? `${held} in inventory` : 'Inventory not used'}</b>${` · ${toPrint} to print`}</p>`);
    if (check) {
      const rows = [];
      for (const k of p.kinds) {
        if (!(k.have > 0 || k.otherColour > 0)) continue;
        const w = wantLine(k, p.colour);
        const sys = k.system ? `${systemName(k.system)}, seed ${k.seedUsed}` : undefined;
        const tip = w.caution ? `${sys || ''}${sys ? '. ' : ''}You have this tile and seed in the other colour; it prints unless you leave it off (print=skip) or switch the mode` : sys;
        rows.push(html`<div class=${`pl-kind${w.caution ? ' caution' : ''}`} title=${tip} key=${`k${rows.length}`}><b>${k.name}</b><span class="pl-kw">${k.want > k.count ? `${k.want} wanted` : `${k.want}`}</span><span class="pl-kt">${w.text}</span></div>`);
      }
      for (const h of p.held) rows.push(html`<div class="pl-kind" key=${`h${rows.length}`}><b>${h.name}</b><span class="pl-kw">${`${h.have}`}</span><span class="pl-kt">${`have ${h.have} · none to print`}</span></div>`);
      if (rows.length) body.push(html`<div class="pl-kinds" key="kinds">${rows}</div>`);
    }
  }
  return html`<${Section} title="Inventory" cls="pl-inv">
    <${Seg} label="Print" options=${[['check', 'Only what I don’t have', 'The count, less what your inventory holds of that exact piece (tile, seed, system, colour)'], ['ignore', 'Everything', 'Every piece the list names, whatever you have']]} current=${check ? 'check' : 'ignore'} on=${(/** @type {string | null} */ v) => act.set('inventory', v)} disabled=${ro} />
    ${body}
  <//>`;
};

const After = (/** @type {PanelProps & { p: Panel }} */ { m, p, act }) => {
  if (!p.planned) return html`<${Section} title="After the print" cls="pl-after" />`;
  const ui = m.ui.value;
  const a = ui.after;
  const n = p.layout.length;
  if (!a.open) {
    return html`<${Section} title="After the print" cls="pl-after">
      <p class="note">When the plate is off the printer, mark what failed and the rest goes into your inventory. You can do this any time, from any plate.</p>
      <${Button} text="Record a plate…" run=${() => act.openSheet(p.view + 1)} />
    <//>`;
  }
  const plate = Math.min(Math.max(a.plate, 1), n);
  const pl = p.layout[plate - 1];
  const bad = a.bad.length, good = pl.pieces.length - bad;
  return html`<${Section} title="After the print" cls="pl-after">
    ${n > 1 ? html`<${Seg} label="Plate" options=${p.layout.map((x) => [String(x.plate), String(x.plate)])} current=${String(plate)} on=${(/** @type {string | null} */ v) => act.sheetPlate(Number(v))} />` : null}
    <div class="pl-sheet">${pl.pieces.map((q, i) => {
      const k = p.kinds[q.kind];
      const isBad = a.bad.includes(i);
      return html`<label class=${`pl-piece${isBad ? ' bad' : ''}`} key=${i}><input type="checkbox" checked=${!isBad} aria-label=${`${q.name}, seed ${k?.seedUsed}: good`} onChange=${(/** @type {Event} */ e) => act.sheetMark(i, /** @type {HTMLInputElement} */ (e.target).checked)} />
        <b>${q.name}</b><span class="pl-seed">${`seed ${k?.seedUsed ?? '?'}`}</span><span class="pl-res">${isBad ? 'failed' : 'good'}</span></label>`;
    })}</div>
    <p class="note">${`Dated ${act.today()}, with each tile’s hash, from ${p.title ?? m.name.value}/plate-${plate}. Failed ones are kept in the log and do not count.`}</p>
    <div class="pl-after-go"><button type="button" class="btn on" disabled=${a.busy || !pl.pieces.length} onClick=${act.recordPlate}>${`Add ${good} to the inventory${bad ? ` · ${bad} failed` : ''}`}</button>
      <button type="button" class="linkish" onClick=${act.closeSheet}>Close</button></div>
  <//>`;
};

const Export = (/** @type {PlannedProps} */ { m, p, act }) => {
  const x = m.ui.value.exporting;
  const bambu = p.format === 'bambu';
  const busy = x.busy;
  const files = x.files;
  return html`<${Section} title="" cls="pl-export">
    <button type="button" class="btn on" disabled=${busy || (p.colour === 'multicolour' && !p.slotCheck.ok)} title=${bambu ? 'One Bambu Studio project holding every plate' : 'One file per plate: every slicer reads an STL'} onClick=${() => act.doExport()}>${busy ? 'Exporting…' : bambu ? 'Export Bambu project' : `Export ${plural(p.layout.length, 'STL')}`}</button>
    ${bambu ? null : html`<button type="button" class="btn" disabled=${busy} title="One painted 3MF per plate, for a slicer that paints by material" onClick=${() => act.doExport('3mf')}>${`Export ${plural(p.layout.length, '3MF')}`}</button>`}
    <p class="note">${busy ? 'Building every piece in full, through the gate…' : bambu ? 'Every plate in one project; paint per triangle in multicolour.' : 'One STL per plate; single colour needs no paint.'}</p>
    ${x.error ? html`<p class="note bad">${x.error}</p>` : null}
    ${x.failures.map((/** @type {string} */ f, /** @type {number} */ i) => html`<p class="note bad" key=${i}>${`Gate: ${f}`}</p>`)}
    ${files ? html`<div class="pl-files">${files.map((/** @type {string} */ f) => html`<code title=${f} key=${f}>${f.split(/[\\/]/).slice(-3).join('/')}</code>`)}</div>
      ${x.saved ? null : html`<button type="button" class="linkish" onClick=${() => act.reveal(files[0])}>Reveal in Finder</button>`}` : null}
  <//>`;
};

export function PlatesPanel(/** @type {PanelProps} */ { m, act }) {
  const t = m.tile.value;
  if (!t || !m.on.value) return null; // not the plates' turn: the panel draws nothing
  const p = m.plan.value;
  const ro = m.readOnly.value;
  const errors = t.errors ?? [];
  // the list's own mistakes, in the same words whether or not a plan came back with them
  const why = html`${errors.slice(0, 3).map((/** @type {import('../api').Mistake} */ e, /** @type {number} */ i) => html`<p class="note bad" key=${i}>${e.message}${e.help ? html`<span class="pl-help">${` ${e.help}`}</span>` : null}</p>`)}
          ${errors.length > 3 ? html`<p class="note">${`and ${errors.length - 3} more in the messages`}</p>` : null}`;
  return html`
    <div class="body">
      ${!p ? html`<div class="empty">${t.status === 'failed' ? html`<b>This list does not plan.</b>${why}` : 'Laying out the plates…'}</div>` : html`
        ${!p.planned ? html`<p class="note bad">This list does not plan yet. You can still choose the printer and the mode here.</p>${why}` : null}
        ${ro ? html`<p class="note">This file cannot be changed here.</p>` : null}
        ${p.planned ? html`<${Plates} m=${m} p=${p} act=${act} /><${Pins} p=${p} ro=${ro} act=${act} /><${Export} m=${m} p=${p} act=${act} /><${After} m=${m} p=${p} act=${act} />` : null}
        <${Pieces} m=${m} p=${p} ro=${ro} act=${act} /><${Inventory} m=${m} p=${p} ro=${ro} act=${act} /><${Printer} m=${m} p=${p} ro=${ro} act=${act} /><${ColourSection} p=${p} ro=${ro} act=${act} />
        ${p.planned ? html`<${Spacing} p=${p} ro=${ro} act=${act} />` : null}`}
    </div>`;
}
