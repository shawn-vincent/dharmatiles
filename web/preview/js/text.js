// The text pane: the document as written, highlighted, and editable.
//
// A transparent textarea lies exactly over the highlighted lines (same
// font, line height and padding), so typing, selecting, copying and the
// system's text keys all work as they do anywhere, while the lines under
// it carry the colours and the marks: the selection, the hovered entry and
// the lines with mistakes.  The page owns the history (one undo for typing
// and for changes made in the view), so the textarea's own undo is not used.

const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);

// KDL v2, enough to colour it: comments, strings, keywords, numbers,
// properties (`name=`), node names (the first word of a node) and other words.
const TOKEN = /(\/\/.*$)|(\/-)|(#?"(?:[^"\\]|\\.)*"#?)|([A-Za-z_][\w+\-.]*)(=)|(#true|#false|#null|#inf|#-inf|#nan)|(-?\d+(?:\.\d+)?(?![\w.]))|([A-Za-z_][\w+\-.]*)|([{};])/g;

/** One line as HTML spans. */
export function highlight(line) {
  let out = '', last = 0, atNode = true;
  for (const m of line.matchAll(TOKEN)) {
    out += esc(line.slice(last, m.index));
    last = m.index + m[0].length;
    if (m[1]) out += `<span class="tk-c">${esc(m[1])}</span>`;
    else if (m[2]) { out += '<span class="tk-c">/-</span>'; atNode = true; continue; }
    else if (m[3]) out += `<span class="tk-s">${esc(m[3])}</span>`;
    else if (m[4]) out += `<span class="tk-k">${esc(m[4])}</span>=`;
    else if (m[6]) out += `<span class="tk-v">${esc(m[6])}</span>`;
    else if (m[7]) out += `<span class="tk-v">${esc(m[7])}</span>`;
    else if (m[8]) out += atNode ? `<span class="tk-n">${esc(m[8])}</span>` : esc(m[8]);
    else if (m[9]) { out += esc(m[9]); atNode = true; continue; }
    atNode = false;
  }
  return out + esc(line.slice(last));
}

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

/** A pure Tab/Shift-Tab edit: text and the selection after it. */
export function indentSelection(text, a, b, out = false) {
  const start = text.lastIndexOf('\n', a - 1) + 1;
  let end = text.indexOf('\n', b);
  if (end < 0) end = text.length;
  const before = text.slice(start, end);
  let removedFirst = 0;
  const changed = before.split('\n').map((line, i) => {
    if (!out) return `    ${line}`;
    const n = Math.min(4, (line.match(/^ */) || [''])[0].length);
    if (i === 0) removedFirst = n;
    return line.slice(n);
  }).join('\n');
  let selStart, selEnd;
  if (a === b && !out) selStart = selEnd = a + 4;
  else if (a === b) selStart = selEnd = Math.max(start, a - removedFirst);
  else {
    const delta = out ? changed.length - before.length : 4 * before.split('\n').length;
    selStart = out ? Math.max(start, a - removedFirst) : a + 4;
    selEnd = b + delta;
  }
  return { text: text.slice(0, start) + changed + text.slice(end), start: selStart, end: selEnd };
}

/** A pure Enter edit: carry indentation and add one level after `{`. */
export function newlineAt(text, a, b) {
  const start = text.lastIndexOf('\n', a - 1) + 1;
  const line = text.slice(start, a);
  const base = (line.match(/^ */) || [''])[0];
  const insert = `\n${base}${line.trimEnd().endsWith('{') ? '    ' : ''}`;
  return { text: text.slice(0, a) + insert + text.slice(b), caret: a + insert.length };
}

export class TextPane {
  /**
   * `code` is the scrolling element.  Callbacks: onCaretLine(n) when the
   * caret moves to a line by a click or a key, onHoverLine(n|null),
   * onEdit(text, kind) after the user changes the text, onReadOnly() when
   * they try to type into a file they cannot write, and onKey(e) first, for
   * completion (return true when it took the key).
   */
  constructor(code, cb) {
    this.code = code;
    this.cb = cb;
    this.rows = [];
    this.file = null;
    this.text = null;
    this.writable = false;
    this.marks = {};
    const ed = document.createElement('div');
    ed.className = 'ed';
    this.hl = document.createElement('div');
    this.hl.className = 'hl';
    this.hl.setAttribute('aria-hidden', 'true');
    const ta = document.createElement('textarea');
    ta.className = 'ta';
    ta.spellcheck = false;
    ta.wrap = 'off';
    ta.setAttribute('autocapitalize', 'off');
    ta.setAttribute('autocomplete', 'off');
    ta.setAttribute('aria-label', 'The document');
    this.ta = ta;
    ed.append(this.hl, ta);
    code.replaceChildren(ed);

    ta.addEventListener('input', () => {
      this.text = ta.value;
      this.render();
      cb.onEdit?.(this.text, 'type');
    });
    ta.addEventListener('keydown', (e) => {
      if (cb.onKey?.(e)) { e.preventDefault(); e.stopPropagation(); return; }
      if (e.key === 'Tab' && this.writable) { // indent, not leave the pane
        e.preventDefault();
        this.indent(e.shiftKey);
      }
      if (e.key === 'Enter' && this.writable) {
        e.preventDefault();
        this.newline();
      }
      if (ta.readOnly && e.key.length === 1 && !e.metaKey && !e.ctrlKey) cb.onReadOnly?.();
    });
    // the menu's Undo and Redo go to the page's history too
    ta.addEventListener('beforeinput', (e) => {
      if (e.inputType === 'historyUndo' || e.inputType === 'historyRedo') {
        e.preventDefault();
        cb.onHistory?.(e.inputType === 'historyUndo' ? -1 : 1);
      }
    });
    ta.addEventListener('paste', () => { if (ta.readOnly) cb.onReadOnly?.(); });
    const caret = () => cb.onCaretLine?.(this.caretLine());
    ta.addEventListener('click', caret);
    ta.addEventListener('keyup', (e) => { if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End' || e.key.startsWith('Page')) caret(); });
    ta.addEventListener('pointermove', (e) => {
      const n = this.lineAtY(e.clientY);
      if (n !== this.hovered) { this.hovered = n; cb.onHoverLine?.(n); }
    });
    ta.addEventListener('pointerleave', () => { this.hovered = null; cb.onHoverLine?.(null); });
  }

  /** Show `text` from `file`; the caret and scroll stay when it is the same file. */
  show(file, text, writable) {
    const same = file === this.file;
    this.writable = !!writable;
    this.ta.readOnly = !writable;
    this.ta.classList.toggle('ro', !writable);
    if (same && text === this.text) return;
    const top = this.code.scrollTop;
    const [s, e] = [this.ta.selectionStart, this.ta.selectionEnd];
    this.file = file;
    this.setText(text);
    if (same) { this.code.scrollTop = top; this.ta.setSelectionRange(Math.min(s, text.length), Math.min(e, text.length)); }
    else { this.code.scrollTop = 0; this.ta.setSelectionRange(0, 0); }
  }

  /** Replace the text (a change made elsewhere: the view, undo); the caret stays near where it was. */
  setText(text, caret = null) {
    const [s, e] = [this.ta.selectionStart, this.ta.selectionEnd];
    this.text = text;
    this.ta.value = text;
    if (caret != null) this.ta.setSelectionRange(caret, caret);
    else this.ta.setSelectionRange(Math.min(s, text.length), Math.min(e, text.length));
    this.render();
  }

  /** Type `s` at the caret, as if typed (the textarea's own undo is not used). */
  insert(s, from = this.ta.selectionStart, to = this.ta.selectionEnd) {
    const t = this.ta.value;
    this.setText(t.slice(0, from) + s + t.slice(to), from + s.length);
    this.cb.onEdit?.(this.text, 'type');
  }

  /** Tab/Shift-Tab: indent the selected lines, or the caret's line. */
  indent(out = false) {
    const r = indentSelection(this.ta.value, this.ta.selectionStart, this.ta.selectionEnd, out);
    this.setText(r.text);
    this.ta.setSelectionRange(r.start, r.end);
    this.cb.onEdit?.(this.text, 'type');
  }

  /** Enter carries the line's indentation; after `{` it enters one level. */
  newline() {
    const r = newlineAt(this.ta.value, this.ta.selectionStart, this.ta.selectionEnd);
    this.setText(r.text, r.caret);
    this.cb.onEdit?.(this.text, 'type');
  }

  clear() { this.file = null; this.text = null; this.ta.value = ''; this.render(); }

  render() {
    const lines = (this.text ?? '').split('\n');
    const frag = document.createDocumentFragment();
    this.rows = lines.map((line, i) => {
      const d = document.createElement('div');
      d.className = 'ln';
      d.dataset.n = i + 1;
      d.innerHTML = highlight(line) || ' ';
      frag.append(d);
      return d;
    });
    this.hl.replaceChildren(frag);
    this.applyMarks();
  }

  caretLine() { return this.ta.value.slice(0, this.ta.selectionStart).split('\n').length; }
  /** The caret's line and column (both from 1) and the line's text. */
  caret() {
    const t = this.ta.value, i = this.ta.selectionStart;
    const start = t.lastIndexOf('\n', i - 1) + 1;
    const end = t.indexOf('\n', i);
    return { index: i, line: t.slice(0, i).split('\n').length, col: i - start + 1, start, before: t.slice(start, i), after: t.slice(i, end < 0 ? t.length : end) };
  }

  lineH() { return this.rows[0]?.offsetHeight || 22; }
  lineAtY(clientY) {
    const r = this.hl.getBoundingClientRect();
    const n = Math.floor((clientY - r.top) / this.lineH()) + 1;
    return n >= 1 && n <= this.rows.length ? n : null;
  }
  /** Where the caret is on screen (its bottom left), for the completion list. */
  caretXY() {
    const c = this.caret();
    const row = this.rows[c.line - 1];
    if (!row) return null;
    if (!this.chW) {
      const m = document.createElement('span');
      m.textContent = 'x'.repeat(40);
      m.style.cssText = 'position:absolute;visibility:hidden;white-space:pre';
      row.append(m);
      this.chW = m.getBoundingClientRect().width / 40;
      m.remove();
    }
    const r = row.getBoundingClientRect();
    const gutter = parseFloat(getComputedStyle(this.ta).paddingLeft) || 50;
    return { x: r.left + gutter + (c.col - 1) * this.chW, y: r.bottom };
  }

  focus() { this.ta.focus({ preventScroll: true }); }

  /** Mark lines: `sel` and `hov` are [first, last] (1-based) or null; `errs` line numbers. */
  mark(sel, hov, errs = []) {
    this.marks = { sel, hov, errs };
    this.applyMarks();
  }

  applyMarks() {
    const { sel, hov, errs } = this.marks;
    const inside = (r, n) => r && n >= r[0] && n <= r[1];
    this.rows.forEach((d, i) => {
      const n = i + 1;
      d.classList.toggle('sel', !!inside(sel, n));
      d.classList.toggle('hov', !inside(sel, n) && !!inside(hov, n));
      d.classList.toggle('err', !!errs?.includes(n));
    });
  }

  /** Scroll so line `n` is in view, unless it already is. */
  reveal(n) {
    const d = this.rows[n - 1];
    if (!d) return;
    const c = this.code, top = d.offsetTop, h = d.offsetHeight;
    if (top < c.scrollTop + 8 || top + h > c.scrollTop + c.clientHeight - 24) {
      c.scrollTo({ top: Math.max(0, top - c.clientHeight / 3), behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
    }
  }
}
