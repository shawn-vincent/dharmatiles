// The overlay on a poster, before the engine has woken: a card whose page picks marks (`overlay`, `highlight`) shows them on its
// picture from the start.  The poster's map is `posters/KEY.map.json`, beside the picture and named by the same key (the map is
// the program's own, dt_eval::map, written by `cargo xtask examples`), so nothing here needs the engine.  The marks are drawn
// by the editor's flat renderer; the poster is the tile's top view cropped to its footprint, so a point of the tile is a
// point of the picture.
import { drawn, marksOf, parsePins } from '../../editor/js/overlay-marks.js';
import { renderMarks } from '../../editor/js/overlay-render.js';

/** Square size on the target the posters are made for (DungeonBlocks), mm: the posters are of the `db` build. */
const SQUARE_MM = 35;

/** @param {HTMLElement} host a <dt-example> that is still cold */
export async function drawOnPoster(host) {
  const img = host.querySelector('.dtx-poster');
  const view = host.querySelector('.dtx-view');
  const key = /posters\/([0-9a-f]+)\.jpg/.exec(img?.getAttribute('src') ?? '')?.[1];
  if (!img || !view || !key) return;
  const url = img.src.replace(/\.jpg(\?.*)?$/, '.map.json');
  const res = await fetch(url);
  if (!res.ok) throw new Error(`cannot read the poster's map ${url}: ${res.status} ${res.statusText}`);
  const map = await res.json();
  if (!map) return;
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
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const fw = map.cols * SQUARE_MM, fh = map.rows * SQUARE_MM;
    const project = (x, y) => ({ x: (x / fw) * r.width, y: (1 - y / fh) * r.height });
    renderMarks(ctx, { map, sq: SQUARE_MM, drawn: drawn(marks, moment), squares: moment.toggles.has('squares'), project, size: { w: r.width, h: r.height } });
  };
  paint();
  new ResizeObserver(paint).observe(view);
}
