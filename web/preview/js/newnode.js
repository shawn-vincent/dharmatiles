// A new node as the typed parts the program writes (`NewNode`, api.d.ts), never as text.  A tool says what the node is called,
// what it holds and where its points are; the program's `make` edit writes the words, the quotes and the points.

/** @typedef {import('./api').NewNode} NewNode */
/** @typedef {import('./api').NewValue} NewValue */
/** @typedef {import('./api').NewControl} NewControl */

/** A name or word of the language. @param {string} w @returns {NewValue} */
export const word = (w) => ({ word: w });
/** A point, squares from the south-west corner. @param {number[]} q @returns {NewValue} */
export const point = (q) => ({ point: [q[0], q[1]] });

/**
 * A node of these parts.
 * @param {string} name
 * @param {{ args?: NewValue[], controls?: NewControl[], children?: NewNode[], spread?: boolean }} [parts]
 * @returns {NewNode}
 */
export function node(name, { args = [], controls = [], children = [], spread = false } = {}) {
  return { name, args, controls, children, spread };
}

/** One word the user typed in the bar, as the value it is. @param {string} t @returns {NewValue} */
function valueOf(t) {
  const quoted = t.match(/^"(.*)"$/);
  if (quoted) return { text: quoted[1] };
  if (t === 'true' || t === 'false') return { bool: t === 'true' };
  if (/^-?\d+$/.test(t)) return { int: Number(t) };
  if (/^-?\d*\.\d+$/.test(t)) return { num: Number(t) };
  return word(t);
}

/**
 * What the user typed after an element's name in the bar (`size=6..10 big`): words and `name=value` controls.
 * @param {string} extra
 * @returns {{ args: NewValue[], controls: NewControl[] }}
 */
export function parseExtra(extra) {
  /** @type {NewValue[]} */
  const args = [];
  /** @type {NewControl[]} */
  const controls = [];
  for (const m of extra.matchAll(/([A-Za-z][\w-]*)=("[^"]*"|\S+)|"[^"]*"|\S+/g)) {
    if (m[1]) controls.push({ name: m[1], value: valueOf(m[2]) });
    else args.push(valueOf(m[0]));
  }
  return { args, controls };
}
