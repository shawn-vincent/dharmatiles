/* vec.js — owns the small math every module shares: 3-vectors as arrays, smooth max, smooth step. No dependencies. */
var Stone = Stone || {};
(function (S) {
  const sqrt = Math.sqrt, abs = Math.abs, max = Math.max;
  const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
  const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
  const mul = (a, k) => [a[0] * k, a[1] * k, a[2] * k];
  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const len = (a) => sqrt(dot(a, a));
  const norm = (a) => { const l = len(a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };
  // any unit vector perpendicular to n (the helper axis is the one n is least aligned with)
  const perp = (n) => norm(cross(n, abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0]));
  // rotate v about the unit axis by ang (Rodrigues)
  const rotate = (v, ax, ang) => { const c = Math.cos(ang), s = Math.sin(ang), d = dot(ax, v) * (1 - c), x = cross(ax, v); return [v[0] * c + x[0] * s + ax[0] * d, v[1] * c + x[1] * s + ax[1] * d, v[2] * c + x[2] * s + ax[2] * d]; };
  const clamp = (x, a, b) => (x < a ? a : x > b ? b : x);
  // polynomial smooth max: equals max(a,b) when they differ by k or more, and lies at most k/4 above it. k <= 0 is the hard max.
  const smax = (a, b, k) => { const m = a > b ? a : b; if (k <= 1e-6) return m; const h = max(k - abs(a - b), 0) / k; return m + h * h * k * 0.25; };
  const sstep = (a, b, x) => { const t = clamp((x - a) / (b - a), 0, 1); return t * t * (3 - 2 * t); };
  S.vec = { add, sub, mul, dot, cross, len, norm, perp, rotate, clamp, smax, sstep };
})(Stone);
