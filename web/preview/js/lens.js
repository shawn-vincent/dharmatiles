// A control's value as the sheet reads and writes it: the helpers behind view/sheet.js (the rows, the scrub and the typed
// value) and view/property.js.  No page here.


/** Does a written value carry its own unit (`3.5dmm`, `20%`, `45deg`)? */
export const carriesUnit = (/** @type {unknown} */ shown) => /\d\s*[a-zA-Z%°]/.test(String(shown ?? ''));


/** A typed value as JSON for the server: a number, a switch, or the words typed (the program writes the KDL, quotes included). */
/** @param {string} raw */
export function parseValue(raw) {
  const t = raw.trim();
  if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
  if (t === 'true') return true;
  if (t === 'false') return false;
  return t;
}

/** Button choices stay direct only when they are certain to fit on one row. */
/** @param {{ word: string }[]} words */
export function wordControlNeedsSelect(words) {
  return words.length > 2 || words.reduce((n, word) => n + word.word.length, 0) > 18;
}
