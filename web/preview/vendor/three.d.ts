// Types for the vendored three.js (three.module.min.js): only the classes and members the viewer uses (js/viewer.js and
// js/viewer/*, js/editor/scene*.js).  A hand-written declaration of a library we do not own, kept to what the page calls:
// a member the page starts to use is added here, and tsc names it until it is.  Not a copy of the page's own data.

export declare const DoubleSide: 2;
export declare const BackSide: 1;
export declare const ColorManagement: { enabled: boolean };

type NumberArray = ArrayLike<number> & { length: number };

export declare class Vector2 {
  constructor(x?: number, y?: number);
  x: number; y: number;
  set(x: number, y: number): this;
  copy(v: Vector2): this;
  clone(): Vector2;
}
export declare class Vector3 {
  constructor(x?: number, y?: number, z?: number);
  x: number; y: number; z: number;
  set(x: number, y: number, z: number): this;
  copy(v: Vector3): this;
  clone(): Vector3;
  add(v: Vector3): this;
  sub(v: Vector3): this;
  addScaledVector(v: Vector3, s: number): this;
  multiplyScalar(s: number): this;
  setScalar(s: number): this;
  normalize(): this;
  length(): number;
  distanceTo(v: Vector3): number;
  dot(v: Vector3): number;
  cross(v: Vector3): this;
  lerp(v: Vector3, t: number): this;
  applyQuaternion(q: Quaternion): this;
  project(camera: Camera): this;
  unproject(camera: Camera): this;
  fromArray(a: ArrayLike<number>, offset?: number): this;
  toArray(): [number, number, number];
  setFromMatrixPosition(m: any): this;
}
export declare class Vector4 {
  constructor(x?: number, y?: number, z?: number, w?: number);
  x: number; y: number; z: number; w: number;
  set(x: number, y: number, z: number, w: number): this;
}
export declare class Quaternion {
  constructor(x?: number, y?: number, z?: number, w?: number);
  x: number; y: number; z: number; w: number;
  copy(q: Quaternion): this;
  clone(): Quaternion;
  set(x: number, y: number, z: number, w: number): this;
  setFromUnitVectors(from: Vector3, to: Vector3): this;
  setFromAxisAngle(axis: Vector3, angle: number): this;
  multiply(q: Quaternion): this;
  premultiply(q: Quaternion): this;
  slerp(q: Quaternion, t: number): this;
  invert(): this;
  normalize(): this;
  angleTo(q: Quaternion): number;
  equals(q: Quaternion): boolean;
}
export declare class Plane {
  constructor(normal?: Vector3, constant?: number);
}
export declare class Ray {
  origin: Vector3; direction: Vector3;
  intersectPlane(plane: Plane, target: Vector3): Vector3 | null;
}
export declare class Raycaster {
  ray: Ray;
  setFromCamera(coords: Vector2, camera: Camera): void;
}
export declare class Color {
  constructor(r?: number | string, g?: number, b?: number);
  r: number; g: number; b: number;
  set(c: number | string | Color): this;
  setRGB(r: number, g: number, b: number): this;
  setStyle(style: string): this;
  copy(c: Color): this;
  clone(): Color;
  lerp(c: Color, t: number): this;
  getHex(): number;
}

export declare class BufferAttribute {
  constructor(array: NumberArray, itemSize: number, normalized?: boolean);
  array: any; // the typed array the attribute was made over (a Float32Array or a Uint8Array here)
  itemSize: number;
  count: number;
  needsUpdate: boolean;
  setXYZ(index: number, x: number, y: number, z: number): this;
  setX(index: number, x: number): this;
}
export declare class BufferGeometry {
  attributes: Record<string, BufferAttribute>;
  index: BufferAttribute | null;
  boundingBox: any;
  setAttribute(name: string, attribute: BufferAttribute): this;
  getAttribute(name: string): BufferAttribute;
  deleteAttribute(name: string): this;
  setIndex(index: BufferAttribute | number[] | null): this;
  setDrawRange(start: number, count: number): void;
  computeVertexNormals(): void;
  computeBoundingBox(): void;
  computeBoundingSphere(): void;
  dispose(): void;
}
export declare class PlaneGeometry extends BufferGeometry { constructor(width?: number, height?: number); }
export declare class BoxGeometry extends BufferGeometry { constructor(width?: number, height?: number, depth?: number); }
export declare class SphereGeometry extends BufferGeometry { constructor(radius?: number, widthSegments?: number, heightSegments?: number); }

