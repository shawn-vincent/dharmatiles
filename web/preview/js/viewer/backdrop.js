// The backdrop (docs/maps.html): the place the tiles stand in.  A per-user choice, never part of a document:
// whatever you chose last is used for the map and for the tile editor.  The tiles stand on a ground that runs
// out to a horizon and has a world round it, never in a sky.
//
//   wilderness  a mottled green ground with a faint grid, a horizon of distant blue-grey mountains, sky and cloud
//   cavern      dark rock floor and walls all round, a warm light over the tiles
//   table       a wooden tabletop under the tiles, a dim room beyond its edge
//   plain       nothing: the plate colour and the grid, as the tile view has always looked
//
// Drawn first, in its own pass with its own camera (the viewer's pose, a far clip that reaches the horizon), so
// the tile view's near and far planes are not stretched by a ground that has no end; then the depth is cleared
// and the pieces are drawn over it.  Everything is procedural (shaders): no pictures, nothing to load.  The sky
// is a dome that follows the camera, the ground a plane at the lowest underside with a haze that matches the
// dome's horizon, and each piece casts a soft contact shadow on it.

import * as THREE from 'three';

export const KINDS = [
  ['wilderness', 'Wilderness', 'Green ground and distant mountains'],
  ['cavern', 'Cavern', 'Dark rock all round, a warm light'],
  ['table', 'Table', 'A wooden tabletop'],
  ['plain', 'Plain', 'Only the plate and the grid'],
];
const KIND_INDEX = { wilderness: 0, cavern: 1, table: 2 };
const MAX_RECTS = 64;

const NOISE = `
float hash(vec2 p) { p = fract(p * vec2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
float vnoise(vec2 p) {
  vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
float fbm(vec2 p) { float v = 0.0, a = 0.5; mat2 r = mat2(0.8, -0.6, 0.6, 0.8); for (int i = 0; i < 5; i++) { v += a * vnoise(p); p = r * p * 2.03 + vec2(17.1, 9.7); a *= 0.5; } return v; }
`;

const DOME_VERT = `varying vec3 vDir; void main() { vDir = normalize(position); gl_Position = projectionMatrix * viewMatrix * modelMatrix * vec4(position, 1.0); }`;
const DOME_FRAG = `
uniform float uKind; uniform float uNight; uniform vec3 uHaze;
varying vec3 vDir;
${NOISE}
vec3 ridge(vec3 col, float th, float el, float base, float amp, float k, float off, vec3 c, float snow) {
  float h = base + amp * fbm(vec2(cos(th), sin(th)) * k + off);
  float inside = 1.0 - smoothstep(0.0, 0.0012, el - h);
  float up = clamp(el / max(h, 1e-4), 0.0, 1.0);
  float tex = 0.92 + 0.16 * fbm(vec2(th * 28.0, el * 90.0) + off);
  vec3 m = c * tex * mix(0.9, 1.07, up);
  m = mix(m, mix(m, vec3(0.94, 0.96, 1.0), 0.55), snow * smoothstep(0.78, 0.97, up));
  return mix(col, m, inside);
}
vec3 wild(vec3 d) {
  float el = asin(clamp(d.z, -1.0, 1.0)), th = atan(d.y, d.x);
  vec3 col = mix(uHaze, mix(vec3(0.30, 0.48, 0.74), vec3(0.03, 0.06, 0.16), uNight), pow(clamp(el * 2.2, 0.0, 1.0), 0.55));
  if (d.z > 0.0) {
    float c = fbm(d.xy / (d.z + 0.3) * 1.5 + vec2(3.0, 1.0));
    col = mix(col, mix(vec3(1.0), vec3(0.34, 0.4, 0.55), uNight), smoothstep(0.5, 0.78, c) * 0.5 * smoothstep(0.0, 0.3, el));
  }
  vec3 far = mix(vec3(0.64, 0.73, 0.83), vec3(0.2, 0.26, 0.4), uNight);
  vec3 mid = mix(vec3(0.52, 0.62, 0.73), vec3(0.15, 0.2, 0.32), uNight);
  vec3 near = mix(vec3(0.4, 0.5, 0.61), vec3(0.1, 0.14, 0.24), uNight);
  col = ridge(col, th, el, 0.06, 0.17, 2.1, 3.0, far, 1.0);
  col = ridge(col, th, el, 0.03, 0.13, 3.0, 11.0, mid, 0.5);
  col = ridge(col, th, el, 0.01, 0.09, 4.3, 23.0, near, 0.0);
  return d.z < 0.0 ? uHaze : col;
}
vec3 cave(vec3 d) {
  float el = asin(clamp(d.z, -1.0, 1.0)), th = atan(d.y, d.x);
  vec2 p = vec2(cos(th), sin(th)) * 3.0;
  float n = fbm(p * 2.0 + vec2(el * 5.0)), band = vnoise(vec2(th * 3.0, el * 38.0));
  vec3 rock = mix(vec3(0.15, 0.125, 0.105), vec3(0.36, 0.3, 0.26), n) * (0.75 + 0.5 * band);
  float up = smoothstep(0.05, 1.0, d.z);
  vec3 col = rock * mix(1.0, 0.3, up);
  col += vec3(0.2, 0.11, 0.05) * (1.0 - smoothstep(0.0, 0.3, abs(el - 0.05))) * (0.5 + n);
  return mix(uHaze, col, smoothstep(0.0, 0.14, el));
}
vec3 room(vec3 d) {
  vec3 col = mix(uHaze, vec3(0.2, 0.17, 0.15), smoothstep(-0.05, 0.8, d.z));
  col += vec3(0.1, 0.085, 0.06) * smoothstep(0.93, 1.0, dot(d, normalize(vec3(0.4, 0.5, 0.6))));
  return col;
}
void main() {
  vec3 d = normalize(vDir);
  vec3 c = uKind < 0.5 ? wild(d) : uKind < 1.5 ? cave(d) : room(d);
  gl_FragColor = vec4(c, 1.0);
}`;

