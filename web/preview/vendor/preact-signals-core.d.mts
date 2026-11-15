// Types for the vendored @preact/signals-core (preact-signals-core.mjs), which ships without any: what the
// vendored file really exports, as read from it.  `peek` is there on every signal (`Signal.prototype.peek`),
// computed ones included, which tsc cannot see through the minified prototype assignments.
export interface SignalOptions<T = unknown> { watched?: (this: Signal<T>) => void; unwatched?: (this: Signal<T>) => void; name?: string }
export declare class Signal<T = any> {
  constructor(value?: T, options?: SignalOptions<T>);
  value: T;
  /** The current value, read without subscribing the running effect or computed to it. */
  peek(): T;
  subscribe(fn: (value: T) => void): () => void;
  valueOf(): T;
  toString(): string;
  toJSON(): T;
  name?: string;
}
export interface ReadonlySignal<T = any> extends Signal<T> { readonly value: T }
export declare class Computed<T = any> extends Signal<T> {}
export declare class Effect {}
export declare function signal<T>(value: T, options?: SignalOptions<T>): Signal<T>;
export declare function signal<T = undefined>(): Signal<T | undefined>;
export declare function computed<T>(fn: () => T, options?: SignalOptions<T>): ReadonlySignal<T>;
export declare function effect(fn: (this: Effect) => void | (() => void), options?: { name?: string }): () => void;
export declare function batch<T>(fn: () => T): T;
export declare function untracked<T>(fn: () => T): T;
export declare function action<T extends (...args: any[]) => any>(fn: T): T;
export declare function createModel<T extends object, A extends any[]>(fn: (...args: A) => T): (...args: A) => T;
