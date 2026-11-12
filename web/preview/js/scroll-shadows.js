// Scroll shadows: a soft shadow at each edge of a scrolling panel that has more
// content beyond it, gone when scrolled to that edge. One mechanism for every
// scroller in the chrome (never the 3D view): this helper sets the classes
// sh-t, sh-b, sh-l, sh-r from the scroll position; the look is in
// docs/assets/theme.css (.scrolls, --scroll-shadow).  A scroller joins by
// selector in SCROLLERS, whenever it appears in the page.

export const SCROLLERS = [
  '#browser #tiles', '.history-menu', '#export-card', '#target-card',
  '#comp', '#lens .body', '.value-pop .vp-list', '#msg-body', '#place-list',
  '#code .cm-scroller',
];

const EPS = 1;

/** Which edges have content beyond them: {t, b, l, r}. */
export function edges(m) {
  return {
    t: m.scrollTop > EPS,
    b: m.scrollTop + m.clientHeight < m.scrollHeight - EPS,
    l: m.scrollLeft > EPS,
    r: m.scrollLeft + m.clientWidth < m.scrollWidth - EPS,
  };
}

function paint(el) {
  const e = edges(el);
  for (const k of ['t', 'b', 'l', 'r']) el.classList.toggle('sh-' + k, e[k]);
}

export function init(root = document.body, selectors = SCROLLERS) {
  const known = new WeakSet();
  const resize = new ResizeObserver(entries => entries.forEach(e => paint(e.target)));
  let queued = false;
  const attach = () => {
    queued = false;
    for (const el of root.querySelectorAll(selectors.join(','))) {
      if (!known.has(el)) {
        known.add(el);
        el.classList.add('scrolls');
        el.addEventListener('scroll', () => paint(el), { passive: true });
        resize.observe(el);
      }
    }
    root.querySelectorAll('.scrolls').forEach(paint);
  };
  // Content changes (a list refilled, an error shown) move scrollHeight
  // without resizing the box, so any change in the page asks for a repaint.
  new MutationObserver(() => {
    if (!queued) { queued = true; requestAnimationFrame(attach); }
  }).observe(root, { childList: true, subtree: true, characterData: true });
  attach();
}
