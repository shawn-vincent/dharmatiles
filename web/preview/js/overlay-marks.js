// @ts-check
// The overlay's marks and what state each is in.  A tile's map (the program's `map` JSON, dt_eval::map) holds the places a
// document names: lines, the points a line goes through, regions, the fill points of a region, named points.  Each is a
// mark, with the span of source it is written in.  A mark is off, shown or lit (`stateOf`), and the moment decides which:
//
//   reading    the caret or the pointer is on its source (or on the mark itself): lit
//   exploring  the reader switched its kind on (lines, points, regions, squares): shown
//   teaching   the author pinned it for the sentence being read (`highlight`): lit, and nothing the reader does hides it
//
// This module is the middle of three: the geometry (overlay-geometry.js) turns a mark into shapes, this says which marks to
// draw and how, and a renderer draws them (the editor's overlay.js into its 3D view, overlay-render.js flat for the site's
// cards).  Pure: no DOM, no state.  Spans in the map are UTF-8 byte offsets, as the program's reader makes them; a text
// area's offsets are UTF-16 code units, so the lookups that take a caret take the text too.

import { byteToIndex } from './spans.js';

/**
 * @typedef {{ id: string, kind: 'line' | 'point' | 'region' | 'fill' | 'square', name: string, index: number | null,
 *   span: [number, number] | null, at: [number, number] | null, parent: string | null, text: string | null }} Mark
 * @typedef {'off' | 'shown' | 'lit'} MarkState
 * @typedef {{ toggles?: Set<string>, pinned?: Set<string>, hover?: string | null, caret?: string | null }} Moment
 */

/** The toggles the page offers, in the order it shows them, and the kinds of mark each one switches. */
export const TOGGLES = /** @type {const} */ ([
  ['lines', 'Lines', ['line']],
  ['points', 'Points', ['point', 'fill']],
  ['regions', 'Regions', ['region']],
  ['squares', 'Squares', ['square']],
]);

/** @param {any} s @returns {[number, number] | null} */
const spanOf = (s) => (Array.isArray(s) && s.length === 2 ? [s[0], s[1]] : null);

/**
 * Every mark of a map: a line and each of its points, a region and each of its fill points, each named point.  A point's `at`
 * is where it is, in squares, or null when the build did not place it.
 * @param {any} map the program's map JSON (null: no marks)
 * @returns {Mark[]}
 */
export function marksOf(map) {
  /** @type {Mark[]} */
  const out = [];
  if (!map) return out;
  for (const l of map.lines ?? []) {
    out.push({ id: `line:${l.name}`, kind: 'line', name: l.name, index: null, span: spanOf(l.span), at: null, parent: null, text: null });
    (l.points ?? []).forEach((/** @type {any} */ p, /** @type {number} */ k) => out.push({
      id: `point:${l.name}#${k}`, kind: 'point', name: l.name, index: k, span: spanOf(p.span), at: p.at ?? null, parent: `line:${l.name}`, text: p.text ?? null,
    }));
  }
  for (const r of map.regions ?? []) {
    out.push({ id: `region:${r.name}`, kind: 'region', name: r.name, index: null, span: spanOf(r.span), at: null, parent: null, text: null });
    (r.fills ?? []).forEach((/** @type {any} */ p, /** @type {number} */ k) => out.push({
      id: `fill:${r.name}#${k}`, kind: 'fill', name: r.name, index: k, span: spanOf(p.span), at: p.at ?? null, parent: `region:${r.name}`, text: p.text ?? null,
    }));
  }
  for (const p of map.points ?? []) {
    out.push({ id: `point:${p.name}`, kind: 'point', name: p.name, index: null, span: spanOf(p.span), at: p.at ?? null, parent: null, text: null });
  }
  return out;
}

/**
 * The state of `mark` at this `moment`: lit when it is pinned, hovered or under the caret; shown when its kind is switched on
 * or the mark it belongs to is lit (a lit line shows its points); else off.
 * @param {Mark} mark @param {Moment} moment @returns {MarkState}
 */
