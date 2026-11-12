// The viewer's constants: the shaders, the tuning numbers, the named views.

import * as THREE from 'three';

THREE.ColorManagement.enabled = false; // colours are already display values

// [name, label, elevation deg, azimuth deg (round from +x), close-up?]
// painted/top/low/close are the contact sheet's four views.
export const VIEWS = [
  ['painted', 'Iso', 40, -120, false],
  ['top', 'Top', 89, -90, false],
  ['front', 'Front', 0, -90, false],
  ['right', 'Right', 0, 0, false],
  ['back', 'Back', 0, 90, false],
  ['left', 'Left', 0, 180, false],
  ['low', 'Low', 14, -150, false],
  ['close', 'Close', 38, -125, true],
  ['under', 'Under', -60, -120, false],
];

export const FOV = 40;
export const GRID_BELOW_MM = 0.02;
// ink lines on the plate: bold at each square, fine at a fifth of one, faded with distance
export const GRID_FRAG = `
uniform float uDim; uniform float uSq; uniform vec2 uCentre; uniform float uR0; uniform float uR1; uniform vec3 uInk;
varying vec2 vXY;
float line(float v, float px, float w) {
  float d = abs(fract(v - 0.5) - 0.5) / px; // distance to the nearest line, in pixels
  return 1.0 - smoothstep(w - 0.5, w + 0.5, d);
}
void main() {
  vec2 g = vXY / uSq;
  vec2 px = max(fwidth(g), vec2(1e-5));
  float major = max(line(g.x, px.x, 1.1), line(g.y, px.y, 1.1));
  vec2 f = g * 5.0;
  float minor = max(line(f.x, px.x * 5.0, 0.6), line(f.y, px.y * 5.0, 0.6));
  float fade = 1.0 - smoothstep(uR0, uR1, distance(vXY, uCentre));
  float a = max(major * 0.42, minor * 0.13) * fade * uDim;
  if (a < 0.01) discard;
  gl_FragColor = vec4(uInk, a);
}`;
export const NARROW_FOV = 3; // the near-orthographic end of the projection tween
export const easeInOutCubic = (u) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2);
export const reduceMotion = () => {
  try { return matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; /* no media queries: motion is not reduced */ }
};
export const rad = (d) => (d * Math.PI) / 180;
export const dirv = (elev, azim) => new THREE.Vector3(
  Math.cos(rad(elev)) * Math.cos(rad(azim)), Math.cos(rad(elev)) * Math.sin(rad(azim)), Math.sin(rad(elev)));

export const VERT = `
attribute vec3 paint; attribute vec2 who;
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
void main() {
  vN = normal; vC = paint; vP = position; vWho = who;
  gl_Position = projectionMatrix * viewMatrix * vec4(position, 1.0);
}`;

// The render.rs shading: 0.3 + key + 0.35 fill, *0.95, gamma 1/1.25,
// back faces turned to face the camera.
// The selection and the thing under the pointer are tinted by the theme accent. `who`
// stores (entry + 1, piece), with entry 0 meaning none; the uniforms keep
// ordinary zero-based entries and a piece of -1 means the whole entry.
export const FRAG = `
uniform vec3 uCam; uniform vec3 uDir; uniform float uOrtho; uniform vec3 uL1; uniform vec3 uL2;
uniform vec2 uSel; uniform vec2 uHov; uniform vec3 uAccent; uniform float uTintSel;
uniform vec2 uKeep; uniform vec2 uLeave; uniform vec2 uLift; uniform vec2 uDrop;
uniform float uKeepA; uniform float uLeaveA; uniform float uLiftA; uniform float uDropA;
uniform float uMode; uniform float uGhost; // 0 all; 2 the rest, faded; 3 all, faded; 4 what stands out of the fade
varying vec3 vN; varying vec3 vC; varying vec3 vP; varying vec2 vWho;
float is(vec2 w) {
  vec2 v = floor(vWho + 0.5);
  return (w.x >= 0.0 && v.x == w.x + 1.0 && (w.y < 0.0 || v.y == w.y)) ? 1.0 : 0.0;
}
void main() {
  // what stands out of the fade, each at its own alpha, the first that
  // holds winning: the selection (uKeep), the thing it just left
  // (uLeave), the thing under the pointer (uLift), the thing it just left (uDrop)
  float keep = is(uKeep);
  float leave = is(uLeave) * (1.0 - keep);
  float lift = is(uLift) * (1.0 - keep) * (1.0 - leave);
  float drop = is(uDrop) * (1.0 - keep) * (1.0 - leave) * (1.0 - lift);
  float out_ = keep + leave + lift + drop;
  if (uMode > 1.5 && uMode < 2.5 && out_ > 0.5) discard;
  if (uMode > 3.5 && out_ < 0.5) discard;
  vec3 n = normalize(vN);
  vec3 ray = uOrtho > 0.5 ? uDir : vP - uCam;
  if (dot(n, ray) > 0.0) n = -n;
  float shade = 0.3 + max(dot(n, uL1), 0.0) + 0.35 * max(dot(n, uL2), 0.0);
  vec3 c = pow(clamp(vC * shade * 0.95, 0.0, 1.0), vec3(1.0 / 1.25));
  // the fade marks the selection; without it, the tint does
  float k = max(is(uSel) * 0.42, is(uHov) * 0.22) * uTintSel;
  c = mix(c, uAccent * (0.55 + 0.45 * shade), k);
  float a = uMode > 3.5 ? keep * uKeepA + leave * uLeaveA + lift * uLiftA + drop * uDropA : uMode > 1.5 ? uGhost : 1.0;
  gl_FragColor = vec4(c, a);
}`;

