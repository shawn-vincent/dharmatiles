// The source pane: the document as written, highlighted, and editable.
//
// CodeMirror 6 (vendor/codemirror.js, bundled once from the entry beside
// it): soft wrap with continuation lines indented under their parent,
// find and replace (⌘F), bracket matching, folding, a lint gutter fed by
// the check's mistakes, outdent, and several cursors.  The page owns the
// history (one undo for typing and for changes made in the view), so
// CodeMirror's own history is not loaded.

import {
  Prec, EditorState, Compartment, StateEffect, StateField, RangeSetBuilder,
  EditorView, keymap, lineNumbers, drawSelection, highlightActiveLineGutter, highlightSpecialChars,
  rectangularSelection, crosshairCursor, dropCursor, Decoration, ViewPlugin,
  defaultKeymap, indentLess, indentMore, insertNewlineAndIndent,
  searchKeymap, highlightSelectionMatches, search,
  StreamLanguage, HighlightStyle, syntaxHighlighting, bracketMatching, foldGutter, foldKeymap, foldService,
  indentUnit, indentService, setDiagnostics, lintGutter, closeBrackets, closeBracketsKeymap, tags,
} from '../vendor/codemirror.js';

/** UTF-8 byte offset `b` in `text` as a string index (the server counts bytes). */
export function byteToIndex(text, b) {
  if (!/[^\x00-\x7f]/.test(text)) return b;
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    if (bytes >= b) return i;
    const c = text.codePointAt(i);
    bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    if (c >= 0x10000) i++;
  }
  return text.length;
}

/** The indentation for a new line at `pos`: the line above's, one level more after `{`, one less before `}`. */
export function indentFor(text, pos) {
  const start = text.lastIndexOf('\n', pos - 1) + 1;
  const rest = text.slice(pos, (text.indexOf('\n', pos) + 1 || text.length + 1) - 1);
  let i = start - 1, prev = '';
  while (i > 0) { // the nearest line above with something on it
    const s = text.lastIndexOf('\n', i - 1) + 1;
    prev = text.slice(s, i);
    if (prev.trim()) break;
    i = s - 1;
  }
  const head = start === pos ? '' : text.slice(start, pos);
  const line = head.trim() ? head : prev;
  let n = (line.match(/^ */) || [''])[0].length;
  if (line.replace(/\/\/.*$/, '').trimEnd().endsWith('{')) n += 4;
  if (rest.trimStart().startsWith('}')) n = Math.max(0, n - 4);
  return n;
}

/** For a line ending in `{`, the offset of its matching `}` (strings and comments skipped), else null. */
export function braceEnd(text, from) {
  let depth = 0;
  for (let i = from; i < text.length; i++) {
    const c = text[i];
    if (c === '"') { i++; while (i < text.length && text[i] !== '"') { if (text[i] === '\\') i++; i++; } continue; }
    if (c === '/' && text[i + 1] === '/') { const nl = text.indexOf('\n', i); if (nl < 0) return null; i = nl; continue; }
    if (c === '{') depth++;
    if (c === '}' && --depth === 0) return i;
  }
  return null;
}