export function stateOf(mark, moment) {
  const { toggles = new Set(), pinned = new Set(), hover = null, caret = null } = moment;
  if (pinned.has(mark.id) || hover === mark.id || caret === mark.id) return 'lit';
  if (mark.parent && (pinned.has(mark.parent) || hover === mark.parent || caret === mark.parent)) return 'shown';
  for (const [name, , kinds] of TOGGLES) if (toggles.has(name) && /** @type {readonly string[]} */ (kinds).includes(mark.kind)) return 'shown';
  return 'off';
}

/**
 * Every mark with its state, off ones left out: what a renderer draws.
 * @param {Mark[]} marks @param {Moment} moment @returns {{ mark: Mark, state: 'shown' | 'lit' }[]}
 */
export function drawn(marks, moment) {
  /** @type {{ mark: Mark, state: 'shown' | 'lit' }[]} */
  const out = [];
  for (const mark of marks) {
    const state = stateOf(mark, moment);
    if (state !== 'off') out.push({ mark, state });
  }
  return out;
}

/**
 * The byte offset (what the map's spans count) of a UTF-16 offset in `text`.
 * @param {string} text @param {number} at
 */
export function byteOffset(text, at) {
  return new TextEncoder().encode(text.slice(0, Math.max(0, at))).length;
}

/**
 * The mark whose source `offset` (UTF-16, in `text`) is in: the narrowest, so a point wins over the line it is written in,
 * and a line over nothing.  Null when the offset is in no mark's source.  This is what the caret and the pointer over the source
 * ask; it works from the map and an offset, so it finds a single point, not just a line.
 * @param {Mark[]} marks @param {string} text @param {number} offset @returns {string | null}
 */
export function markAtOffset(marks, text, offset) {
  const at = byteOffset(text, offset);
  let best = null, size = Infinity;
  for (const m of marks) {
    if (!m.span) continue;
    const [from, len] = m.span;
    // the end belongs to the mark too: a caret just after a point is still on it
    if (at < from || at > from + len) continue;
    if (len < size) { size = len; best = m.id; }
  }
  return best;
}

/**
 * A mark's source as a UTF-16 range in `text`, for the source pane to light.
 * @param {Mark | undefined} mark @param {string} text @returns {[number, number] | null}
 */
export function sourceRange(mark, text) {
  if (!mark?.span) return null;
  const [from, len] = mark.span;
  return [byteToIndex(text, from), byteToIndex(text, from + len)];
}

/**
 * The marks a page's `highlight="line shore, point 1, region pool"` pins.  Words: `line NAME`, `region NAME`, `point N` (the
 * Nth point, from 1, of the first line; `point NAME N` of the line NAME), `fill N` / `fill NAME N` likewise of a region's fill
 * points, `point NAME` for a point the tile names.  A word that names no mark of the map is an error, said in words: a pin that
 * pins nothing would be a lie.
 * @param {string} spec @param {Mark[]} marks @returns {{ ids: Set<string>, errors: string[] }}
 */
export function parsePins(spec, marks) {
  const ids = new Set();
  const errors = [];
  for (const part of spec.split(',').map((s) => s.trim()).filter(Boolean)) {
    const [kind, ...rest] = part.split(/\s+/);
    const num = rest.length && /^\d+$/.test(rest[rest.length - 1]) ? Number(rest.pop()) : null;
    const name = rest.join(' ') || null;
    let found = null;
    if (kind === 'line' || kind === 'region') {
      found = marks.find((m) => m.kind === kind && m.name === name);
    } else if (kind === 'point' || kind === 'fill') {
      const owner = marks.find((m) => (kind === 'point' ? m.kind === 'line' : m.kind === 'region') && (name ? m.name === name : true));
      if (num != null && owner) found = marks.find((m) => m.kind === kind && m.name === owner.name && m.index === num - 1);
      else if (num == null && name) found = marks.find((m) => m.kind === 'point' && m.index == null && m.name === name);
    }
    if (found) ids.add(found.id);
    else errors.push(`highlight "${part}" names nothing in this tile`);
  }
  return { ids, errors };
}
