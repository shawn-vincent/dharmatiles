// The card's source: real selectable text in a plain textarea, with a mirror behind it that draws what marks a place in it: an
// underline for each open message (wavy red for an error, yellow for a note: a stripe, never a tile colour), the lit mark's
// span, and the pinned one.  The editor's source is CodeMirror (web/preview/js/text.js, 360 KB); a card has a few lines, so it
// keeps the text area and its own mirror.  Offsets are UTF-16 code units, as the text area counts them.

export class SourcePane {
  /**
   * @param {HTMLElement} host where the pane goes
   * @param {{ text: string, label: string, onInput: (text: string) => void, onCaret: (offset: number) => void,
   *   onHover: (offset: number | null) => void }} o
   */
  constructor(host, { text, label, onInput, onCaret, onHover }) {
    this.host = host;
    host.classList.add('dtx-source');
    this.mirror = document.createElement('pre');
    this.mirror.className = 'dtx-mirror';
    this.mirror.setAttribute('aria-hidden', 'true');
    this.area = document.createElement('textarea');
    this.area.className = 'dtx-area';
    this.area.spellcheck = false;
    this.area.autocapitalize = 'off';
    this.area.setAttribute('autocomplete', 'off');
    this.area.setAttribute('aria-label', label);
    this.area.value = text;
    this.area.rows = Math.min(24, text.split("\n").length);
    host.append(this.mirror, this.area);
    /** @type {{ from: number, to: number, cls: string }[]} */
    this.ranges = [];
    this.area.addEventListener('input', () => { this.#paint(); onInput(this.area.value); });
    this.area.addEventListener('scroll', () => this.#sync());
    const caret = () => onCaret(this.area.selectionStart);
    this.area.addEventListener('keyup', caret);
    this.area.addEventListener('click', caret);
    this.area.addEventListener('focus', caret);
    this.area.addEventListener('blur', () => onCaret(-1));
    this.area.addEventListener('pointermove', (e) => onHover(this.offsetAt(e)));
    this.area.addEventListener('pointerleave', () => onHover(null));
    this.#paint();
  }

  get text() { return this.area.value; }

  /** Replace the whole text (a control wrote the source, or the example was reset). */
  set(text) {
    this.area.value = text;
    this.#paint();
  }

  /**
   * The offset under a pointer event: the text area is monospace and does not wrap, so a point is a line and a column.
   * @param {PointerEvent} e @returns {number | null}
   */
  offsetAt(e) {
    const r = this.area.getBoundingClientRect();
    const cs = getComputedStyle(this.area);
    const pad = { l: parseFloat(cs.paddingLeft), t: parseFloat(cs.paddingTop) };
    const lh = parseFloat(cs.lineHeight);
    const probe = (this.#probe ??= (() => {
      const s = document.createElement('span');
      s.style.cssText = `position:absolute;visibility:hidden;white-space:pre;font:${cs.font}`;
      s.textContent = 'x'.repeat(40);
      this.host.append(s);
      return s;
    })());
    const cw = probe.getBoundingClientRect().width / 40;
    const row = Math.floor((e.clientY - r.top - pad.t + this.area.scrollTop) / lh);
    const lines = this.area.value.split('\n');
    if (row < 0 || row >= lines.length) return null;
    const col = Math.round((e.clientX - r.left - pad.l + this.area.scrollLeft) / cw);
    if (col < 0 || col > lines[row].length) return null;
    return lines.slice(0, row).reduce((n, l) => n + l.length + 1, 0) + col;
  }

  /** @type {HTMLElement | undefined} */
  #probe;

  /** Mark ranges of the text: [{from, to, cls}] with cls `err`, `note`, `lit` or `pin`. */
  mark(ranges) {
    this.ranges = ranges;
    this.#paint();
  }

  /** Put the caret on a line (1-based) and bring it into view: a message's "go to line". */
  showLine(line) {
    const lines = this.area.value.split('\n');
    const at = lines.slice(0, line - 1).reduce((n, l) => n + l.length + 1, 0);
    this.area.focus();
    this.area.setSelectionRange(at, at + (lines[line - 1]?.length ?? 0));
  }

  #sync() {
    this.mirror.scrollTop = this.area.scrollTop;
    this.mirror.scrollLeft = this.area.scrollLeft;
  }

  #paint() {
    const text = this.area.value;
    const cuts = new Set([0, text.length]);
    for (const r of this.ranges) { cuts.add(Math.max(0, Math.min(text.length, r.from))); cuts.add(Math.max(0, Math.min(text.length, r.to))); }
    const at = [...cuts].sort((a, b) => a - b);
    this.mirror.textContent = '';
    for (let i = 0; i < at.length - 1; i++) {
      const [a, b] = [at[i], at[i + 1]];
      if (a === b) continue;
      const classes = this.ranges.filter((r) => r.from <= a && r.to >= b).map((r) => r.cls);
      if (!classes.length) { this.mirror.append(text.slice(a, b)); continue; }
      const s = document.createElement('span');
      s.className = classes.map((c) => `m-${c}`).join(' ');
      s.textContent = text.slice(a, b);
      this.mirror.append(s);
    }
    // a final newline would not take up a line in the mirror, so the last empty line is a space
    if (text.endsWith('\n')) this.mirror.append(' ');
    this.#sync();
  }
}
