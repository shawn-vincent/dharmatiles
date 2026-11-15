// Types for the vendored htm (htm.mjs), bound to Preact's `h` in js/view/html.js: a tagged template that makes vnodes.
import type { VNode } from './preact.mjs';
declare function htm(strings: TemplateStringsArray, ...values: any[]): VNode;
export default htm;
