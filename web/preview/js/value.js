// A control's value as the sheet edits it: what may be typed, what it is
// written as, and what can go at the caret.  No page here;
// the lens draws it (lens.js).  Nothing here checks a value: the program's
// check (dt_doc's `check_text`, by `/api/check-value`) is the only one.

/** The units whose values are numbers or expressions. */
const NUMERIC = new Set(['mm', 'degrees', 'percent', 'count', 'per square', 'number']);
export const isNumeric = (c) => NUMERIC.has(c.unit);

/** `text` as a KDL v2 string. */
export function kdlString(text) {
  const esc = { '"': '\\"', '\\': '\\\\', '\n': '\\n', '\r': '\\r', '\t': '\\t' };
  return `"${text.replace(/["\\\n\r\t\u0000-\u001f\u007f]/g, (ch) => esc[ch] ?? `\\u{${ch.charCodeAt(0).toString(16)}}`)}"`;
}

/** A written KDL string's text: quotes off, escapes undone. */
export function unquote(written) {
  const t = written.trim();
  const raw = t.match(/^(#+)"([\s\S]*)"\1$/);
  if (raw) return raw[2];
  if (!/^"[\s\S]*"$/.test(t)) return t;
  return t.slice(1, -1).replace(/\\(u\{([0-9a-fA-F]+)\}|[\s\S])/g, (_, e, hex) => (hex ? String.fromCodePoint(parseInt(hex, 16)) : ({ n: '\n', r: '\r', t: '\t', s: ' ' }[e] ?? e)));
}

// what KDL takes without quotes: a number (with a unit, `(h)1.2`), a word,
// a switch; a word that KDL reads as something else is quoted
const NUMBER = /^(\(\w+\))?-?\d+(\.\d+)?$/;
// format 1 writes a unit after a number (`8dmm`, `0.5sq`, `40deg`, `25%`); the program says which unit a control takes
const UNIT = '(?:dmm|mm|sq|in|layer|deg|rad|%)?';
const UNITED = new RegExp(`^-?\\d+(\\.\\d+)?${UNIT}$`);
const UNITED_RANGE = new RegExp(`^-?\\d+(\\.\\d+)?${UNIT}\\s*\\.\\.\\s*-?\\d+(\\.\\d+)?${UNIT}$`);
const WORD = /^[A-Za-z_][\w-]*$/;
const KEYWORD = new Set(['true', 'false', 'null', 'inf', 'nan']);

/** A typed value as it goes in the text: bare when KDL takes it bare, else a string. */
export function written(typed) {
  const t = typed.trim();
  if (!t || NUMBER.test(t) || t === '#true' || t === '#false') return t;
  if (WORD.test(t) && !KEYWORD.has(t)) return t;
  return kdlString(t);
}

/** What a typed value is: empty, a number, a range, a word or an expression. */
export function formOf(typed) {
  const t = typed.trim();
  if (!t) return 'empty';
  if (UNITED.test(t) || NUMBER.test(t)) return 'number';
  if (UNITED_RANGE.test(t)) return 'range';
  if (WORD.test(t)) return 'word';
  return 'expression';
}

/** The typed value as one number, or as a range from it (`6` ↔ `6..6`). */
export function asRange(typed) {
  const t = typed.trim();
  return formOf(t) === 'number' ? `${t}..${t}` : t;
}
export function asNumber(typed) {
  const t = typed.trim();
  return formOf(t) === 'range' ? t.split('..')[0].trim() : t;
}

/**
 * What can go at `caret` in `typed`: {from, to, items: [{label, insert, about, kind}]}.
 * The control's words while nothing but a word is typed.
 */
export function valueItems(typed, caret, c) {
  const before = typed.slice(0, caret);
  const word = (before.match(/[A-Za-z_][\w-]*$/) || [''])[0];
  // a name starts afresh: not straight after a number (`(h)5`, `1e`)
  const from = caret - word.length;
  if (/[\w.)%]$/.test(before.slice(0, from))) return { from, to: caret, items: [] };
  let items = [];
  // nothing typed but a word (or not even that): the control's words
  const t = typed.trim();
  if (!t || WORD.test(t)) items.push(...(c.words || []).map((w) => ({ label: w.word, insert: w.word, about: w.about, kind: 'word' })));
  items = items.filter((i) => i.label.startsWith(word) && i.label !== word);
  return { from, to: caret, items };
}

/**
 * `fn` run once, `ms` after the last call (with that call's arguments);
 * `.cancel()` drops a run that is waiting.  The value helper asks the
 * program about what is typed through this, so a burst of keys is one
 * request.
 */
export function debounce(fn, ms, timers = { set: (f, t) => setTimeout(f, t), clear: (id) => clearTimeout(id) }) {
  let id = null;
  const call = (...args) => {
    if (id !== null) timers.clear(id);
    id = timers.set(() => { id = null; fn(...args); }, ms);
  };
  call.cancel = () => { if (id !== null) timers.clear(id); id = null; };
  return call;
}