const GROUND_VERT = `varying vec3 vW; void main() { vec4 w = modelMatrix * vec4(position, 1.0); vW = w.xyz; gl_Position = projectionMatrix * viewMatrix * w; }`;
const GROUND_FRAG = `
uniform float uKind; uniform float uNight; uniform vec3 uHaze; uniform vec3 uEye; uniform vec2 uCentre; uniform float uExtent;
uniform float uSq; uniform float uGrid; uniform vec3 uInk; uniform float uFog; uniform vec4 uRects[${MAX_RECTS}]; uniform float uN;
varying vec3 vW;
${NOISE}
float line(float v, float px, float w) { float d = abs(fract(v - 0.5) - 0.5) / px; return 1.0 - smoothstep(w - 0.5, w + 0.5, d); }
vec3 meadow(vec2 p) {
  mat2 r = mat2(0.8, -0.6, 0.6, 0.8);
  float a = fbm(p / 260.0), b = fbm(p / 62.0 + 7.0), c = fbm(r * p / 9.0 + 3.0), f = vnoise(r * p / 1.6);
  vec3 g1 = vec3(0.34, 0.50, 0.26), g2 = vec3(0.46, 0.58, 0.29), g3 = vec3(0.27, 0.41, 0.23), dry = vec3(0.55, 0.57, 0.31);
  vec3 col = mix(g1, g2, smoothstep(0.3, 0.7, a));
  col = mix(col, g3, smoothstep(0.45, 0.7, b) * 0.55);
  col = mix(col, dry, smoothstep(0.62, 0.8, fbm(p / 150.0 + 21.0)) * 0.4);
  col *= 0.93 + 0.14 * c;
  col *= 0.95 + 0.1 * f;
  col = mix(col, col * vec3(0.62, 0.7, 0.56), smoothstep(0.66, 0.76, vnoise(r * p / 1.15 + 40.0)) * 0.55);
  col = mix(col, col * vec3(1.18, 1.14, 0.92), smoothstep(0.7, 0.82, vnoise(r * r * p / 1.4 + 90.0)) * 0.35);
  return mix(col, col * vec3(0.16, 0.22, 0.4) * 1.6, uNight);
}
vec3 rockfloor(vec2 p) {
  float n = fbm(p / 90.0), m = fbm(p / 17.0 + 5.0);
  vec3 col = mix(vec3(0.17, 0.145, 0.13), vec3(0.3, 0.26, 0.23), n) * (0.8 + 0.4 * m);
  float crack = 1.0 - smoothstep(0.0, 0.018, abs(fbm(p / 55.0 + 2.0) - 0.5));
  col *= 1.0 - 0.4 * crack;
  float pool = exp(-pow(length(p - uCentre) / (uExtent * 2.2 + 120.0), 2.0));
  vec3 lit = col * (0.3 + 1.9 * pool) * mix(vec3(1.0), vec3(1.0, 0.8, 0.55), pool);
  return lit;
}
vec3 wood(vec2 p, float outside) {
  float board = floor(p.y / 92.0), off = hash(vec2(board, 3.0)) * 600.0;
  vec2 q = vec2((p.x + off) / 240.0, p.y / 5.0);
  float g = fbm(q + board * 1.7), ring = sin(g * 38.0 + fbm(q * 0.4) * 9.0);
  vec3 base = mix(vec3(0.44, 0.27, 0.14), vec3(0.6, 0.4, 0.22), hash(vec2(board, 9.0)));
  vec3 col = base * (0.78 + 0.34 * g) * (0.95 + 0.05 * ring);
  float yy = fract(p.y / 92.0) * 92.0;
  col *= 0.5 + 0.5 * smoothstep(0.0, 1.4, min(yy, 92.0 - yy));
  float xx = mod(p.x + off, 980.0);
  col *= 0.6 + 0.4 * smoothstep(0.0, 1.2, min(xx, 980.0 - xx));
  return col;
}
void main() {
  vec2 p = vW.xy;
  float dist = length(p - uEye.xy);
  vec3 col;
  float gridA = 0.16, gridOn = 1.0, haze = 1.0 - exp(-dist / uFog);
  if (uKind < 0.5) col = meadow(p);
  else if (uKind < 1.5) { col = rockfloor(p); gridA = 0.1; }
  else {
    float hw = uExtent * 1.7 + 330.0;
    vec2 e = abs(p - uCentre) - vec2(hw * 1.5, hw);
    float d = max(e.x, e.y);
    vec3 top = wood(p, 0.0);
    top *= 1.0 - 0.5 * smoothstep(0.35, 1.45, length((p - uCentre) / vec2(hw * 1.5, hw)));
    top = mix(top, top * 1.35 + 0.05, (1.0 - smoothstep(0.0, 9.0, -d)) * step(d, 0.0));
    vec3 floorc = vec3(0.075, 0.066, 0.06) * (0.9 + 0.2 * fbm(p / 14.0));
    col = d < 0.0 ? top : mix(floorc, floorc * 0.3, smoothstep(0.0, 40.0, d));
    gridA = 0.07;
    gridOn = d < 0.0 ? 1.0 : 0.0;
    haze *= 0.5;
  }
  // the grid: ink at each square, fading out from the map
  vec2 g = p / uSq, px = max(fwidth(g), vec2(1e-5));
  float major = max(line(g.x, px.x, 1.1), line(g.y, px.y, 1.1));
  float fade = 1.0 - smoothstep(uExtent * 1.3 + uSq, uExtent * 3.0 + uSq * 3.0, length(p - uCentre));
  col = mix(col, uInk, major * gridA * fade * uGrid * gridOn);
  // contact shadows: a soft darkening round each piece's footprint
  float s = 0.0;
  for (int i = 0; i < ${MAX_RECTS}; i++) {
    if (float(i) >= uN) break;
    vec4 r = uRects[i];
    vec2 c = (r.xy + r.zw) * 0.5, h = (r.zw - r.xy) * 0.5;
    vec2 q = abs(p - c) - h;
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
    s = max(s, 1.0 - smoothstep(0.0, 11.0, d));
  }
  col *= 1.0 - (uKind > 0.5 && uKind < 1.5 ? 0.6 : 0.34) * s;
  col = mix(col, uHaze, haze);
  gl_FragColor = vec4(col, 1.0);
}`;

