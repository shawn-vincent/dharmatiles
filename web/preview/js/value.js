// @ts-check
// A control's value as the sheet edits it: what may be typed, what it is
// shaped as, and what can go at the caret.  The page never writes KDL: the program does.  No page here;
// the sheet draws it (view/sheet.js).  Nothing here checks a value: the program's
// check (dt_doc's `check_text`, by `/api/check-value`) is the only one.

/** Whether a control's value is a number or an expression: the program says so (`numeric`) for every control it describes. */
export const isNumeric = (/** @type {{ numeric?: boolean }} */ c) => c.numeric === true;

/** A written KDL string's text: quotes off, escapes undone. */
export function unquote(/** @type {string} */ written) {
  const t = written.trim();
  const raw = t.match(/^(#+)"([\s\S]*)"\1$/);
  if (raw) return raw[2];
  if (!/^"[\s\S]*"$/.test(t)) return t;
  return t.slice(1, -1).replace(/\\(u\{([0-9a-fA-F]+)\}|[\s\S])/g, (/** @type {string} */ _, /** @type {string} */ e, /** @type {string | undefined} */ hex) => (hex ? String.fromCodePoint(parseInt(hex, 16)) : ((/** @type {Record<string, string>} */ ({ n: '\n', r: '\r', t: '\t', s: ' ' }))[e] ?? e)));
}

// what a typed value is, to say which forms it can switch between; the program writes it (a typed value is sent as a number, a switch or words)
const NUMBER = /^(\(\w+\))?-?\d+(\.\d+)?$/;
// format 1 writes a unit after a number (`8dmm`, `0.5sq`, `40deg`, `25%`); the unit words are the program's (`setUnits`, from its language table)
let UNITED = /^$/, UNITED_RANGE = /^$/;
/** Take the unit words the program's language has (`dmm`, `sq`, …); `%` is always a unit after a number. */
/** @param {Iterable<string>} units */
export function setUnits(units) {
  const unit = `(?:${[...units, '%'].map((u) => u.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})?`;
  UNITED = new RegExp(`^-?\\d+(\\.\\d+)?${unit}$`);
  UNITED_RANGE = new RegExp(`^-?\\d+(\\.\\d+)?${unit}\\s*\\.\\.\\s*-?\\d+(\\.\\d+)?${unit}$`);
}
const WORD = /^[A-Za-z_][\w-]*$/;

/** What a typed value is: empty, a number, a range, a word or an expression. */
/** @param {string} typed */
export function formOf(typed) {
  const t = typed.trim();
  if (!t) return 'empty';
  if (UNITED.test(t) || NUMBER.test(t)) return 'number';
  if (UNITED_RANGE.test(t)) return 'range';
  if (WORD.test(t)) return 'word';
  return 'expression';
}

/** The typed value as one number, or as a range from it (`6` ↔ `6..6`). */
/** @param {string} typed */
export function asRange(typed) {
  const t = typed.trim();
  return formOf(t) === 'number' ? `${t}..${t}` : t;
}
/** @param {string} typed */
export function asNumber(typed) {
  const t = typed.trim();
  return formOf(t) === 'range' ? t.split('..')[0].trim() : t;
}

/**
 * What can go at `caret` in `typed`: {from, to, items: [{label, insert, about, kind}]}.
 * The control's words while nothing but a word is typed.
 * @param {string} typed @param {number} caret @param {{ words?: { word: string, about?: string }[] }} c
 */
export function valueItems(typed, caret, c) {
  const before = typed.slice(0, caret);
  const word = (before.match(/[A-Za-z_][\w-]*$/) || [''])[0];
  // a name starts afresh: not straight after a number (`(h)5`, `1e`)
  const from = caret - word.length;
  if (/[\w.)%]$/.test(before.slice(0, from))) return { from, to: caret, items: [] };
  /** @type {{ label: string, insert: string, about?: string, kind: string }[]} */
  let items = [];
  // nothing typed but a word (or not even that): the control's words
  const t = typed.trim();
  if (!t || WORD.test(t)) items.push(...(c.words || []).map((/** @type {{ word: string, about?: string }} */ w) => ({ label: w.word, insert: w.word, about: w.about, kind: 'word' })));
  items = items.filter((i) => i.label.startsWith(word) && i.label !== word);
  return { from, to: caret, items };
}

/**
 * `fn` run once, `ms` after the last call (with that call's arguments);
 * `.cancel()` drops a run that is waiting.  The value helper asks the
 * program about what is typed through this, so a burst of keys is one
 * request.
 * @template {any[]} A
 * @param {(...args: A) => void} fn @param {number} ms
 * @param {{ set: (f: () => void, ms: number) => unknown, clear: (id: any) => void }} [timers]  the clock, which a test replaces
 */
export function debounce(fn, ms, timers = { set: (f, t) => setTimeout(f, t), clear: (id) => clearTimeout(id) }) {
  /** @type {unknown} */
  let id = null;
  const call = (/** @type {A} */ ...args) => {
    if (id !== null) timers.clear(id);
    id = timers.set(() => { id = null; fn(...args); }, ms);
  };
  call.cancel = () => { if (id !== null) timers.clear(id); id = null; };
  return call;
}
