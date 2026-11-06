// The property sheet: the selected entry's controls fill the right sidebar
// (docs/design/editor.html). Every number in a value is
// scrubbed by dragging it sideways (Shift for fine steps) or typed after a
// click; a word control is a row of its words; a switch flips.  Each change
// is a change to the document's text, made through `api`:
//
//   api.writable()                      may this file be changed?
//   api.replace(start, len, text, end)  replace bytes [start, start+len) of
//                                       the document; `end` when a drag ends
//   api.set(at, name, value)            write a control on the node at byte
//                                       `at` (value null: take it out)
//   api.makeMine()                      offer a copy of a library tile

const STEP = { mm: 0.1, level: 0.1, degrees: 1, percent: 1, count: 1, 'per square': 0.05, number: 0.05 };
const PX = { count: 10, degrees: 3, percent: 3 }; // pixels a step takes; 4 otherwise
const NUM = /-?\d+(?:\.\d+)?/g;
const enc = new TextEncoder();
const bytes = (s) => enc.encode(s).length;
const decimals = (s) => (s.includes('.') ? s.split('.')[1].length : 0);

/**
 * The value part of a written control and where it starts in its text.
 * The server gives the value's own span (`"6..10"`); a whole `name=value`
 * or child line `name value` is taken apart too.
 */
