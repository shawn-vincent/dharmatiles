// @ts-check
// The page's one request layer: every call to the program is made through it (js/host.js has no other way to
// reach the network), and it holds the two rules that keep an answer from putting an older fact back.
//
//   latest   `latest(key, run)`: of the requests made under one key, only the newest one's answer is taken.  A
//            request that is answered after a newer one began (or after the newer one was answered) resolves
//            `SUPERSEDED` and its answer is thrown away, whatever order the network brought them in.  The
//            checks, the names, the offers, a typed value's reading: asked again with every keystroke, and
//            the last one asked is the one shown.
//   queue    `queue(key, run)`: requests under one key are made one after the other, each when the one before has
//            answered, so two quick writes never overtake each other.
//   direct   `direct(run)`: made at once and answered as it comes.
//
// All three count what is in flight, so a test (or the page) can wait for the layer to be idle.

/** What a superseded request resolves. */
export const SUPERSEDED = Symbol('superseded');

export function createRequests() {
  /** @type {Map<string, number>} */
  const newest = new Map();
  /** @type {Map<string, Promise<unknown>>} */
  const tails = new Map();
  let inflight = 0;
  /** @type {Set<() => void>} */
  const waiting = new Set();

  /** @template T @param {Promise<T>} p @returns {Promise<T>} */
  const track = (p) => {
    inflight += 1;
    const done = () => { inflight -= 1; if (!inflight) for (const w of [...waiting]) w(); };
    p.then(done, done);
    return p;
  };

  return {
    /**
     * @template T
     * @param {string} key @param {() => Promise<T>} run
     * @returns {Promise<T | typeof SUPERSEDED>}
     */
    latest(key, run) {
      const mine = (newest.get(key) ?? 0) + 1;
      newest.set(key, mine);
      return track(run().then((r) => (newest.get(key) === mine ? r : SUPERSEDED)));
    },
    /** @template T @param {string} key @param {() => Promise<T>} run @returns {Promise<T>} */
    queue(key, run) {
      const next = (tails.get(key) ?? Promise.resolve()).then(run);
      tails.set(key, next.catch(() => {}));
      return track(next);
    },
    /** @template T @param {() => Promise<T>} run @returns {Promise<T>} */
    direct: (run) => track(run()),
    /** Requests in flight now. */
    get inflight() { return inflight; },
    /** Resolves when nothing is in flight. */
    idle() {
      if (!inflight) return Promise.resolve();
      return new Promise((resolve) => {
        const w = () => { if (inflight) return; waiting.delete(w); resolve(undefined); };
        waiting.add(w);
      });
    },
  };
}
