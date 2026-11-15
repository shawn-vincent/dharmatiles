// Types for the vendored @preact/signals (preact-signals.mjs): the core's exports again, and the hooks.
export { Signal, action, batch, computed, createModel, effect, signal, untracked } from './preact-signals-core.mjs';
import type { Signal, ReadonlySignal } from './preact-signals-core.mjs';
export declare function useSignal<T>(value: T): Signal<T>;
export declare function useComputed<T>(fn: () => T): ReadonlySignal<T>;
export declare function useSignalEffect(fn: () => void | (() => void)): void;
export declare function useModel<T>(model: () => T): T;
