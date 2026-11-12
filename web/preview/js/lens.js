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
//   api.canMine()                       a library tile: its controls work, and the first change copies it (the api's `set` and `replace` do that)
//   api.checkValue(element, control, typed) -> Promise<string|null>
//                                       the program's check of a typed value
//   api.weighting(query) -> Promise<reply|null>
//                                       the program's weighting curves (gallery.js)

import { weightGallery } from './gallery.js';

// The weighting gallery writes plain numbers, and a document's values carry
// their units: until it writes units it stays parked.
const GALLERY_WRITES_UNITS = false;
import { sizeGrid } from './size-grid.js';
import { asNumber, asRange, debounce, formOf, isNumeric, kdlString, unquote, valueItems, written } from './value.js';

/** Does a written value carry its own unit (`3.5dmm`, `20%`, `45deg`)? */
export const carriesUnit = (shown) => /\d\s*[a-zA-Z%°]/.test(String(shown ?? ''));

// how long typing pauses before the program is asked about the value
const CHECK_PAUSE_MS = 200;
// and before the weighting curves are asked for
const GALLERY_PAUSE_MS = 120;

export { kdlString, unquote };

const STEP = { mm: 0.1, level: 0.1, degrees: 1, percent: 1, count: 1, 'per square': 0.05, number: 0.05 };
const PX = { count: 10, degrees: 3, percent: 3 }; // pixels a step takes; 4 otherwise
const NUM = /-?\d+(?:\.\d+)?/g;
// a die showing five
const DICE = '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="1.5" width="13" height="13" rx="3" fill="none" stroke="currentColor" stroke-width="1.4"/>'
  + [[5, 5], [11, 5], [8, 8], [5, 11], [11, 11]].map(([x, y]) => `<circle cx="${x}" cy="${y}" r="1.25" fill="currentColor"/>`).join('') + '</svg>';
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
    this.field = null; // the field being typed in, if any: the sheet is held, not redrawn under it
    this.hide();
  }

  hide() {
    this.model = null;
    this.identity = null;
    this.field = null;
    this.pending = null;
    this.root.hidden = false;
    this.root.replaceChildren();
    const empty = el('div', 'empty');
    empty.append(el('b', '', this.emptyTitle ?? 'Nothing selected'), this.emptyText ?? 'Click something on the tile or put the cursor in its line to inspect and change it.');
    this.root.append(empty);
  }
  get dragging() { return !!this.drag; }

  /** A control's default as the document writes it (a length with its unit). */
  dflt(c) { return c.default1 ?? c.default; }

  /**
   * Show `model`: {title, kind (the element it edits), href, note, actions, groups: [{heading, at, element, schema, written: [setting]}]},
   * a setting being {name, text, span: [byte offset, length], file}.
   */
  show(model) {
    if (this.drag || this.field) { this.pending = model; return; } // not under a drag or a field being typed in: shown when it ends
    // the same thing drawn again (a value changed) keeps where it was scrolled to
    const identity = `${model.kind}|${model.title}`;
    const kept = identity === this.identity ? this.root.querySelector('.body')?.scrollTop ?? 0 : 0;
    this.identity = identity;
    this.model = model;
    const r = this.root;
    r.replaceChildren();
    r.hidden = false;
    const head = el('div', model.slim ? 'lt slim' : 'lt');
    const heading = el('div', 'heading');
    heading.append(el('span', 'cap', model.kind || 'Properties'));
    if (!model.slim) heading.append(el('b', '', model.title));
    head.append(heading);
    if (model.href) {
      const a = el('a', '', 'Help ↗');
      a.href = model.href; a.target = '_blank'; a.rel = 'noopener';
      a.title = `Help for ${model.title}`;
      head.append(a);
    }
    const body = el('div', 'body');
    r.append(head, body);
    if (model.note) body.append(el('div', 'note', model.note));
    let hidden = 0;
    for (const g of model.groups) {
      if (g.heading) {
        const h = el('div', 'grp');
        h.append(el('span', '', g.heading));
        if (g.remove && this.editable()) {
          const x = el('button', 'x', '×');
          x.type = 'button'; x.title = g.remove.title; x.setAttribute('aria-label', g.remove.title);
          x.onclick = g.remove.run;
          h.append(x);
        }
        body.append(h);
      }
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
    if (model.add && this.editable()) body.append(this.addMenu(model.add));
    if (model.tail) body.append(model.tail);
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
    body.scrollTop = kept;
  }

  /** A menu that adds something: `add` is {label, items: [{label, title, disabled, run}]}; choosing an item runs it, then the menu is ready again. */
  addMenu(add) {
    const select = el('select', 'word-select add');
    select.setAttribute('aria-label', add.label);
    const first = el('option', '', `${add.label}…`);
    first.value = ''; first.disabled = true; first.selected = true;
    select.append(first);
    add.items.forEach((it, i) => {
      const o = el('option', '', it.label);
      o.value = String(i); o.title = it.title || ''; o.disabled = !!it.disabled;
      select.append(o);
    });
    select.onchange = () => {
      const it = add.items[Number(select.value)];
      select.value = '';
      if (it && !it.disabled) it.run();
    };
    return select;
  }

  /** May a control be used? On a library tile it may: the first change copies the tile. */
  editable() { return this.api.writable() || !!this.api.canMine?.(); }

  /** Write control `name` of group `g`: through the group's own writer when it has one. */
  put(g, name, value) { return g.set ? g.set(name, value) : this.api.set(g.at, name, value); }

  row(g, c, s) {
    const ro = !this.editable() || (s && s.file);
    const row = el('div', `row${s ? '' : ' faint'}`);
    row.addEventListener('mouseenter', () => this.api.onFieldHover?.(c.name, true));
    row.addEventListener('mouseleave', () => this.api.onFieldHover?.(c.name, false));
    const nm = el('span', 'nm', c.name);
    nm.title = [c.about, c.unit && `in ${c.unit}`, c.takes].filter(Boolean).join(' · ');
    row.append(nm);
    const cell = el('span', 'cell');
    row.append(cell);
    const v = s ? valueOf(s.text, c.name) : null;
    const shown = v ? v.text : (this.dflt(c) ?? '');
    const words = c.words?.length ? c.words : null;
    if (words && c.unit === 'several') {
      const cur = new Set(shown.split(/\s+/).filter(Boolean).map((w) => unquote(w)));
      const box = el('span', 'words');
      for (const w of words) {
        const on = cur.has(w.word);
        const b = el('button', on ? 'on' : '', w.word);
        b.title = w.about || '';
        b.disabled = ro;
        b.setAttribute('aria-pressed', on);
        b.onclick = () => {
          const next = new Set(cur);
          if (on) next.delete(w.word); else next.add(w.word);
          this.put(g, c.name, words.map((x) => x.word).filter((x) => next.has(x)));
        };
        box.append(b);
      }
      cell.append(box);
    } else if (words && !isNumeric(c)) {
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
        select.onchange = () => this.put(g, c.name, select.value);
        cell.append(select);
      } else {
        const box = el('span', 'words');
        for (const w of words) {
          const b = el('button', w.word === cur ? 'on' : '', w.word);
          b.title = w.about || '';
          b.disabled = ro;
          b.onclick = () => this.put(g, c.name, w.word);
          box.append(b);
        }
        cell.append(box);
      }
    } else if (c.unit === 'switch') {
      const on = /#?true/.test(shown);
      const b = el('button', `flip${on ? ' on' : ''}`, on ? 'on' : 'off');
      b.disabled = ro;
      b.onclick = () => this.put(g, c.name, !on);
      cell.append(b);
    } else {
      const val = el('span', `val${ro ? ' ro' : ''}`);
      // a quoted value shows without its quotes ("20%" reads as 20%, like 100%);
      // they stay in the text, hidden, so a scrub rewrites them as written
      const pre = c.unit === 'text' || c.unit === 'raw' ? '' : (shown.match(/^#?"/) || [''])[0];
      const post = pre ? (shown.slice(pre.length).match(/"#?$/) || [''])[0] : '';
      // a description reads as its words: no quotes, no escapes
      const body = c.unit === 'text' ? unquote(shown) : shown.slice(pre.length, shown.length - post.length);
      if (pre) val.append(el('span', 'q', pre));
      let last = 0;
      for (const m of c.unit === 'text' || c.viaSet ? [] : body.matchAll(NUM)) {
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
      // a seed's dice: another arrangement, at random
      if (c.name === 'seed' && !ro) {
        const d = el('button', 'dice');
        d.type = 'button'; d.title = 'Roll a new seed: another arrangement'; d.setAttribute('aria-label', 'Roll a new seed');
        d.innerHTML = DICE;
        d.onclick = () => this.put(g, c.name, 1 + Math.floor(Math.random() * 9999));
        cell.append(d);
      }
      // a value written with its unit (3.5dmm..6.5dmm) is not given the unit again
      if (c.unit && !carriesUnit(shown) && !['number', 'word', 'tag', 'count', 'text', 'raw', 'size', 'height', 'several'].includes(c.unit)) cell.append(el('span', 'unit', c.unit === 'degrees' ? '°' : c.unit === 'percent' ? '' : c.unit));
    }
    if (s && !ro && !c.required) {
      const x = el('button', 'x', '×');
      x.title = `Take ${c.name} out (back to its default)`;
      x.onclick = () => this.put(g, c.name, null);
      row.append(x);
    }
    // a control with more to show opens its own level
    if (this.api.detail?.(c)) {
      const d = el('button', 'chev', '›');
      d.type = 'button'; d.title = `More about ${c.name}`; d.setAttribute('aria-label', `More about ${c.name}`);
      d.onclick = () => this.api.openControl?.(Math.max(0, this.model?.groups.indexOf(g) ?? 0), c.name);
      row.append(d);
    }
    return row;
  }

  /** Drag number `n` sideways; the whole value is rewritten as shown. */
  scrubber(n, g, c, s, v) {
    n.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      this.moved = false;
      if (!s) { this.moved = true; this.put(g, c.name, parseValue(this.dflt(c) ?? '0')); return; } // write the default; drag again
      n.setPointerCapture(e.pointerId);
      this.api.onField?.(c.name, true);
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
        if (c.limits && !/\.\./.test(v.text)) x = Math.min(c.limits[1] ?? Infinity, Math.max(c.limits[0], x));
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
        this.api.onField?.(c.name, false);
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
    const text = c.unit === 'text'; // plain words: typed without quotes, written as a KDL string
    // a number, range or expression: typed bare, quoted as it is written
    const num = isNumeric(c);
    const shownText = v ? v.text : (this.dflt(c) ?? '');
    input.value = text || num ? unquote(shownText) : shownText;
    const was = input.value;
    input.spellcheck = false;
    val.replaceWith(input);
    this.api.onField?.(c.name, true);
    const field = this.field = {};
    input.focus();
    input.select();
    let done = false, close = null;
    // the gallery's changes land in the source as they are made (as a drag's
    // do): the span the value has now, and the sheet is held, not redrawn
    // under the field, until the edit ends
    const live = { start: s && v ? s.span[0] + bytes(s.text.slice(0, v.at)) : 0, len: v ? bytes(v.text) : 0, wrote: false };
    const write = num && s && v && !c.viaSet && !text ? (typed) => {
      const out = written(typed);
      this.drag ??= { len: 0, x: 0 };
      this.api.replace(live.start, live.len, out, false);
      live.len = bytes(out);
      live.wrote = true;
    } : null;
    const finish = (keep) => {
      if (done) return;
      done = true;
      const current = this.field === field; // else the sheet was cleared under the field
      if (current) this.field = null;
      close?.();
      this.api.onField?.(c.name, false);
      if (live.wrote) {
        // the source holds what was last written; it ends as one step of history, or is put back
        this.drag = null;
        this.api.replace(live.start, live.len, keep ? written(input.value) : v.text, true);
      } else if (keep && input.value !== was) {
        const typed = text ? input.value.trim() : null;
        if (text && !typed) this.put(g, c.name, null);
        else if (s && !c.viaSet) this.api.replace(s.span[0] + bytes(s.text.slice(0, v.at)), bytes(v.text), text ? kdlString(typed) : num ? written(input.value) : input.value.trim(), true);
        else this.put(g, c.name, text ? typed : parseValue(input.value));
      }
      if (!current) return;
      const held = this.pending;
      this.pending = null;
      this.show(held ?? this.model);
    };
    let pop = null;
    input.onkeydown = (e) => {
      if (pop?.key(e)) { e.preventDefault(); return; }
      if (e.key === 'Enter') finish(true);
      if (e.key === 'Escape') { e.stopPropagation(); finish(false); }
    };
    input.onblur = () => finish(true);
    if (c.unit === 'size') close = sizeGrid(input, () => finish(true));
    else if (num) { pop = this.valuePop(input, c, g.schema?.name, write); close = pop.close; }
  }

  /**
   * The helper under a typed value: what the control takes, the program's
   * check of what is typed (asked of the server; nothing is checked here),
   * and what can go at the caret (its words).  Arrows pick, Enter or Tab
   * takes.  Returns {key(event) -> taken, close}.
   */
  valuePop(input, c, element, write = null) {
    const pop = el('div', 'value-pop');
    const takes = el('div', 'vp-takes', [[c.unit, c.takes].filter(Boolean).join(', '), this.dflt(c) && `default ${this.dflt(c)}`].filter(Boolean).join(' · '));
    const check = el('div', 'vp-check');
    const forms = el('div', 'words vp-forms');
    const list = el('div', 'vp-list');
    list.setAttribute('role', 'listbox');
    pop.append(takes, check);
    let one = null, range = null, weighted = null, gallery = null;
    if (c.ranges) {
      one = el('button', '', 'one number');
      range = el('button', '', 'a range');
      one.onclick = () => { input.value = asNumber(input.value); update(); };
      range.onclick = () => { input.value = asRange(input.value); update(); };
      forms.append(one, range);
      // a draw takes odds along its range: the gallery of weighting functions
      // (parked: the gallery writes plain numbers, and a value carries its units, so it is typed)
      if (c.weighted && this.api.weighting && GALLERY_WRITES_UNITS) {
        weighted = el('button', '', 'weighted');
        weighted.onclick = () => {
          if (formOf(input.value) === 'number') input.value = asRange(input.value);
          update();
          // from a plain range: lean low, then the gallery and its sliders take over
          if (gallery.function === null || gallery.function === 'flat') gallery.pickWeighted();
        };
        forms.append(weighted);
        gallery = weightGallery({
          api: this.api,
          input,
          change: () => { update(false); write?.(input.value); },
          refuse: (message) => { check.textContent = message; check.hidden = false; },
          resized: () => place(),
        });
        pop.append(forms, gallery.node);
      } else pop.append(forms);
    }
    pop.append(list);
    let s = { items: [] }, i = 0, asked = 0;
    const askLater = debounce((typed) => {
      const ask = ++asked;
      this.api.checkValue?.(element, c.name, typed).then((why) => {
        if (ask !== asked) return;
        check.textContent = why || '';
        check.hidden = !why;
      });
    }, CHECK_PAUSE_MS);
    // the program's curves for what is typed, asked once the typing pauses
    const galleryLater = debounce(() => gallery?.refresh().then(() => weighted?.classList.toggle('on', gallery.function !== null && gallery.function !== 'flat')), GALLERY_PAUSE_MS);
    const take = () => {
      const it = s.items[i];
      if (!it) return false;
      input.value = input.value.slice(0, s.from) + it.insert + input.value.slice(s.to);
      const at = s.from + it.insert.length;
      input.setSelectionRange(at, at);
      update();
      return true;
    };
    const draw = () => {
      list.replaceChildren();
      s.items.forEach((it, k) => {
        const d = el('div', `it${k === i ? ' on' : ''}`);
        d.setAttribute('role', 'option');
        d.setAttribute('aria-selected', k === i);
        d.append(el('span', 'nm', it.label), el('span', 'ds', it.about || it.kind));
        d.onpointerdown = (e) => { e.preventDefault(); i = k; take(); };
        list.append(d);
      });
      list.hidden = !s.items.length;
    };
    const update = (again = true) => {
      s = valueItems(input.value, input.selectionStart ?? input.value.length, c);
      i = 0;
      draw();
      // the program's answer, for what is typed now (a later one wins),
      // asked once the typing pauses
      askLater(input.value);
      const form = formOf(input.value);
      one?.classList.toggle('on', form === 'number');
      range?.classList.toggle('on', form === 'range');
      if (gallery) {
        gallery.open(form === 'range' || form === 'expression');
        if (again) galleryLater();
        const w = gallery.function;
        weighted.classList.toggle('on', form !== 'number' && w !== null && w !== 'flat');
      }
    };
    // the field keeps the focus: a click in the helper is not a blur
    pop.addEventListener('pointerdown', (e) => e.preventDefault());
    pop.addEventListener('mousedown', (e) => e.preventDefault());
    const place = () => {
      const f = input.getBoundingClientRect();
      pop.style.width = `${Math.max(f.width, gallery && !gallery.node.hidden ? 300 : 240)}px`;
      pop.style.left = `${Math.max(4, Math.min(f.left, window.innerWidth - pop.offsetWidth - 4))}px`;
      const below = f.bottom + 4 + pop.offsetHeight <= window.innerHeight - 4;
      pop.style.top = `${below ? f.bottom + 4 : Math.max(4, f.top - 4 - pop.offsetHeight)}px`;
    };
    input.addEventListener('input', () => { update(); place(); });
    input.addEventListener('click', () => { update(); place(); });
    window.addEventListener('scroll', place, true);
    document.body.append(pop);
    update();
    place();
    return {
      key: (e) => {
        if (!s.items.length) return false;
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          i = (i + (e.key === 'ArrowDown' ? 1 : s.items.length - 1)) % s.items.length;
          draw();
          return true;
        }
        // Enter takes a half-typed name; a whole value is written
        if (e.key === 'Tab' || (e.key === 'Enter' && s.to > s.from)) return take();
        return false;
      },
      close: () => { askLater.cancel(); galleryLater.cancel(); gallery?.close(); window.removeEventListener('scroll', place, true); pop.remove(); },
    };
  }
}
