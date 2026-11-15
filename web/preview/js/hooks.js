// A set of callbacks a part of the page offers to the rest of it (the viewer's render, a user's move of the view).
//
// Each part adds its own with `add(fn)` and gets back the function that removes it; `run(...args)` calls every
// one, in the order added, and a callback that throws does not keep the others from running (its error is
// raised after them, once, so it is seen).  There is no one slot to overwrite or to chain by hand, so the
// order the parts start in decides nothing.

export function hookSet() {
  const fns = /** @type {Set<(...args: any[]) => void>} */ (new Set());
  return {
    /** Add `fn`; the result removes it. */
    /** @param {(...args: any[]) => void} fn */
    add(fn) {
      if (typeof fn !== 'function') throw new TypeError('a hook is a function');
      fns.add(fn);
      return () => { fns.delete(fn); };
    },
    /** Call every hook with `args`. */
    /** @param {any[]} args handed to every hook (each hook set says what its hooks take) */
    run(...args) {
      let failure = null;
      for (const fn of [...fns]) {
        try { fn(...args); } catch (e) { failure ??= e; }
      }
      if (failure) throw failure;
    },
    get size() { return fns.size; },
  };
}