// The pick pass: `who` already carries entry + 1, so zero stays no owner.
export const PICK_FRAG = `
varying vec2 vWho;
void main() {
  vec2 v = floor(vWho + 0.5);
  float e = v.x;
  gl_FragColor = vec4(mod(e, 256.0), floor(e / 256.0), mod(v.y, 256.0), floor(v.y / 256.0)) / 255.0;
}`;

// The outline's mask: the selection alone, drawn with no depth test, so
// the outline follows its whole shape, through whatever stands in front.
export const MASK_FRAG = `
uniform vec2 uSel;
varying vec2 vWho;
void main() {
  vec2 v = floor(vWho + 0.5);
  if (uSel.x < 0.0 || v.x != uSel.x + 1.0 || (uSel.y >= 0.0 && v.y != uSel.y)) discard;
  gl_FragColor = vec4(1.0);
}`;

// The outline: a ring of samples round each pixel outside the mask; the
// nearer the mask, the stronger. uPx is one device pixel in texture units,
// uWidth the line's width in device pixels.
export const OUTLINE_VERT = `
varying vec2 vUv;
void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }`;
export const OUTLINE_FRAG = `
uniform sampler2D uMask; uniform vec2 uPx; uniform float uWidth; uniform vec3 uAccent;
varying vec2 vUv;
void main() {
  float inside = texture2D(uMask, vUv).r;
  float near = 0.0;
  for (int r = 1; r <= 3; r++) {
    float d = uWidth * float(r) / 3.0;
    float w = 1.0 - smoothstep(uWidth - 0.75, uWidth + 0.75, d);
    for (int k = 0; k < 16; k++) {
      float a = 6.2831853 * (float(k) + 0.5 * float(r)) / 16.0;
      near = max(near, texture2D(uMask, vUv + vec2(cos(a), sin(a)) * d * uPx).r * w);
    }
  }
  float alpha = near * (1.0 - inside);
  if (alpha < 0.01) discard;
  gl_FragColor = vec4(uAccent, alpha);
}`;
// The outline's width, CSS pixels.
export const OUTLINE_PX = 2.5;

// The point pass: each fragment writes where it is, x and y in 12 bits
// each and z in 8, across the mesh's bounds.
export const POINT_FRAG = `
uniform vec3 uLo; uniform vec3 uSpan;
varying vec3 vP;
void main() {
  vec3 q = clamp((vP - uLo) / uSpan, 0.0, 1.0);
  float x = floor(q.x * 4095.0 + 0.5), y = floor(q.y * 4095.0 + 0.5), z = floor(q.z * 255.0 + 0.5);
  gl_FragColor = vec4(mod(x, 256.0), floor(x / 256.0) + 16.0 * mod(y, 16.0), floor(y / 16.0), z) / 255.0;
}`;

// How much of a faded thing shows: the rest beside a selection; an older
// build's mesh while the new one builds; a stage's finished parts and a
// rough stand-in in a build's sketch.
export const FADE = 0.45, REGION_FADE = 0.16, STALE = 0.3, SKETCH_PARTS = 0.7, SKETCH_ROUGH = 0.35;
// What stands for work still going (a rough stand-in, or the old build
// before anything new has come) breathes: its alpha swings this share
// either way, once every PULSE_S seconds.
export const PULSE = 0.35, PULSE_S = 1.4;
// Seconds the rest takes to fade when something is selected, and to come
// back when nothing is; and a hover's, quicker.
export const FADE_IN = 0.12, FADE_OUT = 0.1, HOVER_S = 0.14;
// The debug layer's dots: a disc with a dark rim, a fixed size on screen.
export const DOT_VERT = `attribute vec3 color; varying vec3 vC; uniform float uSize;
void main() { vC = color; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); gl_PointSize = uSize; }`;
export const DOT_FRAG = `varying vec3 vC;
void main() { float r = length(gl_PointCoord - 0.5); if (r > 0.5) discard; gl_FragColor = vec4(r > 0.36 ? vC * 0.35 : vC, 1.0); }`;
// cloud sheets: a fill whose opacity is each node's shade (how often that
// height comes up), a brighter contour
export const CONTOUR_ALPHA = 0.95;
// a hovered cloud is drawn at this share of a selected one's strength
export const CLOUD_HOVER = 0.4;
// how wide a cloud's span (low to high at a point) and a sheet's contour are
// drawn, CSS pixels: thick enough to read beside the selection's outline
export const CLOUD_SPAN_PX = 3.5, CLOUD_CONTOUR_PX = 2;
// a cloud at a point: a marker at the height the roll gave, a fixed size on screen

// The hover: with nothing selected the rest dims a little round what is
// under the pointer; with a selection, what is under the pointer lifts
// part way out of the fade.
export const HOVER_REST = 0.7, HOVER_LIFT = 0.75;