export function valueOf(text, name) {
  if (text.startsWith(`${name}=`)) return { at: name.length + 1, text: text.slice(name.length + 1) };
  const m = text.match(new RegExp(`^${name.replace(/[.+-]/g, '\\$&')}\\s+`));
  if (!m) return { at: 0, text };
  const v = text.slice(m[0].length).match(/^(#?"(?:[^"\\]|\\.)*"#?|[^\s{;]+)/);
  return { at: m[0].length, text: v ? v[0] : '' };
}

/** A typed value as JSON for the server: a number, a switch, or a string (quotes taken off). */
export function parseValue(raw) {
  const t = raw.trim();
  if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
  if (t === '#true' || t === 'true') return true;
  if (t === '#false' || t === 'false') return false;
  return t.replace(/^#?"|"#?$/g, '');
}

/** Button choices stay direct only when they are certain to fit on one row. */
export function wordControlNeedsSelect(words) {
  return words.length > 2 || words.reduce((n, word) => n + word.word.length, 0) > 18;
}

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

export class Lens {
  constructor(root, api) {
    this.root = root;
    this.api = api;
    this.more = false;
    this.drag = null;
    this.hide();
  }

  hide() {
    this.model = null;
    this.root.hidden = false;
    this.root.replaceChildren();
    const empty = el('div', 'empty');
    empty.append(el('b', '', 'Nothing selected'), 'Click something on the tile or put the cursor in its line to inspect and change it.');
    this.root.append(empty);
  }
  get dragging() { return !!this.drag; }

  /**
   * Show `model`: {title, kind (the element it edits), href, note, actions, groups: [{heading, at, element, schema, written: [setting]}]},
   * a setting being {name, text, span: [byte offset, length], file}.
   */
  show(model) {
    if (this.drag) { this.pending = model; return; } // not under a drag
    this.model = model;
    const r = this.root;
    r.replaceChildren();
    r.hidden = false;
    const head = el('div', 'lt');
    const heading = el('div', 'heading');
    heading.append(el('span', 'cap', model.kind || 'Properties'), el('b', '', model.title));
    head.append(heading);
    if (model.href) {
      const a = el('a', '', 'Help ↗');
      a.href = model.href; a.target = '_blank'; a.rel = 'noopener';
      a.title = `Help for ${model.title}`;
      head.append(a);
    }
    const body = el('div', 'body');
    r.append(head, body);
    if (!this.api.writable()) {
      const n = el('div', 'note');
      n.append('A library tile: ');
      const b = el('button', 'linkish', 'make it yours');
      b.onclick = () => this.api.makeMine();
      n.append(b, ' to change it.');
      body.append(n);
    }
    if (model.note) body.append(el('div', 'note', model.note));
    let hidden = 0;
    for (const g of model.groups) {
      if (g.heading) body.append(el('div', 'grp', g.heading));
      const written = new Map(g.written.map((s) => [s.name, s]));
      const controls = g.schema ? g.schema.controls : g.written.map((s) => ({ name: s.name }));
      for (const c of controls) {
        const s = written.get(c.name);
        if (!s && (c.expert || c.taken === false) && !this.more) { hidden++; continue; }
        body.append(this.row(g, c, s));
      }
      // written controls the schema does not know (a definition's arguments)
      if (g.schema) for (const s of g.written) if (!g.schema.controls.some((c) => c.name === s.name)) body.append(this.row(g, { name: s.name }, s));
    }
    if (hidden || this.more) {
      const b = el('button', 'linkish more', this.more ? 'fewer controls' : `${hidden} more control${hidden === 1 ? '' : 's'}`);
      b.onclick = () => { this.more = !this.more; this.show(this.model); };
      body.append(b);
    }
    if (model.actions?.length) {
      const actions = el('div', 'entry-actions');
      for (const action of model.actions) {
        const b = el('button', `btn${action.danger ? ' danger' : ''}`, action.label);
        b.type = 'button'; b.title = action.title; b.onclick = action.run;
        actions.append(b);
      }
      r.append(actions);
    }
  }

  row(g, c, s) {
    const ro = !this.api.writable() || (s && s.file);
    const row = el('div', `row${s ? '' : ' faint'}`);
    const nm = el('span', 'nm', c.name);
    nm.title = [c.about, c.unit && `in ${c.unit}`, c.limits && `${c.limits[0]} to ${c.limits[1]}`].filter(Boolean).join(' · ');
    row.append(nm);
    const cell = el('span', 'cell');
    row.append(cell);
    const v = s ? valueOf(s.text, c.name) : null;
    const shown = v ? v.text : (c.default ?? '');
    const words = c.words?.length ? c.words : null;
    if (words) {
      const cur = shown.replace(/^"|"$/g, '');
      if (wordControlNeedsSelect(words)) {
        const select = el('select', 'word-select');
        select.disabled = ro;
        if (!words.some((w) => w.word === cur)) {
          const option = el('option', '', 'not set'); option.value = ''; option.disabled = true;
          select.append(option);
        }
        for (const w of words) {
          const option = el('option', '', w.word);
          option.value = w.word; option.title = w.about || '';
          select.append(option);
        }
        select.value = words.some((w) => w.word === cur) ? cur : '';
        select.title = words.find((w) => w.word === cur)?.about || c.about || '';
        select.onchange = () => this.api.set(g.at, c.name, select.value);
        cell.append(select);
      } else {
        const box = el('span', 'words');
        for (const w of words) {
          const b = el('button', w.word === cur ? 'on' : '', w.word);
          b.title = w.about || '';
          b.disabled = ro;
          b.onclick = () => this.api.set(g.at, c.name, w.word);
          box.append(b);
        }
        cell.append(box);
      }
    } else if (c.unit === 'switch') {
      const on = /#?true/.test(shown);
      const b = el('button', `flip${on ? ' on' : ''}`, on ? 'on' : 'off');
      b.disabled = ro;
      b.onclick = () => this.api.set(g.at, c.name, !on);
      cell.append(b);
    } else {
      const val = el('span', `val${ro ? ' ro' : ''}`);
      // a quoted value shows without its quotes ("20%" reads as 20%, like 100%);
      // they stay in the text, hidden, so a scrub rewrites them as written
      const pre = (shown.match(/^#?"/) || [''])[0];
      const post = pre ? (shown.slice(pre.length).match(/"#?$/) || [''])[0] : '';
      const body = shown.slice(pre.length, shown.length - post.length);
      if (pre) val.append(el('span', 'q', pre));
      let last = 0;
      for (const m of body.matchAll(NUM)) {
        val.append(body.slice(last, m.index));
        const n = el('span', 'num', m[0]);
        if (!ro) this.scrubber(n, g, c, s, v);
        val.append(n);
        last = m.index + m[0].length;
      }
      val.append(body.slice(last));
      if (post) val.append(el('span', 'q', post));
      if (!shown) val.append(el('span', 'unit', 'not set'));
      if (!ro) val.addEventListener('click', (e) => { if (!this.moved) this.typeIn(val, g, c, s, v, e); });
      cell.append(val);
      if (c.unit && !['number', 'word', 'tag', 'count'].includes(c.unit)) cell.append(el('span', 'unit', c.unit === 'degrees' ? '°' : c.unit === 'percent' ? '' : c.unit));
    }
    if (s && !ro) {
      const x = el('button', 'x', '×');
      x.title = `Take ${c.name} out (back to its default)`;
      x.onclick = () => this.api.set(g.at, c.name, null);
      row.append(x);
    }
    return row;
  }

  /** Drag number `n` sideways; the whole value is rewritten as shown. */
  scrubber(n, g, c, s, v) {
    n.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      this.moved = false;
      if (!s) { this.moved = true; this.api.set(g.at, c.name, parseValue(c.default ?? '0')); return; } // write the default; drag again
      n.setPointerCapture(e.pointerId);
      const start = Number(n.textContent);
      const step = STEP[c.unit] ?? 0.1;
      const dec = Math.max(decimals(n.textContent), decimals(String(step)));
      const valueStart = s.span[0] + bytes(s.text.slice(0, v.at));
      this.drag = { len: bytes(v.text), x: e.clientX };
      n.classList.add('active');
      const move = (ev) => {
        const dx = ev.clientX - this.drag.x;
        if (Math.abs(dx) > 2) this.moved = true;
        if (!this.moved) return;
        const fine = ev.shiftKey ? 0.1 : 1;
        let x = start + Math.round(dx / (PX[c.unit] ?? 4)) * step * fine;
        if (c.limits && !/\.\./.test(v.text)) x = Math.min(c.limits[1], Math.max(c.limits[0], x));
        const txt = String(Number(x.toFixed(fine < 1 ? dec + 1 : dec))); // 7, not 7.0
        if (txt === n.textContent) return;
        n.textContent = txt;
        this.rewrite(n, v, valueStart, false);
      };
      const up = () => {
        n.removeEventListener('pointermove', move);
        n.removeEventListener('pointerup', up);
        n.removeEventListener('pointercancel', up);
        n.classList.remove('active');
        const len = this.drag.len;
        this.drag = null; // ended before the last write, so it is saved
        if (this.moved) this.api.replace(valueStart, len, n.parentElement.textContent.replace(/not set$/, ''), true);
        if (this.pending) { const p = this.pending; this.pending = null; this.show(p); }
      };
      n.addEventListener('pointermove', move);
      n.addEventListener('pointerup', up);
      n.addEventListener('pointercancel', up);
    });
  }

  /** Write the value as the row shows it now over the bytes it had. */
  rewrite(n, v, valueStart, end) {
    const text = n.parentElement.textContent.replace(/not set$/, '');
    this.api.replace(valueStart, this.drag.len, text, end);
    this.drag.len = bytes(text);
  }

  /** Type a value in place of the shown one. */
  typeIn(val, g, c, s, v) {
    const input = el('input', 'edit');
    input.value = v ? v.text : (c.default ?? '');
    input.spellcheck = false;
    val.replaceWith(input);
    input.focus();
    input.select();
    let done = false;
    const finish = (keep) => {
      if (done) return;
      done = true;
      if (keep && input.value !== (v ? v.text : c.default ?? '')) {
        if (s) this.api.replace(s.span[0] + bytes(s.text.slice(0, v.at)), bytes(v.text), input.value.trim(), true);
        else this.api.set(g.at, c.name, parseValue(input.value));
      }
      this.show(this.model);
    };
    input.onkeydown = (e) => {
      if (e.key === 'Enter') finish(true);
      if (e.key === 'Escape') { e.stopPropagation(); finish(false); }
    };
    input.onblur = () => finish(true);
  }
}
