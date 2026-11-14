// @ts-check
// The tiers the page wants of a map's pieces (or of the picture shelf), without a connection held while anything
// builds.  The page owns one thing here, the set of what it wants; the program owns the order and the work
// (crates/dharmatiles/src/preview/tiers.rs):
//
//   want(id, spec)   adds one to the set; the answer is {buf, docRev} once the program has built it (and null when
//                    what came is of another revision, or the want was dropped)
//   drop(id)         takes one out: the program cancels what nobody wants any more
//
// Each change to the set is one post of the whole set (changes made in the same moment are one post, and a post
// waits for the one before it), which replaces the last: so the page holds one short connection at most, never one
// per piece, and an edit is never queued behind builds.  The program announces each finished tier on the event
// stream; the page then fetches that result by key, once.
//
// The page learns a want's key from the post's answer, but the program announces what is already built while it
// answers, so the announcement can come first.  An announcement that comes while a post is out and names no key the
// page knows is held until that post's answer, then matched: the order the two arrive in cannot lose one.

/** Every set, to hand the stream's announcements to. */
const sets = new Set();

/** @typedef {Pick<import('./api').StreamEvent & { type: 'tier' }, 'key' | 'docRev' | 'error' | 'mistake'>} TierDone  the tier event's words (api.rs `StreamEvent`) */

/** The stream said a tier is done. @param {TierDone} m */
export function tierReady(m) { for (const s of sets) s.ready(m); }
/** The stream is back after a gap: every set is posted again, so nothing wanted is lost with what was announced meanwhile. */
export function tiersReconnected() { for (const s of sets) s.repost(); }

/**
 * @param {{ host: { tiersWant: Function, tierResult: Function }, scope: string, doc?: () => string | null }} o
 */
export function createTiers({ host, scope, doc = () => null }) {
  /** @type {Map<string, { spec: any, resolve: (v: { buf: ArrayBuffer, docRev: number } | null) => void, reject: (e: Error) => void }>} */
  const wants = new Map();
  /** @type {Map<string, string>} the program's key for each want, from the last post's answer */
  const byKey = new Map();
  /** @type {Map<string, TierDone>} announcements that came while a post was out, before its answer named their key */
  const early = new Map();
  let scheduled = false, posting = false, again = false;

  async function post() {
    if (posting) { again = true; return; }
    posting = true;
    try {
      do {
        again = false;
        const entries = [...wants.entries()];
        const r = await host.tiersWant({ scope, doc: doc(), wants: entries.map(([, w]) => w.spec) });
        if (r.status === 0) { early.clear(); break; } // disconnected: posted again on reconnect, which announces again
        if (r.status !== 200) {
          for (const [id, w] of entries) { wants.delete(id); w.reject(new Error(`the program refused the tiers wanted: ${r.body?.error ?? r.status}`)); }
          break;
        }
        byKey.clear();
        entries.forEach(([id], i) => byKey.set(r.body.keys[i], id));
        const held = [...early.values()];
        early.clear();
        for (const m of held) if (byKey.has(m.key)) set.ready(m);
      } while (again);
    } finally { posting = false; }
  }
  function schedule() {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => { scheduled = false; post().catch((e) => { for (const [id, w] of wants) { wants.delete(id); w.reject(e); } }); });
  }

  const set = {
    /** @param {string} id @param {any} spec @returns {Promise<{ buf: ArrayBuffer, docRev: number } | null>} */
    want(id, spec) {
      const old = wants.get(id);
      if (old) old.resolve(null);
      return new Promise((resolve, reject) => { wants.set(id, { spec, resolve, reject }); schedule(); });
    },
    /** @param {string} id */
    drop(id) {
      const w = wants.get(id);
      if (!w) return;
      wants.delete(id);
      w.resolve(null);
      schedule();
    },
    /** @param {TierDone} m */
    async ready(m) {
      const id = byKey.get(m.key);
      // not known yet, and a post is out: its answer may name it (the program announces the built while it answers)
      if (id === undefined && posting) { early.set(m.key, m); return; }
      const w = id !== undefined ? wants.get(id) : undefined;
      if (!w || id === undefined) return;
      wants.delete(id);
      byKey.delete(m.key);
      // `mistake`: the text has a mistake in it (the document's to show), not a failure of the program
      if (m.error) { w.reject(Object.assign(new Error(m.error), { mistake: !!m.mistake })); return; }
      try {
        const buf = await host.tierResult(m.key, m.docRev);
        w.resolve(buf ? { buf, docRev: m.docRev } : null);
      } catch (e) { w.reject(/** @type {Error} */ (e)); }
    },
    repost() { if (wants.size) schedule(); },
    /** How many are wanted and not yet delivered. */
    get size() { return wants.size; },
  };
  sets.add(set);
  return set;
}
