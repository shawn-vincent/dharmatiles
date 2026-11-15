// What the vendored CodeMirror 6 bundle (codemirror.js, made from codemirror.entry.js beside it) exports.  The
// @codemirror packages' own types are not vendored (the bundle is one minified file), so this declares the part the page
// uses (js/text.js, which wraps it in the typed `TextPane`) and nothing more: a hand-written declaration of a library we
// do not own.  The exports the page does not call by a member, only passes along in a list of extensions, are `any`.

export interface Line { from: number; to: number; number: number; text: string }
export interface Text {
  lines: number;
  line(n: number): Line;
  lineAt(pos: number): Line;
  toString(): string;
}
export interface SelectionRange { anchor: number; head: number; from: number; to: number }
export interface ChangeSet { }
export interface ChangeSpec { from: number; to?: number; insert?: string }
export interface Transaction {
  changes: { }; // a ChangeSet: handed to `DecorationSet.map` and to nothing else
  effects: readonly StateEffect<any>[];
  state: EditorState;
  isUserEvent(event: string): boolean;
}
export interface TransactionSpec {
  changes?: ChangeSpec;
  selection?: { anchor: number; head?: number };
  effects?: StateEffect<any> | readonly StateEffect<any>[];
  userEvent?: string;
}
export interface Extension { }
export declare class EditorState {
  doc: Text;
  selection: { main: SelectionRange };
  static create(config: { doc?: string; extensions?: Extension[] }): EditorState;
  static languageData: { of(f: () => unknown[]): Extension };
  static allowMultipleSelections: { of(on: boolean): Extension };
  static tabSize: { of(n: number): Extension };
  static readOnly: { of(on: boolean): Extension };
}
export interface StateEffectType<V> { of(value: V): StateEffect<V> }
export interface StateEffect<V> { value: V; is<W>(type: StateEffectType<W>): this is StateEffect<W> }
export declare const StateEffect: { define<V = void>(): StateEffectType<V> };
export interface StateField<V> extends Extension { }
export declare const StateField: {
  define<V>(spec: {
    create: (state: EditorState) => V;
    update: (value: V, tr: Transaction) => V;
    provide?: (field: StateField<V>) => Extension;
  }): StateField<V>;
};
export interface Range<V> { from: number; to: number; value: V }
export interface DecorationSet { map(changes: { }): DecorationSet }
export interface Decoration { range(from: number, to?: number): Range<Decoration> }
export declare const Decoration: {
  none: DecorationSet;
  line(spec: { class?: string; attributes?: Record<string, string> }): Decoration;
  mark(spec: { class?: string }): Decoration;
  set(ranges: Range<Decoration>[], sort?: boolean): DecorationSet;
};
export declare class RangeSetBuilder<V> {
  add(from: number, to: number, value: V): void;
  finish(): DecorationSet;
}
export declare class Compartment {
  of(ext: Extension): Extension;
  reconfigure(ext: Extension): StateEffect<unknown>;
}
export interface ViewUpdate {
  view: EditorView;
  state: EditorState;
  transactions: readonly Transaction[];
  docChanged: boolean;
  viewportChanged: boolean;
  selectionSet: boolean;
}
export interface PluginValue { decorations?: DecorationSet }
export declare const ViewPlugin: {
  fromClass<T extends PluginValue>(cls: new (view: EditorView) => T, spec?: { decorations?: (v: T) => DecorationSet }): Extension;
};
export declare class EditorView {
  constructor(config: { parent?: Element; state?: EditorState });
  state: EditorState;
  scrollDOM: HTMLElement;
  visibleRanges: readonly { from: number; to: number }[];
  dispatch(spec: TransactionSpec): void;
  dispatch(tr: unknown, spec?: TransactionSpec): void;
  posAtCoords(coords: { x: number; y: number }, precise: false): number;
  posAtCoords(coords: { x: number; y: number }, precise?: boolean): number | null;
  coordsAtPos(pos: number): { left: number; right: number; top: number; bottom: number } | null;
  lineBlockAt(pos: number): { top: number; bottom: number };
  focus(): void;
  static decorations: { from(field: StateField<DecorationSet>): Extension };
  static domEventHandlers(handlers: {
    keydown?: (e: KeyboardEvent, view: EditorView) => boolean | void;
    paste?: (e: ClipboardEvent, view: EditorView) => boolean | void;
    mousemove?: (e: MouseEvent, view: EditorView) => boolean | void;
    mouseleave?: (e: MouseEvent, view: EditorView) => boolean | void;
  }): Extension;
  static lineWrapping: Extension;
  static contentAttributes: { of(attrs: Record<string, string>): Extension };
  static updateListener: { of(f: (u: ViewUpdate) => void): Extension };
  static scrollIntoView(pos: number, options?: { y?: 'start' | 'center' | 'end' | 'nearest'; yMargin?: number }): StateEffect<unknown>;
}
export interface KeyBinding { key: string; run?: unknown; shift?: unknown }
export declare const keymap: { of(bindings: readonly KeyBinding[]): Extension };
export declare const Prec: { highest(ext: Extension): Extension };
export declare const foldService: { of(f: (state: EditorState, from: number) => { from: number; to: number } | null): Extension };
export declare const indentUnit: { of(unit: string): Extension };
export declare const indentService: { of(f: (cx: { state: EditorState }, pos: number) => number | null): Extension };
export interface Diagnostic {
  from: number; to: number; severity: 'error' | 'warning' | 'info'; message: string;
  [key: string]: unknown; // the page adds its own fields (id, actions)
}
export declare function setDiagnostics(state: EditorState, diagnostics: readonly Diagnostic[]): unknown;
export declare function foldGutter(config?: { markerDOM?: (open: boolean) => HTMLElement }): Extension;
export declare function search(config?: { top?: boolean }): Extension;
export declare function lineNumbers(): Extension;
export declare function lintGutter(): Extension;
export declare function highlightActiveLineGutter(): Extension;
export declare function highlightSpecialChars(): Extension;
export declare function drawSelection(): Extension;
export declare function dropCursor(): Extension;
export declare function rectangularSelection(): Extension;
export declare function crosshairCursor(): Extension;
export declare function bracketMatching(): Extension;
export declare function closeBrackets(): Extension;
export declare function highlightSelectionMatches(): Extension;
export declare const indentMore: unknown, indentLess: unknown, insertNewlineAndIndent: unknown;
export declare const defaultKeymap: readonly KeyBinding[], searchKeymap: readonly KeyBinding[], foldKeymap: readonly KeyBinding[], closeBracketsKeymap: readonly KeyBinding[];

// exported by the bundle (codemirror.entry.js) and not used by the page's own code
export declare const EditorSelection: any, highlightActiveLine: any;
export declare const StreamLanguage: any, HighlightStyle: any, syntaxHighlighting: any, codeFolding: any;
export declare const linter: any, openSearchPanel: any, tags: any;