export interface Uniform { value: any }
export interface MaterialParameters {
  side?: number; transparent?: boolean; opacity?: number; depthWrite?: boolean; depthTest?: boolean; colorWrite?: boolean;
  vertexColors?: boolean; color?: number | string | Color; extensions?: { derivatives?: boolean }; polygonOffset?: boolean;
  polygonOffsetFactor?: number; polygonOffsetUnits?: number; [key: string]: unknown;
}
export declare class Material {
  side: number; transparent: boolean; opacity: number; depthWrite: boolean; depthTest: boolean; colorWrite: boolean;
  visible: boolean;
  wireframe: boolean;
  userData: Record<string, any>;
  needsUpdate: boolean;
  dispose(): void;
}
export declare class ShaderMaterial extends Material {
  constructor(parameters?: MaterialParameters & { uniforms?: Record<string, Uniform>; vertexShader?: string; fragmentShader?: string });
  uniforms: Record<string, Uniform>;
  clone(): ShaderMaterial;
}
export declare class MeshBasicMaterial extends Material {
  constructor(parameters?: MaterialParameters);
  color: Color;
}
export declare class MeshLambertMaterial extends Material {
  constructor(parameters?: MaterialParameters);
  color: Color;
}
export declare class LineBasicMaterial extends Material {
  constructor(parameters?: MaterialParameters);
  color: Color;
}

export declare class Object3D {
  position: Vector3;
  quaternion: Quaternion;
  scale: Vector3;
  up: Vector3;
  visible: boolean;
  renderOrder: number;
  frustumCulled: boolean;
  matrixAutoUpdate: boolean;
  matrix: any; // a Matrix4 (elements, copy, fromArray): the page sets a piece's from an array and reads its elements
  userData: Record<string, any>;
  children: Object3D[];
  parent: Object3D | null;
  add(...objects: Object3D[]): this;
  remove(...objects: (Object3D | null | undefined)[]): this; // three ignores an object that is not a child, null included
  clear(): this;
  lookAt(x: number | Vector3, y?: number, z?: number): void;
  updateMatrixWorld(force?: boolean): void;
  updateMatrix(): void;
  traverse(callback: (object: any) => void): void; // any: the page reads `geometry` and `material` of the Mesh, Points and LineSegments it visits
}
export declare class Group extends Object3D {}
export declare class HemisphereLight extends Object3D {
  constructor(skyColor?: number | string | Color, groundColor?: number | string | Color, intensity?: number);
}
export declare class DirectionalLight extends Object3D {
  constructor(color?: number | string | Color, intensity?: number);
}
export declare class Scene extends Object3D {
  background: Color | null;
}
export declare class Mesh extends Object3D {
  constructor(geometry?: BufferGeometry, material?: Material);
  geometry: BufferGeometry;
  material: any; // a Material, or an array of them: the page sets and reads ShaderMaterial and MeshBasicMaterial only
}
export declare class Points extends Object3D {
  constructor(geometry?: BufferGeometry, material?: Material);
  geometry: BufferGeometry;
  material: any;
}
export declare class LineSegments extends Object3D {
  constructor(geometry?: BufferGeometry, material?: Material);
  geometry: BufferGeometry;
  material: any;
}

export declare class Camera extends Object3D {
  matrixWorldInverse: any;
  projectionMatrix: any;
  updateProjectionMatrix(): void;
}
export declare class PerspectiveCamera extends Camera {
  constructor(fov?: number, aspect?: number, near?: number, far?: number);
  fov: number; aspect: number; near: number; far: number; zoom: number;
  setViewOffset(fullWidth: number, fullHeight: number, x: number, y: number, width: number, height: number): void;
  clearViewOffset(): void;
}
export declare class OrthographicCamera extends Camera {
  constructor(left?: number, right?: number, top?: number, bottom?: number, near?: number, far?: number);
  left: number; right: number; top: number; bottom: number; near: number; far: number; zoom: number;
  setViewOffset(fullWidth: number, fullHeight: number, x: number, y: number, width: number, height: number): void;
  clearViewOffset(): void;
}

export declare class Texture {}
export declare class WebGLRenderTarget {
  constructor(width?: number, height?: number, options?: { samples?: number; depthBuffer?: boolean; [key: string]: unknown });
  texture: Texture;
  width: number; height: number;
  setSize(width: number, height: number): void;
  dispose(): void;
}
export declare class WebGLRenderer {
  constructor(parameters?: { canvas?: HTMLCanvasElement; antialias?: boolean; alpha?: boolean; preserveDrawingBuffer?: boolean });
  domElement: HTMLCanvasElement;
  autoClear: boolean;
  setPixelRatio(ratio: number): void;
  setSize(width: number, height: number, updateStyle?: boolean): void;
  setClearColor(color: Color | number | string, alpha?: number): void;
  getClearAlpha(): number;
  setRenderTarget(target: WebGLRenderTarget | null): void;
  render(scene: Object3D, camera: Camera): void;
  clear(color?: boolean, depth?: boolean, stencil?: boolean): void;
  clearDepth(): void;
  readRenderTargetPixels(target: WebGLRenderTarget, x: number, y: number, width: number, height: number, buffer: ArrayBufferView): void;
  getSize(target: Vector2): Vector2;
  getPixelRatio(): number;
  forceContextLoss(): void;
  dispose(): void;
}