// KDL v2, enough to colour it: comments, strings, properties (`name=`),
// keywords, numbers, node names (the first word of a node) and other words.
const kdl = StreamLanguage.define({
  name: 'kdl',
  startState: () => ({ atNode: true }),
  token(stream, st) {
    if (stream.sol()) st.atNode = true;
    if (stream.eatSpace()) return null;
    if (stream.match(/^\/\/.*/)) return 'comment';
    if (stream.match('/-')) { st.atNode = true; return 'comment'; }
    if (stream.match(/^#?"(?:[^"\\]|\\.)*"?#?/)) { st.atNode = false; return 'string'; }
    if (stream.match(/^[A-Za-z_][\w+\-.]*(?==)/)) { st.atNode = false; return 'propertyName'; }
    if (stream.match(/^(#true|#false|#null|#inf|#-inf|#nan)/)) { st.atNode = false; return 'atom'; }
    if (stream.match(/^-?\d+(?:\.\d+)?(?![\w.])/)) { st.atNode = false; return 'number'; }
    if (stream.match(/^[A-Za-z_][\w+\-.]*/)) { const n = st.atNode; st.atNode = false; return n ? 'tagName' : null; }
    if (stream.match(/^[{};]/)) { st.atNode = true; return 'brace'; }
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: '//' } },
});
const colours = HighlightStyle.define([
  { tag: tags.comment, class: 'tk-c' },
  { tag: tags.string, class: 'tk-s' },
  { tag: tags.propertyName, class: 'tk-k' },
  { tag: [tags.atom, tags.number], class: 'tk-v' },
  { tag: tags.tagName, class: 'tk-n' },
]);

// soft wrap: a wrapped line's continuation sits one level under its own indentation
const hanging = ViewPlugin.fromClass(class {
  constructor(view) { this.decorations = this.build(view); }
  update(u) { if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view); }
  build(view) {
    const b = new RangeSetBuilder();
    for (const { from, to } of view.visibleRanges) {
      for (let pos = from; pos <= to;) {
        const line = view.state.doc.lineAt(pos);
        const n = (line.text.match(/^ */) || [''])[0].length + 4;
        b.add(line.from, line.from, Decoration.line({ attributes: { style: `padding-left: calc(${n}ch + 6px); text-indent: -${n}ch` } }));
        pos = line.to + 1;
      }
    }
    return b.finish();
  }
}, { decorations: (v) => v.decorations });

// the page's marks: the selected entry's lines, the hovered entry's, the lines with mistakes
const setMarks = StateEffect.define();
const marks = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setMarks)) continue;
      const { sel, hov, errs, notes = [] } = e.value;
      const doc = tr.state.doc, b = new RangeSetBuilder();
      const inside = (r, n) => r && n >= r[0] && n <= r[1];
      const err = new Set(errs), note = new Set(notes);
      for (let n = 1; n <= doc.lines; n++) {
        const cls = [inside(sel, n) && 'ln-sel', !inside(sel, n) && inside(hov, n) && 'ln-hov', err.has(n) && 'ln-err', !err.has(n) && note.has(n) && 'ln-note'].filter(Boolean);
        if (cls.length) { const l = doc.line(n); b.add(l.from, l.from, Decoration.line({ class: cls.join(' ') })); }
      }
      deco = b.finish();
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const folds = foldService.of((state, from) => {
  const line = state.doc.lineAt(from);
  if (!line.text.replace(/\/\/.*$/, '').trimEnd().endsWith('{')) return null;
  const text = state.doc.toString();
  const open = line.from + line.text.replace(/\/\/.*$/, '').lastIndexOf('{');
  const end = braceEnd(text, open);
  if (end == null || state.doc.lineAt(end).number === line.number) return null;
  return { from: line.to, to: end };
});

const QUIET = 'dt-quiet'; // a change made by the page, not by typing

export class TextPane {
  /**
   * `code` is the element the editor fills.  Callbacks: onCaretLine(n) when
   * the caret moves to a line by a click or a key, onHoverLine(n|null),
   * onCaret() after any move of the caret or edit of the text, onEdit(text, kind) after the user changes the text, onReadOnly({from, to, insert}) when
   * they try to type or paste into a file they cannot write (the change they meant: `insert` over the selection), onKey(e) first, for
   * completion (return true when it took the key), and onHistory(step).
   */
  constructor(code, cb) {
    this.code = code;
    this.cb = cb;
    this.file = null;
    this.text = null;
    this.writable = false;
    this.marks = {};
    this.hovered = null;
    this.ro = new Compartment();
    const pane = this;
    const keyFirst = Prec.highest(EditorView.domEventHandlers({
      keydown(e) {
        if (cb.onKey?.(e)) { e.preventDefault(); e.stopPropagation(); return true; }
        if (!pane.writable && e.key.length === 1 && !e.metaKey && !e.ctrlKey) { cb.onReadOnly?.(pane.typed(e.key)); return true; }
        return false;
      },
      paste(e) { if (!pane.writable) { cb.onReadOnly?.(pane.typed(e.clipboardData?.getData('text/plain') ?? '')); return true; } return false; },
      mousemove(e, view) {
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
        const n = pos == null ? null : view.state.doc.lineAt(pos).number;
        if (n !== pane.hovered) { pane.hovered = n; cb.onHoverLine?.(n); }
      },
      mouseleave() { pane.hovered = null; cb.onHoverLine?.(null); },
    }));
    const keys = keymap.of([
      { key: 'Tab', run: indentMore, shift: indentLess },
      { key: 'Enter', run: insertNewlineAndIndent },
      ...closeBracketsKeymap, ...searchKeymap, ...foldKeymap, ...defaultKeymap,
    ]);
    this.view = new EditorView({
      parent: code,
      state: EditorState.create({
        doc: '',
        extensions: [
          lineNumbers(), foldGutter({ openText: '▾', closedText: '▸' }), lintGutter(), highlightActiveLineGutter(),
          highlightSpecialChars(), drawSelection(), dropCursor(), EditorState.allowMultipleSelections.of(true),
          rectangularSelection(), crosshairCursor(), bracketMatching(), closeBrackets(), highlightSelectionMatches(),
          search({ top: true }), kdl, syntaxHighlighting(colours), folds,
          indentUnit.of('    '), EditorState.tabSize.of(4),
          indentService.of((cx, pos) => indentFor(cx.state.doc.toString(), pos)),
          EditorView.lineWrapping, hanging, marks, keyFirst, keys,
          this.ro.of(EditorState.readOnly.of(true)),
          EditorView.contentAttributes.of({ 'aria-label': 'The document', autocapitalize: 'off', spellcheck: 'false' }),
          EditorView.updateListener.of((u) => this.updated(u)),
        ],
      }),
    });
    this.watchMarks();
  }

  /** A press on a mark in the gutter: `onMarkClick(line)`, which opens the message it points to. */
  watchMarks() {
    this.code.addEventListener('click', (e) => {
      if (!e.target.closest?.('.cm-lint-marker')) return;
      const at = this.view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
      if (at != null) this.cb.onMarkClick?.(this.view.state.doc.lineAt(at).number);
    });
  }

  updated(u) {
    const quiet = u.transactions.some((tr) => tr.isUserEvent(QUIET));
    if (u.docChanged) {
      this.text = u.state.doc.toString();
      if (!quiet) this.cb.onEdit?.(this.text, 'type');
    }
    if (u.selectionSet && !quiet && u.transactions.some((tr) => tr.isUserEvent('select'))) {
      this.cb.onCaretLine?.(this.caretLine());
    }
    // the caret is somewhere else now, by a move or by typing: whatever follows the caret (a cloud) asks again
    if ((u.selectionSet || u.docChanged) && !quiet) this.cb.onCaret?.();
  }

  /** Change the document from the page (not typing): only the changed middle, so folds and scroll stay. */
  replace(text, selection) {
    const old = this.view.state.doc.toString();
    let a = 0;
    while (a < old.length && a < text.length && old[a] === text[a]) a++;
    let b = 0;
    while (b < old.length - a && b < text.length - a && old[old.length - 1 - b] === text[text.length - 1 - b]) b++;
    const spec = { userEvent: QUIET };
    if (a !== old.length || a !== text.length) spec.changes = { from: a, to: old.length - b, insert: text.slice(a, text.length - b) };
    if (selection) spec.selection = selection;
    this.view.dispatch(spec);
    this.text = text;
  }

  /** Show `text` from `file`; the caret and scroll stay when it is the same file. */
  show(file, text, writable) {
    const same = file === this.file;
    this.writable = !!writable;
    this.view.dispatch({ effects: this.ro.reconfigure(EditorState.readOnly.of(!writable)), userEvent: QUIET });
    this.code.classList.toggle('ro', !writable);
    if (same && text === this.text) return;
    this.file = file;
    if (same) { this.setText(text); return; }
    this.replace(text, { anchor: 0 });
    this.view.scrollDOM.scrollTop = 0;
    this.applyMarks();
  }

  /** Replace the text (a change made elsewhere: the view, undo); the caret stays near where it was. */
  setText(text, caret = null) {
    const s = this.view.state.selection.main;
    const clamp = (n) => Math.min(n, text.length);
    this.replace(text, caret != null ? { anchor: caret } : { anchor: clamp(s.anchor), head: clamp(s.head) });
  }

  /** Type `s` over [from, to] (the caret's selection by default), as if typed. */
  insert(s, from = this.view.state.selection.main.from, to = this.view.state.selection.main.to) {
    this.view.dispatch({ changes: { from, to, insert: s }, selection: { anchor: from + s.length }, userEvent: 'input.complete' });
  }

  clear() { this.file = null; this.replace(''); }

  /** The change typing (or pasting) `insert` would make over the selection: {from, to, insert}. */
  typed(insert) {
    const { from, to } = this.view.state.selection.main;
    return { from, to, insert };
  }

  /** Where the caret is, as an index into the text. */
  get selectionStart() { return this.view.state.selection.main.from; }
  caretLine() { return this.view.state.doc.lineAt(this.view.state.selection.main.head).number; }
  /** The caret's line and column (both from 1) and the line's text either side of it. */
  caret() {
    const i = this.view.state.selection.main.head;
    const l = this.view.state.doc.lineAt(i);
    return { index: i, line: l.number, col: i - l.from + 1, start: l.from, before: l.text.slice(0, i - l.from), after: l.text.slice(i - l.from) };
  }

  /** Where the caret is on screen (its bottom left), for the completion list. */
  caretXY() {
    const c = this.view.coordsAtPos(this.view.state.selection.main.head);
    return c ? { x: c.left, y: c.bottom } : null;
  }

  focus() { this.view.focus(); }

  /** Mark lines: `sel` and `hov` are [first, last] (1-based) or null; `errs` line numbers or {line, message, severity ('error', the default, or 'warning' for a note), id (the message it points to)}. */
  mark(sel, hov, errs = []) {
    this.marks = { sel, hov, errs };
    this.applyMarks();
  }

  applyMarks() {
    const { sel = null, hov = null, errs = [] } = this.marks;
    const doc = this.view.state.doc;
    const list = (errs || []).map((e) => (typeof e === 'number' ? { line: e } : e)).filter((e) => e.line >= 1 && e.line <= doc.lines);
    const diags = list.map((e) => { const l = doc.line(e.line); return { from: l.from, to: l.to, severity: e.severity === 'warning' ? 'warning' : 'error', message: e.message || 'A mistake on this line', id: e.id }; });
    const lines = (sev) => list.filter((e) => (e.severity === 'warning') === (sev === 'warning')).map((e) => e.line);
    this.view.dispatch(setDiagnostics(this.view.state, diags), { effects: setMarks.of({ sel, hov, errs: lines('error'), notes: lines('warning') }), userEvent: QUIET });
  }

  /** Scroll so line `n` is in view, unless it already is. */
  reveal(n) {
    const doc = this.view.state.doc;
    if (n < 1 || n > doc.lines) return;
    const pos = doc.line(n).from;
    const block = this.view.lineBlockAt(pos);
    const sc = this.view.scrollDOM;
    const top = block.top, bottom = block.bottom;
    if (top < sc.scrollTop + 8 || bottom > sc.scrollTop + sc.clientHeight - 24) {
      this.view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: 'start', yMargin: sc.clientHeight / 3 }), userEvent: QUIET });
    }
  }
}
