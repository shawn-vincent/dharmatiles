// Types for the vendored three.js OrbitControls (OrbitControls.js): the members the viewer uses.  three itself
// is untyped here (no types are vendored), so the camera and target are `any`.
export declare class OrbitControls {
  constructor(camera: any, domElement: HTMLElement);
  enabled: boolean;
  enableDamping: boolean;
  enableRotate: boolean;
  enablePan: boolean;
  zoomToCursor: boolean;
  minDistance: number;
  maxDistance: number;
  minZoom: number;
  maxZoom: number;
  target: any;
  update(): boolean;
  dispose(): void;
  addEventListener(type: string, listener: (event?: any) => void): void;
  removeEventListener(type: string, listener: (event?: any) => void): void;
}
