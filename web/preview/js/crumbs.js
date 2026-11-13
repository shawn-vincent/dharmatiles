// The crumbs over the panel as buttons: one helper for both bars (the folder level's and the
// tile's).  A crumb above the tile names a folder (`place: {root, dir}`) and so is also a place to
// let a tile go (it carries the same `data-drop-*` a folder row does); Places itself takes none.
// No store here: the caller gives the crumbs (nav.js `crumbsFor`) and what a click does.

import { Crumbs } from './view/crumbs.js';
import { html, render } from './view/html.js';

/** Draw `crumbs` into `nav` (the component is js/view/crumbs.js); `go(crumb)` is what a click does. */
export function fillCrumbs(nav, crumbs, go, { trailing = false, leading = false } = {}) {
  render(html`<${Crumbs} crumbs=${crumbs} go=${go} trailing=${trailing} leading=${leading} />`, nav);
}

/** Keep the end of a crumb line in view (where you are), with the start fading out when it is cut off. */
export function pinEnd(scroller) {
  const fit = () => {
    scroller.scrollLeft = scroller.scrollWidth;
    scroller.classList.toggle('clipped', scroller.scrollWidth > scroller.clientWidth + 1);
  };
  fit();
  // the panel can be resized and a crumb can be given its width late: once more when the frame is laid out
  requestAnimationFrame(fit);
}

/** Pin the end now and whenever the bar's size changes (once per scroller). */
export function keepEnd(scroller) {
  if (typeof ResizeObserver === 'undefined') return;
  new ResizeObserver(() => pinEnd(scroller)).observe(scroller);
}
