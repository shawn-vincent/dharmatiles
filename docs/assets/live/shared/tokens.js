// Colour tokens (`--accent`, `--water` …) read once and kept: reading one with
// getComputedStyle forces a style recalculation, and the overlay asks for
// several on every frame.  The cache is dropped when the theme changes (the
// `data-theme` attribute on the root, or the system's light or dark).

const cache = new Map();
let watching = false;

function watch() {
  watching = true;
  const drop = () => cache.clear();
  new MutationObserver(drop).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] });
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', drop);
}

/** The value of the CSS custom property `name` on the root (empty when unset). */
export function token(name) {
  if (!watching) watch();
  let v = cache.get(name);
  if (v === undefined) {
    v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    cache.set(name, v);
  }
  return v;
}
