// Lines a few pixels wide, whatever the zoom.  WebGL draws a line one pixel
// thick, which is too thin to read over a tile's surface, so a segment is
// drawn as a quad: its two ends, pushed sideways in the screen by half the
// width (the vertex shader does the pushing; this builds the vertices).

/**
 * The vertices of `n` segments as quads.
 * `pos` is six numbers a segment (end A, end B, in mm); `rgba` is eight
 * (the colour and opacity at A, then at B).  Each segment is four vertices:
 * A and B twice, with the side each is pushed to, and six indices.
 */
export function fatGeometry(pos, rgba) {
  const n = Math.floor(pos.length / 6);
  const position = new Float32Array(12 * n), other = new Float32Array(12 * n);
  const side = new Float32Array(4 * n), color = new Float32Array(16 * n);
  const index = new Uint32Array(6 * n);
  // the shader measures the direction from each vertex to its `other`, so at
  // B the direction is turned round: its sides are swapped to stay on the
  // same side of the segment as A's
  const sides = [1, -1, -1, 1];
  for (let k = 0; k < n; k++) {
    const a = pos.subarray(6 * k, 6 * k + 3), b = pos.subarray(6 * k + 3, 6 * k + 6);
    for (let v = 0; v < 4; v++) {
      const at = v < 2 ? a : b, to = v < 2 ? b : a;
      position.set(at, 12 * k + 3 * v);
      other.set(to, 12 * k + 3 * v);
      side[4 * k + v] = sides[v];
      color.set(rgba.subarray(8 * k + (v < 2 ? 0 : 4), 8 * k + (v < 2 ? 4 : 8)), 16 * k + 4 * v);
    }
    index.set([4 * k, 4 * k + 1, 4 * k + 2, 4 * k + 1, 4 * k + 3, 4 * k + 2], 6 * k);
  }
  return { position, other, side, color, index, n };
}

export const FAT_VERT = `attribute vec3 other; attribute float side; attribute vec4 color;
uniform vec2 uRes; uniform float uWidth; varying vec4 vC;
void main() {
  vec4 a = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  vec4 b = projectionMatrix * modelViewMatrix * vec4(other, 1.0);
  vec2 sa = a.xy / a.w * uRes * 0.5, sb = b.xy / b.w * uRes * 0.5;
  vec2 d = sb - sa;
  d = length(d) < 1e-4 ? vec2(0.0, 1.0) : normalize(d);
  vec2 off = vec2(-d.y, d.x) * side * uWidth * 0.5;
  gl_Position = vec4(a.xy + off / (uRes * 0.5) * a.w, a.z, a.w);
  vC = color;
}`;

export const FAT_FRAG = `varying vec4 vC; void main() { gl_FragColor = vC; }`;