export class Backdrop {
  constructor() {
    this.kind = 'plain';
    this.night = 0;
    this.scene = new THREE.Scene();
    this.cam = new THREE.PerspectiveCamera(40, 1, 2, 4e6);
    this.cam.up.set(0, 0, 1);
    this.ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, 1, 4e6);
    this.ortho.up.set(0, 0, 1);
    this.haze = new THREE.Color(0.72, 0.79, 0.87);
    this.uniforms = {
      uKind: { value: 0 }, uNight: { value: 0 }, uHaze: { value: this.haze }, uEye: { value: new THREE.Vector3() }, uCentre: { value: new THREE.Vector2() }, uExtent: { value: 200 },
      uSq: { value: 35 }, uGrid: { value: 1 }, uInk: { value: new THREE.Color(0.1, 0.16, 0.08) }, uFog: { value: 26000 },
      uRects: { value: Array.from({ length: MAX_RECTS }, () => new THREE.Vector4()) }, uN: { value: 0 },
    };
    const dome = new THREE.Mesh(new THREE.SphereGeometry(1, 48, 28), new THREE.ShaderMaterial({
      uniforms: this.uniforms, vertexShader: DOME_VERT, fragmentShader: DOME_FRAG, side: THREE.BackSide, depthWrite: false, depthTest: false,
    }));
    dome.scale.setScalar(2e6);
    dome.renderOrder = -2;
    dome.frustumCulled = false;
    const ground = new THREE.Mesh(new THREE.PlaneGeometry(1.6e6, 1.6e6), new THREE.ShaderMaterial({
      uniforms: this.uniforms, vertexShader: GROUND_VERT, fragmentShader: GROUND_FRAG, side: THREE.DoubleSide, depthWrite: false, depthTest: false, extensions: { derivatives: true },
    }));
    ground.renderOrder = -1;
    ground.frustumCulled = false;
    this.dome = dome;
    this.ground = ground;
    this.scene.add(dome, ground);
  }

  /** Is there a backdrop to draw (not `plain`)? */
  on() { return this.kind !== 'plain'; }

  /** Choose the kind (`wilderness`, `cavern`, `table` or `plain`). */
  setKind(kind) {
    this.kind = KIND_INDEX[kind] === undefined ? 'plain' : kind;
    this.uniforms.uKind.value = KIND_INDEX[this.kind] ?? 0;
    this.palette();
  }

  /** Daylight (0) or dusk (1): the dark theme's backdrop is evening. */
  setNight(n) { this.night = n; this.uniforms.uNight.value = n; this.palette(); }

  /** The colours that belong to the kind and the hour. */
  palette() {
    const u = this.uniforms;
    const k = this.kind;
    if (k === 'cavern') { this.haze.setRGB(0.026, 0.021, 0.018); u.uFog.value = 2200; u.uInk.value.setRGB(0.9, 0.78, 0.6); }
    else if (k === 'table') { this.haze.setRGB(0.09, 0.075, 0.065); u.uFog.value = 9000; u.uInk.value.setRGB(0.05, 0.03, 0.02); }
    else {
      this.haze.setRGB(0.72, 0.79, 0.87).lerp(new THREE.Color(0.2, 0.25, 0.37), this.night);
      u.uFog.value = 24000;
      u.uInk.value.setRGB(0.1, 0.16, 0.08).lerp(new THREE.Color(0.7, 0.8, 0.95), this.night);
    }
  }

  /** The ground: the world box of what stands on it (lo and hi, mm), its square, and the plane's height. */
  setGround(box, sq, z) {
    const u = this.uniforms;
    u.uCentre.value.set((box.lo[0] + box.hi[0]) / 2, (box.lo[1] + box.hi[1]) / 2);
    u.uExtent.value = Math.max(box.hi[0] - box.lo[0], box.hi[1] - box.lo[1], sq) / 2;
    u.uSq.value = sq;
    this.ground.position.set(0, 0, z);
    if (this.kind === 'cavern') u.uFog.value = Math.max(2200, u.uExtent.value * 14);
    if (this.kind === 'table') u.uFog.value = Math.max(9000, u.uExtent.value * 40);
  }

  /** Show the grid on the ground (the view's Grid switch). */
  setGrid(on) { this.uniforms.uGrid.value = on ? 1 : 0; }

  /** The footprints standing on the ground, `[x0, y0, x1, y1]` each (mm): they cast a soft shadow. */
  setRects(rects) {
    const n = Math.min(rects.length, MAX_RECTS);
    this.uniforms.uN.value = n;
    for (let i = 0; i < n; i++) this.uniforms.uRects.value[i].set(...rects[i]);
  }

  /** Draw the backdrop for the viewer's camera: its pose, a far clip that reaches the horizon. */
  render(renderer, camera) {
    const u = this.uniforms;
    u.uEye.value.copy(camera.position);
    let c;
    if (camera.isOrthographicCamera) {
      c = this.ortho;
      for (const k of ['left', 'right', 'top', 'bottom', 'zoom']) c[k] = camera[k];
      c.near = 1; c.far = 4e6;
    } else {
      c = this.cam;
      c.fov = camera.fov; c.aspect = camera.aspect;
    }
    c.position.copy(camera.position);
    c.quaternion.copy(camera.quaternion);
    c.updateProjectionMatrix();
    c.updateMatrixWorld(true);
    this.dome.position.copy(camera.position);
    renderer.render(this.scene, c);
  }
}
