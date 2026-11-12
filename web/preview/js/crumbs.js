// The crumbs over the panel as buttons: one helper for both bars (the folder level's and the
// tile's).  A crumb above the tile names a folder (`place: {root, dir}`) and so is also a place to
// let a tile go (it carries the same `data-drop-*` a folder row does); Places itself takes none.
// No store here: the caller gives the crumbs (nav.js `crumbsFor`) and what a click does.

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

/**
 * Fill `nav` with a button for each of `crumbs`, a `›` after each when `trailing` (before the next
 * element) and between them otherwise.  `go(crumb)` is what a click does.
 */
export function fillCrumbs(nav, crumbs, go, { trailing = false, leading = false } = {}) {
  const nodes = [];
  crumbs.forEach((c, i) => {
    if (i > 0 || leading) { const s = el('span', 'sep', '›'); s.setAttribute('aria-hidden', 'true'); nodes.push(s); }
    const b = el('button', `crumb${c.current ? ' cur' : ''}`);
    b.type = 'button';
    b.append(el('span', '', c.label));
    b.title = c.label;
    if (c.current) b.setAttribute('aria-current', 'location');
    if (c.place && c.place.root != null) { b.dataset.dropRoot = String(c.place.root); b.dataset.dropDir = c.place.dir || ''; }
    b.onclick = () => go(c);
    nodes.push(b);
  });
  if (trailing && crumbs.length) { const s = el('span', 'sep', '›'); s.setAttribute('aria-hidden', 'true'); nodes.push(s); }
  nav.replaceChildren(...nodes);
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
