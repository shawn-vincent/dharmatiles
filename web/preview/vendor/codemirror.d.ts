// What the vendored CodeMirror 6 bundle (codemirror.js, made from codemirror.entry.js beside it) exports.  The
// @codemirror packages' own types are not vendored (the bundle is one minified file), so each export is declared
// `any`: a genuine boundary to a library we do not own, kept in this one file.  The page's own use of it is
// js/text.js, which wraps it in the typed `Source` editor.
export declare const Prec: any, EditorState: any, Compartment: any, StateEffect: any, StateField: any, RangeSetBuilder: any, EditorSelection: any;
export declare const EditorView: any, keymap: any, lineNumbers: any, drawSelection: any, highlightActiveLine: any, highlightActiveLineGutter: any, highlightSpecialChars: any, rectangularSelection: any, crosshairCursor: any, dropCursor: any, Decoration: any, ViewPlugin: any;
export declare const defaultKeymap: any, indentLess: any, indentMore: any, insertNewlineAndIndent: any;
export declare const searchKeymap: any, highlightSelectionMatches: any, search: any, openSearchPanel: any;
export declare const StreamLanguage: any, HighlightStyle: any, syntaxHighlighting: any, bracketMatching: any, foldGutter: any, foldKeymap: any, foldService: any, indentUnit: any, indentService: any, codeFolding: any;
export declare const lintGutter: any, setDiagnostics: any, linter: any;
export declare const closeBrackets: any, closeBracketsKeymap: any;
export declare const tags: any;
