// Types for the vendored Preact (preact.mjs): the exports the page uses.  A vnode is what `h` returns.
export interface VNode<P = any> { type: any; props: P; key: any }
export type ComponentChild = VNode | string | number | boolean | null | undefined | ComponentChild[];
export declare function h(type: any, props?: any, ...children: any[]): VNode;
export declare function render(vnode: ComponentChild, parent: Element | DocumentFragment | null): void;
export declare const Fragment: (props: { children?: ComponentChild }) => ComponentChild;
