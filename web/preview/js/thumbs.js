// @ts-check
// Pictures of tiles, from one place: "a picture of tile X (at seed S), at this size, on this plate".  The Add tiles
// grid, the seed strip and whatever shows a tile small (the map's plan strip, the plates, the shelf, the history)
// ask here and nowhere else.
//
// The pictures are made the way the page already makes them, not by a second path: the program builds the tile at the
// `super-low` tier (a map's pieces use the same lane, the same cache and the same seed override: `POST
// /api/tiers/want`, crates/dharmatiles/src/preview/tiers.rs) and the page draws that mesh once on one small offscreen
// renderer (editor/scene-thumbs.js), overhang painted as everywhere.  This module owns three things:
//
//   * a queue that runs at most `LIMIT` (five) pictures at a time: a picture is built when something asks for it, and
//     never on idle; what nobody waits for any more (its element went away before its turn) is not built;
//   * the session's cache, by (tile, tier, seed, revision, system, printer, size, plate): a picture asked for again
//     is the same promise, so a rejected seed comes back free;
//   * `Thumb`, an element that is a quiet placeholder until the picture arrives.
//
// A failure is never hidden: the promise rejects and the one who asked shows it (the cache forgets it, so asking
// again tries again).

import { createTiers } from './tiers.js';
import { html, useEffect, useState } from './view/html.js';

/** How many pictures are being made at once. */
export const LIMIT = 5;

/**
 * What one picture is of.
 * @typedef {object} Spec
 * @property {string} tile       the program's id of the tile
 * @property {number} [seed]     a seed to build it at, in place of the tile's own (the file is not edited)
 * @property {string} [text]     a source text to build in place of the file's own (a step of the history): the file is not edited
 * @property {string} [file]     the file `text` is of (the tile's own when left out)
 * @property {string} [system]   `db` or `ol`
 * @property {string} [printer]  a printer profile
 * @property {'super-low' | 'low'} [tier]  the build's resolution; `super-low` unless more is asked for
 * @property {string} [rev]      the version of the tile the picture is of: a changed tile is drawn again
 * @property {'top'} [view]     looking straight down with north up and the tile filling the picture (a plan's picture: the map's squares, a plate's pieces); the default is the three-quarter view
 * @property {string} [plate]    the colour it is drawn on
 * @property {number} [w]        width in px
 * @property {number} [h]        height in px
 */

/** A short name for a text, so a picture of it is kept by what it says (FNV-1a, 32 bits). @param {string} t */
const textName = (t) => { let h = 0x811c9dc5; for (let i = 0; i < t.length; i++) h = Math.imul(h ^ t.charCodeAt(i), 0x01000193); return `${t.length}.${(h >>> 0).toString(16)}`; };

/** The cache key of a picture: everything that makes it a different picture. @param {Spec} s */
export const thumbKey = (s) => [s.tile, s.tier ?? 'super-low', s.seed ?? '-', s.text == null ? '-' : `${s.file ?? '-'}:${textName(s.text)}`, s.rev ?? '-', s.system ?? '-', s.printer ?? '-', s.view ?? '-', `${s.w ?? 0}x${s.h ?? 0}`, s.plate ?? '-'].join('|');

/**
 * The service.
 * `draw(buf, spec)` makes a data or object URL from a built mesh; `valid(spec, got)` says whether the mesh is of the
 * tile's revision as the page knows it now (a stale one is not drawn: null, asked again at the next look).
 * @param {{ host: { tiersWant: Function, tierResult: Function }, draw: (buf: ArrayBuffer, spec: Spec) => string, valid?: (spec: Spec, got: { buf: ArrayBuffer, docRev: number }) => boolean, limit?: number, scope?: string }} o
 */
