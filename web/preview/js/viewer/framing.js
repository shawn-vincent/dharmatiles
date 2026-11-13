// Where the camera goes, decided from a description and nothing else.
//
// The viewer is told what it shows (`describe`: a key naming the document in its mode, what it is, the preset
// asked for, the box of what is there) and decides its own camera.  This module is that decision, with no
// three.js in it, so it is tested alone: `step` takes the description and what the viewer now holds and
// answers with one action, and it holds the only memory of where the camera was: the user's last view of each
// document in each mode.  No other part of the page moves the camera.
//
//   key       what is shown: the document and the mode it is shown in (`tile:PATH`, `map:PATH`, `plates:PATH`).
//   kind      'tile' | 'map' | 'plates': how what is shown is framed as it arrives.
//   view      the preset in force (a name of VIEWS: Iso, Top, Low, Close, ...).
//   asked     a fresh object each time a preset is asked for, so asking for the same one again counts.
//   fresh     a tile with no finished build yet: it is followed while it builds.
//
// What the camera does:
//   - a document shown for the first time is framed on its preset, at once; one the user moved before comes back
//     as they left it;
//   - a preset asked for frames what is shown, with a flight unless the ask says not;
//   - while the user's own view holds (they dragged, wheeled or flew it), nothing the program does moves it:
//     not a rebuild, an edit, a piece arriving; only asking for a preset gives the camera back;
//   - a tile still building widens to take in each part and settles on the finished tile;
//   - a map or the plates frame again whenever what they show changes size, or the view is resized, mid-flight
//     too (the flight lands on the new frame).

/** A box's diagonal, mm. */
const diag = (b) => Math.hypot(...b.hi.map((v, a) => v - b.lo[a]));

/** The same box for the camera's purpose: a small change is not worth a move. */
export function nearBox(a, b) {
  if (!a || !b) return false;
  return Math.abs(diag(a) - diag(b)) < 0.08 * diag(b)
    && a.lo.every((v, k) => Math.abs(v - b.lo[k]) < 2) && a.hi.every((v, k) => Math.abs(v - b.hi[k]) < 2);
}

/** @param {any} box */
const sameBox = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The camera's memory and its rules. */
export function createFraming() {
  /** key -> {camera, view}: the user's last view of a document in a mode. */
  const saved = new Map();
  /** What the camera is on now: {key, view, held, asked, box, finished, following} (null: nothing yet). */
  let cur = null;

  const frame = (view, box, animate) => ({ do: 'frame', view, box, animate });

  return {
    /** What the camera is on, for the page's buttons: the preset, and whether the user's own view holds instead. */
    state: () => (cur ? { view: cur.view, held: cur.held, key: cur.key } : { view: null, held: false, key: null }),

    /** The user took hold of the view (drag, wheel, flight of their own): nothing the program does moves it now. */
    hold() { if (cur && !cur.held) { cur.held = true; return true; } return false; },

    /** The user's views remembered, by key (a test reads it). */
    savedKeys: () => [...saved.keys()],

    /**
     * What to do about the camera now.
     * @param {{desc: {key: string, kind: string, view: string, asked: any, fresh?: boolean} | null, box: any, finished: boolean, flying?: boolean, resized?: boolean, camera: () => any}} input
     *   `box` is what is shown, framed by the preset (a map's close view: the selected piece); `finished` that the
     *   document's own mesh is in; `flying` that a flight is on; `resized` that the view's size changed.
     * @returns {{do: 'keep'} | {do: 'restore', camera: any} | {do: 'frame', view: string, box: any, animate: boolean}}
     */
    step({ desc, box, finished, flying = false, resized = false, camera }) {
      if (!desc) return { do: 'keep' };
      if (!cur || cur.key !== desc.key) {
        if (cur?.held) saved.set(cur.key, { camera: camera(), view: cur.view });
        const back = saved.get(desc.key);
        cur = { key: desc.key, view: desc.view, held: !!back, asked: desc.asked, box, finished: !!finished, following: !!desc.fresh && desc.kind === 'tile' && !finished };
        if (back) return { do: 'restore', camera: back.camera };
        return frame(desc.view, box, false);
      }
      if (desc.asked !== cur.asked) {
        cur.asked = desc.asked; cur.view = desc.view; cur.held = false; cur.box = box; cur.finished = !!finished;
        return frame(desc.view, box, desc.asked?.animate ?? true);
      }
      if (cur.held) { cur.finished = !!finished; return { do: 'keep' }; }
      if (resized) { cur.box = box; return frame(cur.view, box, flying); }
      if (desc.kind === 'tile') {
        if (finished && !cur.finished) { // settles on the finished tile
          cur.finished = true; cur.box = box;
          return frame(cur.view, box, cur.following);
        }
        if (finished) return { do: 'keep' };
        if (nearBox(cur.box, box)) return { do: 'keep' };
        cur.box = box; // widens to take in what the build has made
        return frame(cur.view, box, true);
      }
      if (sameBox(cur.box, box)) return { do: 'keep' };
      cur.box = box; cur.finished = !!finished;
      return frame(cur.view, box, flying);
    },
  };
}
