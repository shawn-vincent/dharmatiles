// @ts-check
// What a page of cards may ask of a device, as pure policy (no worker, no DOM) so tests can hold it:
//
//   ByteCache      the engine's result cache, bounded by bytes, not by count: a draft is about a megabyte, a full build is
//                  tens, and a full result is never kept (the card that shows it holds it, and a fresh worker follows it)
//   runawayMs      how long a full build may run before it is stopped, from the program's own estimate of it
//   costWords      the estimate as the Full detail button says it
//   deviceVerdict  whether this device can carry that estimate, from what the browser says of itself; it picks the default
//                  and the wording, and never hides the cost
//   storeKey       the key a draft is kept between visits under: the engine's hash and the text, so a mesh from an
//                  older engine is never found

/** The result cache's size, bytes.  A draft (the low tier, about 24 cells) is under 2 MB of arrays, so about 30 of them. */
export const CACHE_BYTES = 64 * 1024 * 1024;

/** A full build's CPU-second estimate is for one native thread; the browser is slower and busy, so a build is allowed this many times it, within the bounds. */
export const RUNAWAY_FACTOR = 3;
export const RUNAWAY_MIN_MS = 30_000;
export const RUNAWAY_MAX_MS = 240_000;

/**
 * A least-recently-used cache that holds at most `limit` bytes of what is put in it.
 * @template V
 */
export class ByteCache {
  /** @param {number} limit @param {(value: V) => number} sizeOf */
  constructor(limit, sizeOf) {
    this.limit = limit;
    this.sizeOf = sizeOf;
    /** @type {Map<string, {value: V, bytes: number}>} oldest first */
    this.items = new Map();
    this.bytes = 0;
  }

  /** @param {string} key */
  get(key) {
    const hit = this.items.get(key);
    if (!hit) return undefined;
    this.items.delete(key); // used now: the newest
    this.items.set(key, hit);
    return hit.value;
  }

  /** Keep `value`; the oldest go until the rest fit.  A value bigger than the whole limit is not kept (false). */
  /** @param {string} key @param {V} value */
  set(key, value) {
    this.delete(key);
    const bytes = this.sizeOf(value);
    if (bytes > this.limit) return false;
    this.items.set(key, { value, bytes });
    this.bytes += bytes;
    for (const [oldest, it] of this.items) {
      if (this.bytes <= this.limit) break;
      this.items.delete(oldest);
      this.bytes -= it.bytes;
    }
    return true;
  }

  /** @param {string} key */
  delete(key) {
    const it = this.items.get(key);
    if (!it) return;
    this.items.delete(key);
    this.bytes -= it.bytes;
  }

  get size() { return this.items.size; }
}

/** What a kept result holds, bytes: its arrays (and a small allowance for a check's answer, the map). */
/** @param {{built?: {positions: {byteLength: number}, colours: {byteLength: number}, stl: {byteLength: number}} | null, checked?: unknown}} kept */
export const keptBytes = (kept) => (kept.built ? kept.built.positions.byteLength + kept.built.colours.byteLength + kept.built.stl.byteLength : 0) + (kept.checked ? 20_000 : 0);

/** The limit for a full build of `cpuSecs` estimated seconds (none known: the longest). */
/** @param {number | null | undefined} cpuSecs */
export const runawayMs = (cpuSecs) => {
  if (cpuSecs == null || !(cpuSecs > 0)) return RUNAWAY_MAX_MS;
  return Math.min(RUNAWAY_MAX_MS, Math.max(RUNAWAY_MIN_MS, Math.round(cpuSecs * RUNAWAY_FACTOR * 1000)));
};

/** MB as a person says it: "2 GB", "300 MB". */
/** @param {number} mb */
const sizeWords = (mb) => (mb >= 1000 ? `${(mb / 1000).toFixed(mb >= 10_000 ? 0 : 1).replace(/\.0$/, '')} GB` : `${Math.max(10, Math.round(mb / 10) * 10)} MB`);

/** The estimate as words for the button: "about 2 GB, 20 s". */
/** @param {{peakMb: number, cpuSecs: number}} cost */
export const costWords = (cost) => {
  const s = cost.cpuSecs;
  const secs = s < 1 ? 'under 1 s' : s < 10 ? `${Math.round(s)} s` : `${Math.round(s / 5) * 5} s`;
  return `about ${sizeWords(cost.peakMb)}, ${secs}`;
};

/**
 * Whether the device can carry a full build of `cost`.  A tab that passes about 40 % of a device's memory is the one the system
 * stops, so where the browser says how much memory the device has (`navigator.deviceMemory`, GB, rounded down to a power of two
 * and capped at 8: Chrome and Edge) that decides; where it does not (Safari, Firefox), a phone (a coarse pointer and a small
 * screen) is taken to carry about 1 GB, and anything else is taken to carry it.  `deviceMemory` is used for this and for the
 * words only: the cost is always shown.
 */
/** @param {{peakMb: number, cpuSecs: number}} cost @param {{deviceMemory?: number, coarse: boolean, smallScreen: boolean}} device @returns {{fits: boolean, why: string}} */
export const deviceVerdict = (cost, device) => {
  if (device.deviceMemory !== undefined) {
    const carries = device.deviceMemory * 1024 * 0.4;
    return cost.peakMb <= carries
      ? { fits: true, why: '' }
      : { fits: false, why: `this device reports ${device.deviceMemory} GB of memory` };
  }
  if (device.coarse && device.smallScreen) {
    return cost.peakMb <= 1000 ? { fits: true, why: '' } : { fits: false, why: 'this looks like a phone' };
  }
  return { fits: true, why: '' };
};

/** The key a kept draft is stored under.  The engine's hash is in it: a mesh the engine of another version made is not found. */
/** @param {string} engine @param {string} system @param {string} text */
export const storeKey = (engine, system, text) => `${engine}\u0000${system}\u0000${text}`;
