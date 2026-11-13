// A control's value as the sheet reads and writes it: the helpers behind view/sheet.js (the rows, the scrub and the typed
// value) and view/property.js.  No page here.

import { kdlString, unquote } from './value.js';

/** Does a written value carry its own unit (`3.5dmm`, `20%`, `45deg`)? */
export const carriesUnit = (shown) => /\d\s*[a-zA-Z%°]/.test(String(shown ?? ''));

export { kdlString, unquote };

/**
 * The value part of a written control and where it starts in its text.
 * The server gives the value's own span (`"6..10"`); a whole `name=value`
 * or child line `name value` is taken apart too.
 */
export function valueOf(text, name) {
  if (text.startsWith(`${name}=`)) return { at: name.length + 1, text: text.slice(name.length + 1) };
  const m = text.match(new RegExp(`^${name.replace(/[.+-]/g, '\\$&')}\\s+`));
  if (!m) return { at: 0, text };
  const v = text.slice(m[0].length).match(/^(#?"(?:[^"\\]|\\.)*"#?|[^\s{;]+)/);
  return { at: m[0].length, text: v ? v[0] : '' };
}

/** A typed value as JSON for the server: a number, a switch, or a string (quotes taken off). */
export function parseValue(raw) {
  const t = raw.trim();
  if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
  if (t === '#true' || t === 'true') return true;
  if (t === '#false' || t === 'false') return false;
  return t.replace(/^#?"|"#?$/g, '');
}

/** Button choices stay direct only when they are certain to fit on one row. */
export function wordControlNeedsSelect(words) {
  return words.length > 2 || words.reduce((n, word) => n + word.word.length, 0) > 18;
}
