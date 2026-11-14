// The controls a card shows (`controls="wobble, seed"`): a slider or a roll button for a value the source writes.  A control
// writes the source, never a copy of it: it splices its number into the span the program's map gives for that value
// (`map.lines[].sheet.controls`, `map.tile.controls`: name, text as written, byte span), so the comments and layout of the
// source survive and the marks in it stay true.  Typing in the source moves the slider, because the slider reads its value from
// the same span after every check.  Pure: no DOM.

import { byteToIndex } from '../../editor/js/spans.js';

/** What a slider covers when the page does not say (`controls="wobble:0..12"`): [min, max, step]. */
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
 * Where `name` is written in the map: `{text, span, owner}` for the first line (or the tile itself) that writes it, else null.
 * `text` is the value as written, quotes and all.
 * @param {any} map @param {string} name
 */
export function findControl(map, name) {
  if (!map) return null;
  for (const l of map.lines ?? []) {
    const c = l.sheet?.controls?.find((/** @type {any} */ k) => k.name === name);
    if (c) return { text: c.text, span: c.span, owner: `line ${l.name}` };
  }
  const own = map.tile?.controls?.find((/** @type {any} */ k) => k.name === name);
  return own ? { text: own.text, span: own.span, owner: 'tile' } : null;
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
export function rangeOf(c, current) {
  const d = RANGES[c.name];
  const min = c.min ?? d?.[0] ?? 0;
  const max = c.max ?? d?.[1] ?? Math.max(10, current * 2);
  return { min, max, step: d?.[2] ?? (max - min <= 20 ? 0.5 : 1) };
}

/**
 * `source` with the control's value written as `value`: the number is put over the old value's span, keeping its unit and its
 * quotes.  Returns the new text and the value's new span (bytes), so a run of changes before the next check stays true.
 * @param {string} source @param {{ text: string, span: [number, number] }} at @param {number} value
 */
export function writeValue(source, at, value) {
  const read = readValue(at.text);
  if (!read) throw new Error(`cannot read the value ${at.text} to change it`);
  const number = Number.isInteger(value) ? String(value) : String(Math.round(value * 100) / 100);
  const written = read.quoted ? `"${number}${read.unit}"` : `${number}${read.unit}`;
  const from = byteToIndex(source, at.span[0]);
  const to = byteToIndex(source, at.span[0] + at.span[1]);
  const bytes = (s) => new TextEncoder().encode(s).length;
  return { text: source.slice(0, from) + written + source.slice(to), span: [at.span[0], bytes(written)], written };
}
