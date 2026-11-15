// @ts-check
// The overlay on a poster, before the engine has woken: a card whose page picks marks (`overlay`, `highlight`) shows them on its
// picture from the start.  The poster's map is `posters/KEY.map.json`, beside the picture and named by the same key (the map is
// the program's own, dt_eval::map::PosterMap: the map and the square of the target the poster was made for, written by
// `cargo xtask examples`), so nothing here needs the engine.  The marks are drawn
// by the editor's flat renderer; the poster is the tile's top view cropped to its footprint, so a point of the tile is a
// point of the picture.
import { drawn, marksOf, parsePins } from '../../editor/js/overlay-marks.js';
import { renderMarks } from '../../editor/js/overlay-render.js';

/** @typedef {import('../../../web/preview/js/api').PosterMap} PosterMap */

/** @param {HTMLElement} host a <dt-example> that is still cold */
export async function drawOnPoster(host) {
  const img = /** @type {HTMLImageElement | null} */ (host.querySelector('.dtx-poster'));
  const view = host.querySelector('.dtx-view');
  const key = /posters\/([0-9a-f]+)\.jpg/.exec(img?.getAttribute('src') ?? '')?.[1];
  if (!img || !view || !key) return;
  const url = img.src.replace(/\.jpg(\?.*)?$/, '.map.json');
  const res = await fetch(url);
  if (!res.ok) throw new Error(`cannot read the poster's map ${url}: ${res.status} ${res.statusText}`);
  /** @type {PosterMap} */
  const poster = await res.json();
  // a map file made before the square size travelled with it is a bare map: it is made again, never read with a guess
  if (typeof poster?.squareMm !== 'number' || !poster.map) throw new Error(`the poster's map ${url} does not say its square size: it was made by an older program; delete docs/posters/*.map.json and run cargo xtask examples`);
  const { map, squareMm } = poster;
  const marks = marksOf(map);
  const pins = parsePins(host.getAttribute('highlight') ?? '', marks);
  if (pins.errors.length) throw new Error(pins.errors.join('; '));
  const moment = { toggles: new Set((host.getAttribute('overlay') ?? '').split(/[\s,]+/).filter(Boolean)), pinned: pins.ids, hover: null, caret: null };
  const canvas = document.createElement('canvas');
  canvas.className = 'dtx-ov dtx-pre';
  canvas.setAttribute('aria-hidden', 'true');
  view.append(canvas);
  const paint = () => {
    const r = view.getBoundingClientRect();
    const dpr = Math.min(devicePixelRatio || 1, 2);
    canvas.width = Math.round(r.width * dpr);
    canvas.height = Math.round(r.height * dpr);
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('This browser gave no 2D canvas for the poster overlay.');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const fw = map.cols * squareMm, fh = map.rows * squareMm;
    const project = (/** @type {number} */ x, /** @type {number} */ y) => ({ x: (x / fw) * r.width, y: (1 - y / fh) * r.height });
    renderMarks(ctx, { map, sq: squareMm, drawn: drawn(marks, moment), squares: moment.toggles.has('squares'), project, size: { w: r.width, h: r.height } });
  };
  paint();
  new ResizeObserver(paint).observe(view);
}
