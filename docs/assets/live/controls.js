// @ts-check
// The controls a card shows (`controls="wobble, seed"`): a slider or a roll button for a value the source writes.  A control
// asks the program to change the source, never makes text itself: it sends a typed edit (`setOp`: the node's id, the control's name and the
// value) to the engine's `edit_tile`, which is the editor's own `dt_service::edit::apply`, so the comments and layout of the
// source survive and the marks in it stay true.  The slider reads its value as the program's map writes it (`map.lines[].sheet.controls`,
// `map.tile.controls`: name, text as written), so typing in the source moves it after the next check.  Pure: no DOM.

import { idAtByte } from '../../editor/js/rows.js';

/** What a slider covers when the page does not say (`controls="wobble:0..12"`): [min, max, step]. */
/** @type {Record<string, [number, number, number]>} */
const RANGES = { wobble: [0, 12, 0.5], width: [2, 30, 1], seed: [1, 999, 1] };

/**
 * `"wobble, width:4..30, seed"` as [{name, min, max}] (min and max null: the default for that name).
 * @param {string} spec
 */
export function parseControls(spec) {
  return spec.split(',').map((s) => s.trim()).filter(Boolean).map((part) => {
    const [name, range] = part.split(':');
    const m = range && /^(-?[\d.]+)\.\.(-?[\d.]+)$/.exec(range.trim());
    return { name: name.trim(), min: m ? Number(m[1]) : null, max: m ? Number(m[2]) : null };
  });
}

/**
 * Where `name` is written in the map: `{text, id, owner}` for the first line (or the tile itself) that writes it, else null.
 * `text` is the value as written, quotes and all; `id` is the program's id of the node that writes it (from the check's `nodes`),
 * which an edit names it by.
 * @param {any} map @param {{ id: string, at: number, len: number }[] | null | undefined} nodes the check's `nodes` @param {string} name
 */
export function findControl(map, nodes, name) {
  if (!map) return null;
  for (const l of map.lines ?? []) {
    const c = l.sheet?.controls?.find((/** @type {any} */ k) => k.name === name);
    const id = c && idAtByte(nodes, l.sheet.at);
    if (c && id) return { text: c.text, id, owner: `line ${l.name}` };
  }
  const own = map.tile?.controls?.find((/** @type {any} */ k) => k.name === name);
  const id = own && idAtByte(nodes, map.tile.at);
  return own && id ? { text: own.text, id, owner: 'tile' } : null;
}

/**
 * A written value as a slider reads it: `"5dmm"` is 5 with unit `dmm`; `97` is 97 with none.
 * @param {string} text @returns {{ value: number, unit: string, quoted: boolean } | null}
 */
export function readValue(text) {
  const quoted = text.startsWith('"') && text.endsWith('"') && text.length >= 2;
  const m = /^(-?\d+(?:\.\d+)?)\s*([a-z%]*)$/.exec((quoted ? text.slice(1, -1) : text).trim());
  return m ? { value: Number(m[1]), unit: m[2], quoted } : null;
}

/** The slider's range for a control: the page's, else the name's own, else from zero to twice the value (at least 10). */
/** @param {{ name: string, min: number | null, max: number | null }} c @param {number} current */
export function rangeOf(c, current) {
  const d = RANGES[c.name];
  const min = c.min ?? d?.[0] ?? 0;
  const max = c.max ?? d?.[1] ?? Math.max(10, current * 2);
  return { min, max, step: d?.[2] ?? (max - min <= 20 ? 0.5 : 1) };
}

/**
 * The edit a slider asks of the program: set the control `name` of the node `at.id` to `value`, keeping the unit it is written
 * in.  A typed `EditReq` (the program's `dt_service::edit`): it names the node by id, carries the fingerprint of the text the id
 * was read at (`read`), and holds no KDL.  The program makes the text (its quotes, its spans) and says it back.
 * @param {string} text the source @param {string} read the fingerprint of `text` @param {{ id: string, name: string, unit: string }} at @param {number} value
 * @returns {import('./engine-wire').EditReq}
 */
export function setOp(text, read, at, value) {
  const number = Number.isInteger(value) ? value : Math.round(value * 100) / 100;
  return { read, op: 'set', text, id: at.id, name: at.name, value: at.unit ? `${number}${at.unit}` : number };
}
