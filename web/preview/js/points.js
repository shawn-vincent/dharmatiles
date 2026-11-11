// How a point on the tile is written in the document.  The page works in
// squares from the south-west corner, as the program's geometry does; a
// format 0 document writes that as `"x,y"`, a format 1 document counts from
// the north-west corner and writes units: `"(0.5sq, 0.25sq)"`.  Nothing here
// reads a point back: the program's map gives where each written point is.

const round = (v) => String(Math.round(v * 100) / 100);

/** The point `q` (squares from the south-west corner) as it is written, quotes included. */
export function pointText(q, rows, format = 0) {
  return format >= 1 ? `"(${round(q[0])}sq, ${round(rows - q[1])}sq)"` : `"${round(q[0])},${round(q[1])}"`;
}

/**
 * What a placed thing writes for its place: `at=` and one point.  A thing's
 * `at=` takes exactly one point (a range there is `dt::place`), so a drag
 * writes where it ended, as a click does.
 */
export function placeText(q, rows, format = 0) {
  return `at=${pointText(q, rows, format)}`;
}

/** The point as a label (no quotes), in the document's own words. */
export function pointLabel(q, rows, format = 0) {
  return pointText(q, rows, format).slice(1, -1);
}
