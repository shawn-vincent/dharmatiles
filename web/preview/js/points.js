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

/** The box between points `a` and `b` as a point drawn from ranges, quotes included. */
export function rangeText(a, b, rows, format = 0) {
  const x = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const y = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  return format >= 1
    ? `"(${round(x[0])}sq..${round(x[1])}sq, ${round(rows - y[1])}sq..${round(rows - y[0])}sq)"`
    : `"${round(x[0])}..${round(x[1])},${round(y[0])}..${round(y[1])}"`;
}

/** The point as a label (no quotes), in the document's own words. */
export function pointLabel(q, rows, format = 0) {
  return pointText(q, rows, format).slice(1, -1);
}
