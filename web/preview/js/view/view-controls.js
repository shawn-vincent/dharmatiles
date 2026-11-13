// The view's controls: the view buttons (the shortcuts beside the view, and the whole list in the display menu), the backdrop
// buttons, and the debug layer's legend.  Each is a function of what it is given; editor/chrome.js gives them the model.

import { html } from './html.js';
import { Icon, cameraIcon } from './icons.js';

/**
 * A row of camera buttons: `names` in order, each from `entries` (name -> {label, n}), each drawn as its camera icon (js/view/icons.js)
 * with its word as the tooltip; the one the camera is at (`active`) is pressed.  With `words` the label is written beside the icon
 * (the display menu's list of all nine).
 */
export function ViewButtons({ names, entries, active, onPick, words = false }) {
  return html`${names.map((name) => {
    const e = entries.get(name);
    if (!e) return null;
    return html`<button type="button" class=${`view${name === active ? ' on' : ''}`} key=${name} data-view=${name} aria-pressed=${String(name === active)} aria-label=${e.label} title=${`${e.label} (${e.n})`} onClick=${() => onPick(name)}><${Icon} name=${cameraIcon(name)} size=${20} />${words ? html`<span>${e.label}</span>` : null}</button>`;
  })}`;
}

/** The backdrops: the place the tiles stand in, as a grid of choices with a swatch of each; the chosen one is pressed. */
export function Backdrops({ kinds, current, onPick }) {
  return html`${kinds.map(([kind, label, title]) => html`<button type="button" key=${kind} class=${`bd bd-${kind}${kind === current ? ' on' : ''}`} data-kind=${kind} aria-pressed=${String(kind === current)} title=${title} onClick=${() => onPick(kind)}><i class="bd-sw" aria-hidden="true"></i><span>${label}</span></button>`)}`;
}

/** The debug layer's legend rows: a swatch, a label and, for marks and lines, how many. */
export function DebugRows({ rows, colour }) {
  if (!rows.length) return html`<li>Nothing in this tile shows here yet: grass does.</li>`;
  return html`${rows.map((r) => html`<li key=${r.label}><i class=${`sw ${r.kind}`} style=${`background: ${colour(r.token)}`}></i><span>${r.label}</span>${r.kind === 'dot' || r.kind === 'line' ? html`<span class="n">${r.n}</span>` : null}</li>`)}`;
}