export function createThumbs({ host, draw, valid = () => true, limit = LIMIT, scope = 'thumbs' }) {
  const tiers = createTiers({ host, scope });
  /** @typedef {{ promise: Promise<string | null>, spec: Spec, wanted: number, started: boolean, resolve: (v: string | null) => void, reject: (e: Error) => void }} Entry */
  /** @type {Map<string, Entry>} */
  const cache = new Map();
  /** @type {string[]} the keys waiting for a place, oldest first */
  const waiting = [];
  let active = 0;

  function pump() {
    while (active < limit && waiting.length) {
      const key = /** @type {string} */ (waiting.shift());
      const e = cache.get(key);
      if (!e || e.started || e.wanted <= 0) continue;
      e.started = true;
      active += 1;
      run(key, e).finally(() => { active -= 1; pump(); });
    }
  }

  /** @param {string} key @param {Entry} e */
  async function run(key, e) {
    const { spec } = e;
    try {
      const got = await tiers.want(key, { tile: spec.tile, tier: spec.tier ?? 'super-low', seed: spec.seed, system: spec.system, printer: spec.printer, text: spec.text, file: spec.file });
      if (!got || !valid(spec, got)) { cache.delete(key); e.resolve(null); return; }
      e.resolve(draw(got.buf, spec));
    } catch (err) {
      cache.delete(key); // asked again, it tries again
      e.reject(/** @type {Error} */ (err));
    }
  }

  return {
    /**
     * The picture's URL (null when it could not be made for want of a current mesh, the reason otherwise: the promise
     * rejects).  Each call must be answered by one `release(spec)` when its element no longer wants it.
     * @param {Spec} spec @returns {Promise<string | null>}
     */
    picture(spec) {
      const key = thumbKey(spec);
      let e = cache.get(key);
      if (!e) {
        /** @type {any} */ let resolve, reject;
        const promise = new Promise((a, b) => { resolve = a; reject = b; });
        e = { promise, spec, wanted: 0, started: false, resolve, reject };
        cache.set(key, e);
        waiting.push(key);
      }
      e.wanted += 1;
      pump();
      return e.promise;
    },
    /** One who asked for `spec` no longer waits: a picture nobody waits for, and not begun, is not built. @param {Spec} spec */
    release(spec) {
      const key = thumbKey(spec);
      const e = cache.get(key);
      if (!e) return;
      e.wanted -= 1;
      if (e.wanted > 0) return;
      // nobody waits for it: not begun, it is not built; begun, the program is told to stop (its want leaves the set)
      if (!e.started) { cache.delete(key); e.resolve(null); } else tiers.drop(key);
    },
    /** How many are being made, how many wait and how many are kept: for the tests and the status line. */
    get stats() { return { active, waiting: new Set(waiting.filter((k) => { const e = cache.get(k); return e && !e.started; })).size, kept: cache.size }; },
  };
}

/** What a picture that could not be made says: for a tile that does not build, its code and the mistake. @param {any} e */
export const brokenWords = (e) => (e?.mistake ? `${e.code ? `${e.code}: ` : ''}${e.message}` : e?.message || 'could not be drawn');
/** The mark a broken picture wears: the code of the tile's mistake (nothing for a failure of the program, which is shown in the messages). @param {string} words */
export const brokenCode = (words) => words.match(/^dt::[a-z0-9-]+/)?.[0] ?? '!';

/**
 * An element for a picture: a quiet placeholder (the plate's own tone) until it arrives, the picture after.  `of` is the
 * service's `picture`/`release` pair; `onError` shows a failure where the user asked for it.
 * @param {{ svc: { picture: (s: Spec) => Promise<string | null>, release: (s: Spec) => void }, spec: Spec, className?: string, onError?: (e: Error) => void, title?: string }} p
 */
export function Thumb({ svc, spec, className = '', onError, title }) {
  const [url, setUrl] = useState(/** @type {string | null} */ (null));
  const [bad, setBad] = useState(/** @type {string | null} */ (null));
  const key = thumbKey(spec);
  useEffect(() => {
    let live = true;
    setUrl(null); setBad(null);
    svc.picture(spec).then((u) => { if (live && u) setUrl(u); }).catch((e) => { if (live) setBad(brokenWords(e)); onError?.(e); });
    return () => { live = false; svc.release(spec); };
  }, [key]);
  return html`<span class=${`thumb-pic${url ? '' : bad ? ' bad' : ' ph'} ${className}`} title=${bad ?? title}>${url ? html`<img alt="" src=${url} draggable=${false} />` : bad ? html`<span class="broken"><b>${brokenCode(bad)}</b></span>` : null}</span>`;
}
