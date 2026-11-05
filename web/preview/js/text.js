// The text pane: the document as written, highlighted, one row per line.
// Read-only for now; clicking or hovering a line tells the page, and the
// page marks the lines of whatever is selected or under the pointer.

const esc = (s) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);

// KDL v2, enough to colour it: comments, strings, keywords, numbers,
// properties (`name=`), node names (the first word of a node) and other words.
const TOKEN = /(\/\/.*$)|(\/-)|(#?"(?:[^"\\]|\\.)*"#?)|([A-Za-z_][\w+\-.]*)(=)|(#true|#false|#null|#inf|#-inf|#nan)|(-?\d+(?:\.\d+)?(?![\w.]))|([A-Za-z_][\w+\-.]*)|([{};])/g;

/** One line as HTML spans. `start` says whether the line begins a node (so its first word is the node's name). */
export function highlight(line) {
  let out = '', last = 0, atNode = true;
  for (const m of line.matchAll(TOKEN)) {
    const gap = line.slice(last, m.index);
    out += esc(gap);
    last = m.index + m[0].length;
    if (m[1]) out += `<span class="tk-c">${esc(m[1])}</span>`;
    else if (m[2]) { out += `<span class="tk-c">/-</span>`; atNode = true; continue; }
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

export class TextPane {
  constructor(code, { onLine, onHoverLine }) {
    this.code = code;
    this.rows = [];
    this.file = null;
    this.text = null;
    code.addEventListener('click', (e) => {
      const row = e.target.closest('.ln');
      if (row) onLine(Number(row.dataset.n));
    });
    code.addEventListener('pointermove', (e) => {
      const row = e.target.closest('.ln');
      const n = row ? Number(row.dataset.n) : null;
      if (n !== this.hovered) { this.hovered = n; onHoverLine(n); }
    });
    code.addEventListener('pointerleave', () => { this.hovered = null; onHoverLine(null); });
  }

  /** Show `text` (from `file`); the scroll is kept when it is the same file again. */
  show(file, text) {
    if (file === this.file && text === this.text) return;
    const same = file === this.file;
    const top = this.code.scrollTop;
    this.file = file; this.text = text;
    const frag = document.createDocumentFragment();
    this.rows = text.split('\n').map((line, i) => {
      const d = document.createElement('div');
      d.className = 'ln';
      d.dataset.n = i + 1;
      d.innerHTML = `<span>${highlight(line) || ' '}</span>`;
      frag.append(d);
      return d;
    });
    this.code.replaceChildren(frag);
    this.code.scrollTop = same ? top : 0;
    this.applyMarks();
  }

  clear() { this.file = null; this.text = null; this.rows = []; this.code.replaceChildren(); }

  /** Mark lines: `sel` and `hov` are [first, last] (1-based) or null; `errs` a list of line numbers. */
  mark(sel, hov, errs = []) {
    this.marks = { sel, hov, errs };
    this.applyMarks();
  }

  applyMarks() {
    const { sel, hov, errs } = this.marks || {};
    const inside = (r, n) => r && n >= r[0] && n <= r[1];
    this.rows.forEach((d, i) => {
      const n = i + 1;
      d.classList.toggle('sel', inside(sel, n));
      d.classList.toggle('hov', !inside(sel, n) && inside(hov, n));
      d.classList.toggle('err', !!errs?.includes(n));
    });
  }

  /** Scroll so line `n` is in view, unless it already is. */
  reveal(n) {
    const d = this.rows[n - 1];
    if (!d) return;
    const c = this.code, top = d.offsetTop - c.offsetTop, h = d.offsetHeight;
    if (top < c.scrollTop + 8 || top + h > c.scrollTop + c.clientHeight - 24) {
      c.scrollTo({ top: Math.max(0, top - c.clientHeight / 3), behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
    }
  }
}
