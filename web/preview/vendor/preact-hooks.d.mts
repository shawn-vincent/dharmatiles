// Types for the vendored preact/hooks (preact-hooks.mjs): the hooks the page uses.
export declare function useState<S>(initial: S | (() => S)): [S, (next: S | ((prev: S) => S)) => void];
export declare function useEffect(effect: () => void | (() => void), deps?: readonly unknown[]): void;
export declare function useLayoutEffect(effect: () => void | (() => void), deps?: readonly unknown[]): void;
export declare function useRef<T>(initial: T): { current: T };
export declare function useRef<T = undefined>(): { current: T | undefined };
