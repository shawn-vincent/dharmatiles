// How a point on the tile is shown in the place tool's label.  The page works in
// squares from the south-west corner, as the program's geometry does; a
// document counts from the north-west corner and writes units:
// `(0.5sq, 0.25sq)`.  This is a label only: the page never writes a point into
// a document.  A tool puts a point in a typed edit (`point([x, y])`,
// newnode.js) and the program writes it.

const round = (/** @type {number} */ v) => String(Math.round(v * 100) / 100);

/** The point as a label, in the document's own words. */
/** @param {number[]} q @param {number} rows */
export function pointLabel(q, rows) {
  return `(${round(q[0])}sq, ${round(rows - q[1])}sq)`;
}
