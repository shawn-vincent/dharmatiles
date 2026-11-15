// @ts-check
// The engine's waiting line, as a pure policy (no worker, no DOM) so it can be held by tests.  The engine is single-threaded in
// the browser, so a build cannot pause partway; everything here follows from that:
//
//   one slot per card    a card holds its newest edit (kind 'edit') and, apart, the full-detail build it asked for ('full');
//                        a newer edit replaces the older one, and never displaces another card's
//   focused card first   the card the reader touched last goes first, its edit before its full build; then the others, by
//                        most recent edit
//   versions             every job carries its card's version; a result that comes back after the card was edited again is
//                        dropped (`isStale`), never painted
//
// A job is {kind: 'edit' | 'full', version, ...whatever the engine needs}.

/** @typedef {import('./engine.js').Job} Job */

export class Queue {
  constructor() {
    /** @type {Map<string, { edit: Job | null, full: Job | null, at: number }>} */
    this.slots = new Map();
    /** @type {string | null} */
    this.focus = null;
    this.clock = 0;
  }

  /** Hold `job` as the card's pending one of its kind.  A new edit makes a waiting full build stale: it is dropped. */
  /** @param {string} card @param {Job} job */
  put(card, job) {
    const slot = this.slots.get(card) ?? { edit: null, full: null, at: 0 };
    slot[job.kind] = job;
    if (job.kind === 'edit') slot.full = null;
    slot.at = ++this.clock;
    this.slots.set(card, slot);
  }

  /** The card's pending job of `kind` is no longer wanted. */
  /** @param {string} card @param {Job['kind']} kind */
  drop(card, kind) {
    const slot = this.slots.get(card);
    if (!slot) return;
    slot[kind] = null;
    if (!slot.edit && !slot.full) this.slots.delete(card);
  }

  /** The card the reader touched last: it goes first. */
  /** @param {string} card */
  focusOn(card) { this.focus = card; }

  /** The pending job of `card` of `kind`, if any. */
  /** @param {string} card @param {Job['kind']} kind */
  pending(card, kind) { return this.slots.get(card)?.[kind] ?? null; }

  /** The order the waiting jobs would run in: [{card, job}], first first. */
  order() {
    const cards = [...this.slots.entries()].sort(([a, x], [b, y]) => Number(b === this.focus) - Number(a === this.focus) || y.at - x.at);
    const out = [];
    for (const [card, slot] of cards) {
      if (slot.edit) out.push({ card, job: slot.edit });
      if (slot.full) out.push({ card, job: slot.full });
    }
    // the focused card's jobs first; of the rest, edits before full builds: an edit is quick and a full build is not
    const mine = out.filter((x) => x.card === this.focus), rest = out.filter((x) => x.card !== this.focus);
    return [...mine, ...rest.filter((x) => x.job.kind === 'edit'), ...rest.filter((x) => x.job.kind === 'full')];
  }

  /** Take the job to run next, or null. */
  next() {
    const first = this.order()[0];
    if (!first) return null;
    this.drop(first.card, first.job.kind);
    return first;
  }

  /** How many waiting jobs run before the card's job of `kind` (-1: it is not waiting). */
  /** @param {string} card @param {Job['kind']} kind */
  ahead(card, kind) {
    return this.order().findIndex((x) => x.card === card && x.job.kind === kind);
  }
}

/** A result for version `got` when the card is at version `now` is stale: the card was edited again since it was asked. */
/** @param {number} now @param {number} got */
export const isStale = (now, got) => got !== now;
