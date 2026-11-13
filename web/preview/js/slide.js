// The one slide of the panel (docs/preview.html, "The panel").  Every step between two levels of the trail, whatever it crosses, is this
// function: a tile's levels and a map's (editor/levels.js), the folder levels (view/places-panel.js), the step between the folder browser
// and a document (editor/levels.js `showPage`).  The timing, the easing, the way reduced motion
// reads, the direction rule (nav.js `slideDir`: deeper comes from the right, shallower from the left) and the keyboard's wait for the
// slide to end are here and nowhere else; a test reads the code for any other animation of a level.

const SLIDE_MS = 200;
const EASE = 'cubic-bezier(.2, .7, .2, 1)';
/** Does the viewer ask for no motion? */
export const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

/** The slides running, by the box they happen in: a box has one slide at a time, and a new one finishes the one before. */
const running = new Map();

/** End the slide running in `scope` (if any) now: its old level is taken away at once. */
export const finishSlide = (scope) => running.get(scope)?.();

/**
 * Take level `out` off and level `to` on, in the box `scope`.  The two stand in the same place (stacked) and `to` is already shown;
 * `dir` is nav.js `slideDir` (1 `to` comes in from the right, -1 from the left, 0 a swap).  `leave(out)` takes `out` away once it has
 * gone (hide it, draw it no more); with `animate` false, reduced motion or no direction it is called at once.  A slide still
 * running in `scope` is finished first, so the box always ends on one level.
 */
export function slide({ scope, to, out, dir, animate = true, leave }) {
  finishSlide(scope);
  if (!animate || reduced() || !dir || !out) { if (out) leave(out); return; }
  const opts = { duration: SLIDE_MS, easing: EASE, fill: 'both' };
  const inn = to.animate([{ transform: `translateX(${dir * 100}%)` }, { transform: 'translateX(0)' }], opts);
  const gone = out.animate([{ transform: 'translateX(0)' }, { transform: `translateX(${-dir * 100}%)` }], opts);
  let over = false;
  // the old level leaves when the slide ends; the timer backs that up, so a page whose frames are paused still ends on one level
  const timer = setTimeout(() => done(), SLIDE_MS + 100);
  function done() {
    if (over) return;
    over = true;
    clearTimeout(timer);
    if (running.get(scope) === done) running.delete(scope);
    inn.cancel();
    gone.cancel();
    leave(out);
  }
  running.set(scope, done);
  gone.onfinish = done;
}

/** Run `fn` once the slide just started has ended: where the keyboard goes after a step (the same wait for every level). */
export const afterSlide = (fn) => setTimeout(fn, reduced() ? 0 : SLIDE_MS + 20);
