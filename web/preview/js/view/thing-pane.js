// @ts-check
// The thing view's pane (editor/thing.js has what it shows): over the thing's own 3D view, a card with the thing, its seed, the room
// it takes (the smallest and the largest box, in printed mm with the dmm the document writes beside them) and its controls as built,
// and under it the strip of draws.  Each is a component of the model; none keeps a copy of a fact it holds.

import { html } from './html.js';
import { Icon } from './icons.js';
import { VIEWS } from '../viewer.js';
import { extentText } from '../thing-model.js';

/** The camera presets the pane offers: the ones a thing is looked at from. */
const LOOKS = ['painted', 'top', 'front'];

/** @param {{ m: any }} p  `m` is the model editor/thing.js exports */
export function ThingPane({ m }) {
  const t = m.target.value;
  if (!m.shown.value || !t) return null;
  const d = m.drawn.value;
  const at = m.seed.value;
  const look = m.lookFrom().value.view;
  const set = (/** @type {number} */ n) => { m.seed.value = Math.max(1, Math.min(99999, Math.round(n) || 1)); };
  return html`
    <div class="card thing-card" role="region" aria-label="The thing">
      <div class="thing-head"><span class="eyebrow">thing</span><b>${t.name}</b>${t.element !== 'define' && t.element !== t.name ? html`<span class="thing-of">${t.element}</span>` : null}${m.busy.value ? html`<span class="thing-busy" role="status">drawing…</span>` : null}</div>
      ${t.things.length > 1 ? html`<div class="seg thing-kids" role="group" aria-label="Which thing of the scatter">${t.things.map((/** @type {string} */ w, /** @type {number} */ i) => html`<button type="button" key=${i} aria-pressed=${String(i === t.child)} onClick=${() => { m.child.value = i; }}>${w}</button>`)}</div>` : null}
      <div class="thing-seed" role="group" aria-label="The draw">
        <span class="cap">Draw · seed</span>
        <button type="button" class="btn" aria-label="The seed before" title="The seed before" disabled=${at <= 1} onClick=${() => set(at - 1)}><${Icon} name="chevron-left" size=${14} /></button>
        <input type="number" min="1" max="99999" value=${at} aria-label="Seed" onChange=${(/** @type {Event} */ e) => set(Number(/** @type {HTMLInputElement} */ (e.target).value))} />
        <button type="button" class="btn" aria-label="The next seed" title="The next seed" onClick=${() => set(at + 1)}><${Icon} name="chevron-right" size=${14} /></button>
        <button type="button" class="dice" aria-label="Roll a new seed" title="Roll a new seed: another draw" onClick=${() => set(1 + Math.floor(Math.random() * 9999))}><${Icon} name="dice" size=${16} /></button>
      </div>
      ${m.problem.value ? html`<p class="thing-problem" role="alert">${m.problem.value}</p>` : null}
      ${d ? html`<${Extent} e=${d.extent} size=${d.draws[0].size} triangles=${d.draws[0].triangles} />` : null}
      ${d && !d.draws[0].checked ? html`<p class="thing-draft" role="status">Draft · not yet checked: the gate's overhang paint comes when the full draw lands.</p>` : null}
      ${d && d.shown.length ? html`<dl class="thing-controls" aria-label="Controls as built">${d.shown.map((/** @type {import('../api').ThingControl} */ c) => html`<div key=${c.control}><dt>${c.control}</dt><dd>${c.text}<span class="from"> from ${c.from}</span></dd></div>`)}</dl>` : null}
      <div class="seg thing-looks" role="group" aria-label="Look from">${VIEWS.filter((v) => LOOKS.includes(v[0])).map((v) => html`<button type="button" key=${v[0]} aria-pressed=${String(look === v[0])} onClick=${() => m.describe(JSON.stringify(t.which), v[0])}>${v[1]}</button>`)}</div>
    </div>
    <div class="thing-strip" role="group" aria-label="Draws of the thing">
      ${m.pics.value.map((/** @type {{ seed: number, url: string, triangles: number }} */ p) => html`<button type="button" key=${p.seed} class=${`seed-card${p.seed === at ? ' on' : ''}`} aria-pressed=${String(p.seed === at)} title=${`Draw ${p.seed}: ${p.triangles} triangles`} onClick=${() => set(p.seed)}>
        <span class="thumb-pic"><img alt="" src=${p.url} draggable=${false} /></span><b>${p.seed}</b></button>`)}
      ${m.pics.value.length ? html`<span class="thing-more"><button type="button" class="btn" disabled=${m.page.value === 0} onClick=${() => { m.page.value -= 1; }}>Earlier</button><button type="button" class="btn" onClick=${() => { m.page.value += 1; }}>More draws</button></span>` : null}
    </div>`;
}

/**
 * The room the thing takes: the smallest and the largest box it can draw, as the two outlines in the view, each in printed mm
 * beside the dmm the document writes.
 * @param {{ e: import('../api').ThingExtent, size: import('../api').Dims, triangles: number }} p
 */
function Extent({ e, size, triangles }) {
  const lo = extentText(e.least, e.leastDmm), hi = extentText(e.most, e.mostDmm);
  const now = extentText(size, size);
  return html`<dl class="thing-extent" aria-label="The room it takes">
    <div><dt><i class="sw least" aria-hidden="true"></i>smallest</dt><dd>${lo.printed}<span class="from"> (${lo.written} written)</span></dd></div>
    <div><dt><i class="sw most" aria-hidden="true"></i>largest</dt><dd>${hi.printed}<span class="from"> (${hi.written} written)</span></dd></div>
    <div><dt>this draw</dt><dd>${now.printed}<span class="from"> · ${triangles} triangles</span></dd></div>
  </dl>`;
}
