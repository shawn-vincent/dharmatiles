// The view's controls: the view buttons (the shortcuts beside the view, and the whole list in the display menu), the backdrop
// buttons, and the debug layer's legend.  Each is a function of what it is given; editor/chrome.js gives them the model.

import { html } from './html.js';

/** A row of view buttons: `names` in order, each from `entries` (name -> {label, n}); the one that is `active` is pressed. */
export function ViewButtons({ names, entries, active, onPick }) {
  return html`${names.map((name) => {
    const e = entries.get(name);
    if (!e) return null;
    return html`<button type="button" class=${`view${name === active ? ' on' : ''}`} key=${name} data-view=${name} aria-pressed=${String(name === active)} title=${`${e.label} (${e.n})`} onClick=${() => onPick(name)}>${e.label}</button>`;
  })}`;
}

/** The backdrops: the place the tiles stand in; the chosen one is pressed. */
export function Backdrops({ kinds, current, onPick }) {
  return html`${kinds.map(([kind, label, title]) => html`<button type="button" key=${kind} class=${kind === current ? 'on' : undefined} data-kind=${kind} aria-pressed=${String(kind === current)} title=${title} onClick=${() => onPick(kind)}>${label}</button>`)}`;
}

/** The debug layer's legend rows: a swatch, a label and, for marks and lines, how many. */
export function DebugRows({ rows, colour }) {
  if (!rows.length) return html`<li>Nothing in this tile shows here yet: grass does.</li>`;
  return html`${rows.map((r) => html`<li key=${r.label}><i class=${`sw ${r.kind}`} style=${`background: ${colour(r.token)}`}></i><span>${r.label}</span>${r.kind === 'dot' || r.kind === 'line' ? html`<span class="n">${r.n}</span>` : null}</li>`)}`;
}
