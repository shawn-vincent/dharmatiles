// The crumbs over a panel as buttons: one component for both bars (the folder level's and the tile's).  A crumb above
// the tile names a folder (`place: {root, dir}`) and so is also a place to let a tile go (it carries the same
// `data-drop-*` a folder row does); Places itself takes none.  The caller gives the crumbs (nav.js `crumbsFor`) and
// what a click does: `go(crumb)`.  A `›` follows each crumb when `trailing` and comes between them otherwise.

import { Fragment, html } from './html.js';
import { Icon } from './icons.js';

export function Crumbs({ crumbs, go, trailing = false, leading = false }) {
  return html`${crumbs.map((c, i) => html`${i > 0 || leading ? html`<span class="sep" aria-hidden="true"><${Icon} name="chevron-right" size=${14} /></span>` : null}<button type="button" key=${c.label + i} class=${`crumb${c.current ? ' cur' : ''}`} title=${c.label} aria-current=${c.current ? 'location' : undefined}
    data-drop-root=${c.place && c.place.root != null ? String(c.place.root) : undefined} data-drop-dir=${c.place && c.place.root != null ? c.place.dir || '' : undefined} onClick=${() => go(c)}><span>${c.label}</span></button>`)}${trailing && crumbs.length ? html`<span class="sep" aria-hidden="true"><${Icon} name="chevron-right" size=${14} /></span>` : null}`;
}

/** The crumbs below the tile (the thing, the control; a map's Add step and its folders): each goes back to its level, `go(crumb)`. */
export const DeepCrumbs = ({ crumbs, go }) => html`${crumbs.map((c) => html`<${Fragment} key=${c.id}><span class="sep" aria-hidden="true"><${Icon} name="chevron-right" size=${14} /></span><button type="button" class=${`crumb${c.current ? ' cur' : ''}`} title=${c.id === 'thing' ? 'Back to this thing' : c.label} aria-current=${c.current ? 'location' : undefined} onClick=${() => go(c)}><span>${c.label}</span></button><//>`)}`;
