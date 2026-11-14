// The property sheet: the controls of the selected thing (a stack entry, a line, a region, the tile itself) fill the
// panel.  Every number in a value is scrubbed by dragging it sideways (Shift for fine steps) or typed after a click; a
// word control is a row of its words; a switch flips.  Each change is a change to the document's text, made through
// `api`:
//
//   api.writable()                      may this file be changed?
//   api.scrub(at, name, label)          a value changed live, a step at a time, made by the program: {to(value), end(value), cancel()}
//   api.set(at, name, value)            write a control on the node at byte `at` (value null: take it out)
//   api.canMine()                       a library tile: its controls work, and the first change copies it
//   api.checkValue(element, control, typed) -> Promise<string|null>   the program's check of a typed value
//   api.onField(name, on), api.onFieldHover(name, on)                 a field in use, a row under the pointer
//   api.detail(c), api.openControl(group, name)                       a control with more to show opens its own level
//
// `Sheet({model, api})` is a function of the sheet's model (js/editor/sheet.js builds it from the document: `{title, kind,
// href, note, actions, groups: [{heading, at, schema, written}], add, tail, slim}`, a setting being `{name, text, span,
// file}`).  What the user is in the middle of is the row's own: the number being scrubbed, the value being typed.  A new
// state of the document is drawn over the sheet as it is, so neither is lost or held back.

import { carriesUnit, parseValue, valueOf, wordControlNeedsSelect } from '../lens.js';
import { asNumber, asRange, debounce, isNumeric, unquote, valueItems } from '../value.js';
import { html, render, useEffect, useRef, useState } from './html.js';
import { Icon } from './icons.js';
import { SizeGridBox, parseSize } from './size-grid.js';
import { ValuePopBox } from './value-pop.js';
import { SeedDice, SeedStrip, candidates, newSeed } from './seed-strip.js';

const STEP = { dmm: 0.1, level: 0.1, degrees: 1, percent: 1, count: 1, 'per square': 0.05, number: 0.05 };
const PX = { count: 10, degrees: 3, percent: 3 }; // pixels a step takes; 4 otherwise
const NUM = /-?\d+(?:\.\d+)?/g;
const CHECK_PAUSE_MS = 200; // how long typing pauses before the program is asked about the value
const decimals = (s) => (s.includes('.') ? s.split('.')[1].length : 0);
const dflt = (c) => c.default1 ?? c.default;

/**
 * What an unwritten control shows: `{text, from}`.  An entry's controls come with what the program says each is where nothing is
 * written (`g.defaults`: the value a build uses and what it comes from: "default", "style brick", "parts/edges", or no `from`
 * when `text` says what leaving it out means); the tile's own nodes show their schema's default or its `absent` words.  The
 * page works nothing out.  Null where nothing says, which the row shows as a mistake, never as a blank.
 * @param {{ defaults?: {control: string, text: string, from: string}[] }} g
 * @param {any} c
 * @returns {{text: string, from: string} | null}
 */
export function inheritedOf(g, c) {
  if (g.defaults) {
    const d = g.defaults.find((x) => x.control === c.name);
    return d ? { text: d.text, from: d.from } : null;
  }
  const v = dflt(c);
  if (v) return { text: v, from: 'default' };
  return c.absent ? { text: c.absent, from: '' } : null;
}
/**
 * An element's controls as the sheet's rows: a group's sub-controls are rows of their own, named by their path
 * (`stones/height`) as the document's written controls are, so a row, its written text, the edit of it and its page all
 * answer to one name.  A group has no value of its own to show, so it has no row.
 */
export function rowsOf(/** @type {any[]} */ controls, prefix = '', /** @type {any} */ up = {}) {
  const out = [];
  for (const c of controls) {
    const path = `${prefix}${c.name}`;
    const expert = up.expert || c.expert;
    const untaken = up.untaken || c.taken === false;
    if (c.unit) out.push({ ...c, name: path, label: path.replace('/', ' '), expert, taken: untaken ? false : c.taken, subs: [] });
    out.push(...rowsOf(c.subs || [], `${path}/`, { expert, untaken }));
  }
  return out;
}
/** What the mark beside an inherited value says. */
const fromText = (/** @type {string} */ from) => (from === 'default' ? 'default' : `from ${from}`);
/** An inherited value in a phrase, for the helper under a field: `default 5dmm`, `2.5dmm .. 3dmm (from style brick)`, or the words for what leaving it out means. */
export const inheritedPhrase = (/** @type {{text: string, from: string} | null} */ inh) => (!inh ? '' : !inh.from ? inh.text : inh.from === 'default' ? `default ${inh.text}` : `${inh.text} (${fromText(inh.from)})`);
/** The value an unwritten control shows to the cell: its text where that is a value, else nothing (the words say what leaving it out means). */
const valueIn = (/** @type {{text: string, from: string} | null} */ inh) => (inh?.from ? inh.text : '');

// a die showing five

// ---- a value typed in place ------------------------------------------------------------------------------------

/** A value being typed: the field, the helper under it (or the size grid), and the writes it makes as it goes. */
function TypeIn({ g, c, s, v, inh, api, put, onDone }) {
  const text = c.unit === 'text'; // plain words: typed without quotes, written as a KDL string
  // a number, range or expression: typed bare, quoted as it is written
  const num = isNumeric(c);
  const shown = v ? v.text : valueIn(inh);
  const initial = text || num ? unquote(shown) : shown;
  const [val, setVal] = useState(initial);
  const [caret, setCaret] = useState(initial.length);
  const [pick, setPick] = useState(0);
  const [check, setCheck] = useState('');
  const input = useRef(/** @type {HTMLInputElement | null} */ (null));
  const mount = useRef(/** @type {HTMLElement | null} */ (null));
  // the field's changes land in the source as they are made (as a drag's do), each a step the program makes and drafts
  // (`api.scrub`); `scrub` is made at the first, null when the source cannot be changed now (undefined: not yet asked)
  const st = useRef({ val: initial, done: false, asked: 0, typed: initial, pointed: null, scrub: /** @type {any} */ (undefined) });
  const items = c.unit === 'size' ? [] : num ? valueItems(val, caret, c).items : [];
  const range = num ? valueItems(val, caret, c) : { items: [], from: 0, to: 0 };
  const write = num && s && v && !c.viaSet && !text ? (typed) => {
    const x = st.current;
    if (!typed.trim()) return; // nothing typed is no value: the field ends by taking the control out, not on the way
    if (x.scrub === undefined) x.scrub = api.scrub?.(g.at, c.name, `Changed ${c.name}`) ?? null;
    x.scrub?.to(parseValue(typed));
  } : null;
  const set = (/** @type {string} */ next, at = next.length) => { st.current.val = next; setVal(next); setCaret(at); setPick(0); };
  const askLater = useRef(debounce((typed) => {
    const ask = ++st.current.asked;
    api.checkValue?.(g.schema?.name, c.name, typed)?.then((why) => { if (ask === st.current.asked) setCheck(why || ''); });
  }, CHECK_PAUSE_MS));
  useEffect(() => {
    api.onField?.(c.name, true);
    input.current?.focus();
    input.current?.select();
    if (num || c.unit === 'size') askLater.current(initial);
    return () => { askLater.current.cancel(); api.onField?.(c.name, false); };
  }, []);
  const finish = (keep) => {
    const x = st.current;
    if (x.done) return;
    x.done = true;
    if (x.scrub) {
      // the source holds what was last written; it ends as one step of history (nothing typed takes the control out), or is put back
      if (keep) x.scrub.end(x.val.trim() ? parseValue(x.val) : null); else x.scrub.cancel();
    } else if (keep && x.val !== initial) {
      const typed = text ? x.val.trim() : null;
      if (text && !typed) put(g, c.name, null);
      else put(g, c.name, text ? typed : parseValue(x.val));
    }
    onDone();
  };
  const take = (k = pick) => {
    const it = items[k];
    if (!it) return false;
    const next = val.slice(0, range.from) + it.insert + val.slice(range.to);
    set(next, range.from + it.insert.length);
    queueMicrotask(() => { input.current?.setSelectionRange(range.from + it.insert.length, range.from + it.insert.length); });
    write?.(next);
    return true;
  };
  const onKeyDown = (/** @type {any} */ e) => {
    if (items.length) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { setPick((pick + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length); e.preventDefault(); return; }
      if (e.key === 'Tab' || (e.key === 'Enter' && range.to > range.from)) { if (take()) { e.preventDefault(); return; } }
    }
    if (e.key === 'Enter') finish(true);
    if (e.key === 'Escape') { e.stopPropagation(); finish(false); }
  };
  const onInput = (/** @type {any} */ e) => {
    const next = e.target.value;
    st.current.typed = next;
    st.current.pointed = null;
    set(next, e.target.selectionStart ?? next.length);
    askLater.current(next);
  };
  // the helper or the size grid hang from the field, in a box of the page (the sheet would clip it)
  useEffect(() => {
    if (!mount.current) { mount.current = document.createElement('div'); document.body.append(mount.current); }
    if (c.unit === 'size') {
      const pointed = st.current.pointed;
      render(html`<${SizeGridBox} pick=${pointed ?? parseSize(val)} full=${false} input=${input.current}
        onPoint=${(p) => { st.current.pointed = p; const f = p ? `"${p[0]}x${p[1]}"` : st.current.typed; st.current.val = f; setVal(f); }} onTake=${(p) => { st.current.val = `"${p[0]}x${p[1]}"`; finish(true); }} />`, mount.current);
    } else if (num) {
      render(html`<${ValuePopBox} c=${c} dflt=${inheritedPhrase(inh)} input=${input.current} typed=${val} items=${items} index=${pick} check=${check}
        onForm=${(f) => { const next = f === 'number' ? asNumber(val) : asRange(val); set(next); write?.(next); askLater.current(next); input.current?.focus(); }} onPick=${(k) => take(k)} />`, mount.current);
    }
  });
  useEffect(() => () => { if (mount.current) { render(null, mount.current); mount.current.remove(); mount.current = null; } }, []);
  return html`<input class="edit" ref=${input} value=${val} spellcheck=${false} onInput=${onInput} onClick=${(e) => setCaret(e.target.selectionStart ?? val.length)} onKeyDown=${onKeyDown} onBlur=${() => finish(true)} />`;
}

// ---- the printed size -----------------------------------------------------------------------------------------

/**
 * What a length prints on the build target, beside the value as written (`7 dmm · prints 5.08 mm`).  The program works it
 * out (`/api/tiles/ID/printed`); this only shows it.  Where the value is a number or a range of numbers, a printed number
 * is typed in place: the program writes it back in the value's own unit (`/api/printed-set`), and the result is written like
 * any typed value, one step of history.  A formula only shows its printed size.
 * @param {{ g: any, c: any, s: any, v: any, ro: boolean, body: string, printed: {ends: string[], editable: boolean, stale?: boolean}, api: any, put: Function }} p
 */
function PrintedSize({ g, c, s, v, ro, body, printed, api, put }) {
  const [at, setAt] = useState(/** @type {number | null} */ (null));
  const [typed, setTyped] = useState('');
  const [bad, setBad] = useState(false);
  const input = useRef(/** @type {HTMLInputElement | null} */ (null));
  const done = useRef(false);
  useEffect(() => { if (at !== null) { done.current = false; input.current?.focus(); input.current?.select(); } }, [at]);
  const finish = async (/** @type {boolean} */ keep) => {
    if (done.current || at === null) return;
    const end = at, text = typed.trim();
    if (!keep || text === printed.ends[end]) { done.current = true; setAt(null); return; }
    if (!/^-?\d+(\.\d+)?$/.test(text)) { setBad(true); return; } // a printed size is a number of mm: the field stays, marked
    done.current = true;
    setAt(null);
    const next = await api.setPrinted?.({ typed: body, end, mm: Number(text) });
    if (next == null) return; // the program's refusal was shown, loudly (editor/sheet.js)
    put(g, c.name, parseValue(next));
  };
  const can = printed.editable && !ro && !!api.setPrinted;
  const ends = printed.ends.map((e, i) => (at === i
    ? html`<input class=${`edit pedit${bad ? ' bad' : ''}`} key=${i} ref=${input} value=${typed} size=${Math.max(4, typed.length + 1)} spellcheck=${false} aria-label=${`Printed size, mm${printed.ends.length > 1 ? (i ? ', high end' : ', low end') : ''}`}
        title=${bad ? 'A printed size is a number of mm' : 'Enter writes it in the value’s own unit; Escape leaves it'}
        onInput=${(/** @type {any} */ ev) => { setTyped(ev.target.value); setBad(false); }}
        onKeyDown=${(/** @type {any} */ ev) => { if (ev.key === 'Enter') finish(true); if (ev.key === 'Escape') { ev.stopPropagation(); finish(false); } }}
        onBlur=${() => finish(true)} />`
    : can
      ? html`<button type="button" class="pnum" key=${i} title="Type a printed size: it is written in the value’s own unit" onClick=${() => { setTyped(e); setBad(false); setAt(i); }}>${e}</button>`
      : html`<span key=${i}>${e}</span>`));
  return html`<span class=${`printed${printed.stale ? ' stale' : ''}`} title="What this prints on the build target (the system and printer in the title bar)">· prints ${ends[0]}${ends[1] ? html`–${ends[1]}` : null} mm</span>`;
}

// ---- a value shown --------------------------------------------------------------------------------------------

/** A value: its numbers scrubbed by a drag, the whole of it typed after a click. */
function ValueCell({ g, c, s, v, inh, ro, api, put, roll, printed }) {
  const [typing, setTyping] = useState(false);
  const [live, setLive] = useState(/** @type {Record<number, string> | null} */ (null)); // the numbers as the drag has them now
  const drag = useRef(/** @type {any} */ (null));
  const moved = useRef(false);
  const shownRaw = v ? v.text : valueIn(inh);
  // a quoted value shows without its quotes ("20%" reads as 20%, like 100%); they stay in the text, hidden, so a scrub rewrites them as written
  const pre = c.unit === 'text' || c.unit === 'raw' ? '' : (shownRaw.match(/^#?"/) || [''])[0];
  const post = pre ? (shownRaw.slice(pre.length).match(/"#?$/) || [''])[0] : '';
  // a description reads as its words: no quotes, no escapes
  const body = c.unit === 'text' ? unquote(shownRaw) : shownRaw.slice(pre.length, shownRaw.length - post.length);
  const parts = [];
  let last = 0, k = 0;
  for (const m of c.unit === 'text' || c.viaSet ? [] : body.matchAll(NUM)) {
    parts.push({ text: body.slice(last, m.index) }, { num: m[0], k: k++ });
    last = m.index + m[0].length;
  }
  parts.push({ text: body.slice(last) });
  // a range of two numbers reads `3.5–6.5 dmm` (an en dash; the unit once when both ends have it); the source keeps `..`
  const tidy = parts.length === 5 && /^[A-Za-z%]*\s*\.\.\s*$/.test(parts[2].text) && /^[A-Za-z%]*$/.test(parts[4].text);
  // a count of nothing reads "none"
  const none = c.unit === 'count' && parts.length === 3 && !parts[0].text && !parts[2].text;
  const about = api.unitAbout?.() ?? {};
  /** A text after a number: its unit word says what it is, in a tooltip. */
  const afterNumber = (/** @type {string} */ text) => {
    const w = text.match(/^[A-Za-z]+/)?.[0];
    return w && about[w] ? html`<span class="u" title=${about[w]}>${w}</span>${text.slice(w.length)}` : text;
  };
  // the text between the two numbers of a tidy range: the first unit only where the ends differ
  const between = (/** @type {string} */ text) => {
    const first = text.match(/^[A-Za-z%]*/)[0];
    return html`${first && first !== parts[4].text ? afterNumber(first) : ''}–`;
  };
  /** The value as the row shows it now: with the numbers of a drag in place of the written ones. */
  const textNow = (/** @type {Record<number, string> | null} */ l) => `${pre}${parts.map((p) => (p.num != null ? (l?.[p.k] ?? p.num) : p.text)).join('')}${post}`;

  const scrub = (/** @type {any} */ p) => ({
    onPointerDown: (/** @type {any} */ e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      moved.current = false;
      if (!s) { moved.current = true; put(g, c.name, parseValue(valueIn(inh) || '0')); return; } // write what it inherits; drag again
      e.currentTarget.setPointerCapture(e.pointerId);
      api.onField?.(c.name, true);
      const start = Number(p.num);
      const step = STEP[c.unit] ?? 0.1;
      const dec = Math.max(decimals(p.num), decimals(String(step)));
      drag.current = { k: p.k, x: e.clientX, start, step, dec, scrub: undefined, text: null, cur: null };
    },
    onPointerMove: (/** @type {any} */ e) => {
      const d = drag.current;
      if (!d) return;
      const dx = e.clientX - d.x;
      if (Math.abs(dx) > 2) moved.current = true;
      if (!moved.current) return;
      const fine = e.shiftKey ? 0.1 : 1;
      let x = d.start + Math.round(dx / (PX[c.unit] ?? 4)) * d.step * fine;
      if (c.limits && !/\.\./.test(v.text)) x = Math.min(c.limits[1] ?? Infinity, Math.max(c.limits[0], x));
      const txt = String(Number(x.toFixed(fine < 1 ? d.dec + 1 : d.dec))); // 7, not 7.0
      if (txt === (d.cur ?? p.num)) return;
      // each step is the program's: it makes the edit of the value and drafts it (`api.scrub`); the page only says the value
      if (d.scrub === undefined) d.scrub = api.scrub?.(g.at, c.name, `Changed ${c.name}`) ?? null;
      if (!d.scrub) { drag.current = null; api.onField?.(c.name, false); return; } // the source cannot be changed now (said)
      d.cur = txt;
      const l = { [d.k]: txt };
      setLive(l);
      d.text = textNow(l);
      d.scrub.to(parseValue(d.text));
    },
    onPointerUp: () => finishScrub(),
    onPointerCancel: () => finishScrub(),
  });
  const finishScrub = () => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    api.onField?.(c.name, false);
    // ended before the last write, so it is saved
    if (moved.current && d.scrub) d.scrub.end(parseValue(d.text ?? textNow(live)));
    setLive(null);
  };
  const dice = c.name === 'seed' && !ro;
  const unit = c.unit && /\d/.test(shownRaw) && !carriesUnit(shownRaw) && !['number', 'word', 'tag', 'count', 'text', 'raw', 'size', 'height', 'several'].includes(c.unit) ? (c.unit === 'degrees' ? '°' : c.unit === 'percent' ? '' : c.unit) : null;
  return html`${typing
    ? html`<${TypeIn} g=${g} c=${c} s=${s} v=${v} inh=${inh} api=${api} put=${put} onDone=${() => setTyping(false)} />`
    : html`<span class=${`val${ro ? ' ro' : ''}`} onClick=${() => { if (!moved.current && !ro) setTyping(true); }}>
      ${pre ? html`<span class="q">${pre}</span>` : null}${parts.map((p, i) => (p.num != null
        ? html`<span class=${`num${drag.current?.k === p.k ? ' active' : ''}`} key=${i} ...${ro ? {} : scrub(p)}>${none && (live?.[p.k] ?? p.num) === '0' ? 'none' : (live?.[p.k] ?? p.num)}</span>`
        : tidy && i === 2 ? between(p.text) : i > 0 && parts[i - 1].num != null ? afterNumber(p.text) : p.text))}${post ? html`<span class="q">${post}</span>` : null}${!shownRaw ? html`<span class=${inh ? 'absent' : 'unit lost'}>${inh ? inh.text : `no default for ${c.name}`}</span>` : null}</span>`}
    ${dice ? html`<${SeedDice} roll=${roll} onRoll=${() => put(g, c.name, newSeed())} />` : null}
    ${unit ? html`<span class="unit">${unit}</span>` : null}
    ${printed ? html`<${PrintedSize} g=${g} c=${c} s=${s} v=${v} ro=${ro} body=${unquote(shownRaw)} printed=${printed} api=${api} put=${put} />` : null}`;
}

// ---- the entries that keep their own seed ---------------------------------------------------------------------

/**
 * The stack entries that write `seed=` (they keep their own randomness when the tile's seed is rolled), as the stack
 * items the program describes say (`seed`, the entry's own `seed=`): `{key, name, seed}` in written order.  The base and an item without a
 * `seed` are not in it.  @param {any[]} stack
 */
export function pinsOf(stack) {
  const pins = [];
  for (const it of stack) {
    if (it.key != null && it.seed != null) pins.push({ key: it.key, name: it.label || it.what || it.key, seed: String(it.seed) });
  }
  return pins;
}

/**
 * The words under the tile's seed row: the pins as names and the rest, e.g. ["wall", " keeps its own seed (15)"] or
 * ["wall", " and ", "flagstones", " keep their own seeds"].  `pinLine` is the same words as one string.
 * @param {{ key: string, name: string, seed: string }[]} pins
 * @returns {(string | { key: string, name: string, seed: string })[]}
 */
export function pinParts(pins) {
  if (!pins.length) return [];
  if (pins.length === 1) return [pins[0], ` keeps its own seed (${pins[0].seed})`];
  const out = [];
  pins.forEach((p, i) => { if (i) out.push(i === pins.length - 1 ? ' and ' : ', '); out.push(p); });
  out.push(' keep their own seeds');
  return out;
}
export const pinLine = (/** @type {{ key: string, name: string, seed: string }[]} */ pins) => pinParts(pins).map((x) => (typeof x === 'string' ? x : x.name)).join('');

// ---- a row ----------------------------------------------------------------------------------------------------

function Row({ g, c, s, editable, api, put, model, printed }) {
  // the tile's own seed row: its dice shows a strip of candidates under the row (`model.seedStrip`)
  const strip = c.name === 'seed' && model.seedStrip && model.groups[0] === g ? model.seedStrip : null;
  const [seeds, setSeeds] = useState(/** @type {(number | null)[] | null} */ (null));
  // the entries that keep their own seed when this one is rolled (`model.seedPins`, from the stack)
  const pins = strip ? model.seedPins || [] : [];
  const ro = !editable || (s && s.file);
  // a value written in a part the entry calls (`lawn` writes the grass) is inherited too: from that part, and not the entry's to change here
  const via = s?.file ? (s.from ?? s.file) : null;
  const v = s ? valueOf(s.text, c.name) : null;
  // what an unwritten control is: the value a build uses, and where it comes from (shown quietly beside it)
  const inh = s ? null : inheritedOf(g, c);
  if (!s && !inh) console.error(`the sheet has no default for ${g.schema?.name ?? ''} ${c.name}: the program's table needs one`);
  const shown = v ? v.text : valueIn(inh);
  const words = c.words?.length ? c.words : null;
  let cell;
  if (words && c.unit === 'several') {
    const cur = new Set(shown.split(/\s+/).filter(Boolean).map((w) => unquote(w)));
    cell = html`<span class="words">${words.map((w) => {
      const on = cur.has(w.word);
      return html`<button type="button" key=${w.word} class=${on ? 'on' : undefined} title=${w.about || ''} disabled=${ro} aria-pressed=${String(on)}
        onClick=${() => { const next = new Set(cur); if (on) next.delete(w.word); else next.add(w.word); put(g, c.name, words.map((x) => x.word).filter((x) => next.has(x))); }}>${w.word}</button>`;
    })}</span>`;
  } else if (words && !isNumeric(c)) {
    const cur = shown.replace(/^"|"$/g, '');
    if (wordControlNeedsSelect(words)) {
      const known = words.some((w) => w.word === cur);
      cell = html`<select class="word-select" disabled=${ro} value=${known ? cur : ''} title=${words.find((w) => w.word === cur)?.about || c.about || ''} onChange=${(e) => put(g, c.name, e.target.value)}>
        ${known ? null : html`<option value="" disabled>${inh?.text ?? `no default for ${c.name}`}</option>`}${words.map((w) => html`<option value=${w.word} title=${w.about || ''} key=${w.word}>${w.word}</option>`)}</select>`;
    } else {
      cell = html`<span class="words">${words.map((w) => html`<button type="button" key=${w.word} class=${w.word === cur ? (s ? 'on' : 'on inh') : undefined} title=${w.about || ''} disabled=${ro} onClick=${() => put(g, c.name, w.word)}>${w.word}</button>`)}</span>`;
    }
  } else if (c.unit === 'switch') {
    const on = /#?true/.test(shown);
    cell = html`<button type="button" class=${`flip${on ? ' on' : ''}`} disabled=${ro} onClick=${() => put(g, c.name, !on)}>${on ? 'on' : 'off'}</button>`;
  } else {
    cell = html`<${ValueCell} g=${g} c=${c} s=${s} v=${v} inh=${inh} ro=${ro} api=${api} put=${put} roll=${strip ? () => setSeeds(seeds ? null : candidates(strip.current)) : null} printed=${printed} />`;
  }
  const mark = s ? via : inh?.from;
  return html`<div class=${`row${!s || via ? ' faint' : ''}`} onMouseEnter=${() => api.onFieldHover?.(c.name, true)} onMouseLeave=${() => api.onFieldHover?.(c.name, false)}>
    <span class="nm" title=${[c.about, c.unit && `in ${c.unit}`, c.takes].filter(Boolean).join(' · ')}>${c.label ?? c.name}</span>
    <span class="cell">${cell}${mark ? html`<span class="from" title=${s ? `Written in ${mark}, not in this tile` : `Not written: a build uses this. Change it to write it${mark === 'default' ? '' : `; it comes from ${mark}`}.`}>${fromText(mark)}</span>` : null}</span>
    ${s && !ro && !c.required ? html`<button type="button" class="x" title=${`Take ${c.name} out (back to its default)`} onClick=${() => put(g, c.name, null)}><${Icon} name="x" size=${14} /></button>` : null}
    ${api.detail?.(c) ? html`<button type="button" class="chev" title=${`More about ${c.name}`} aria-label=${`More about ${c.name}`} onClick=${() => api.openControl?.(Math.max(0, model.groups.indexOf(g)), c.name)}><${Icon} name="chevron-right" size=${14} /></button>` : null}
  </div>${pins.length ? html`<p class="note pins">${pinParts(pins).map((x) => (typeof x === 'string' ? x
      : html`<button type="button" class="linkish" key=${x.key} title=${`Select ${x.name}`} onClick=${() => model.openEntry?.(x.key)}>${x.name}</button>`))}</p>` : null}${strip && seeds ? html`<${SeedStrip} s=${strip} seeds=${seeds} ro=${ro}
    onPick=${(/** @type {number | null} */ x) => { setSeeds(null); if (x !== strip.current && x != null) put(g, 'seed', x); }}
    onMore=${() => setSeeds(candidates(strip.current))} onClose=${() => setSeeds(null)} />` : null}`;
}

/** A menu that adds something: `add` is {label, items: [{label, title, disabled, run}]}; choosing an item runs it, then the menu is ready again. */
function AddMenu({ add }) {
  return html`<select class="word-select add" aria-label=${add.label} value="" onChange=${(e) => { const it = add.items[Number(e.target.value)]; e.target.value = ''; if (it && !it.disabled) it.run(); }}>
    <option value="" disabled>${`${add.label}…`}</option>${add.items.map((it, i) => html`<option value=${String(i)} title=${it.title || ''} disabled=${!!it.disabled} key=${i}>${it.label}</option>`)}</select>`;
}

/** The lengths of a sheet the program is asked the printed size of: [{key, element, control, typed}], the typed value as written (or the default shown). */
function printedWanted(/** @type {any} */ model) {
  const out = [];
  for (const g of model.groups) {
    if (!g.schema?.name) continue;
    for (const c of g.schema.controls || []) {
      if (c.unit !== 'dmm' && c.unit !== 'height') continue;
      const w = g.written.find((/** @type {any} */ x) => x.name === c.name);
      const typed = unquote(w ? valueOf(w.text, c.name).text : valueIn(inheritedOf(g, c)));
      if (typed) out.push({ key: printedKey(g, c), element: g.schema.name, control: c.name, typed });
    }
  }
  return out;
}
const printedKey = (/** @type {any} */ g, /** @type {any} */ c) => `${g.schema?.name}@${g.at}.${c.name}`;

export function Sheet({ model, api, emptyTitle = 'Nothing selected', emptyText = 'Click something on the tile or put the cursor in its line to inspect and change it.' }) {
  const [more, setMore] = useState(false);
  // the printed size of each length, asked of the program when what is written (or the build target) changes; the last
  // answer stays, dimmed, while the next one is on its way
  const [printed, setPrinted] = useState(/** @type {Record<string, any>} */ ({}));
  const wanted = model && api.printed ? printedWanted(model) : [];
  const asked = `${JSON.stringify(wanted)}|${api.printedKey?.() ?? ''}`;
  useEffect(() => {
    if (!wanted.length) { setPrinted({}); return undefined; }
    let live = true;
    api.printed(wanted.map(({ element, control, typed }) => ({ element, control, typed }))).then((/** @type {any[] | null} */ res) => {
      if (!live || !res) return;
      const next = /** @type {Record<string, any>} */ ({});
      wanted.forEach((w, i) => { next[w.key] = res[i] ? { ...res[i], typed: w.typed } : null; });
      setPrinted(next);
    });
    return () => { live = false; };
  }, [asked]);
  if (!model) return html`<div class="empty"><b>${emptyTitle}</b>${emptyText}</div>`;
  const editable = api.writable() || !!api.canMine?.();
  const put = (g, name, value) => (g.set ? g.set(name, value) : api.set(g.at, name, value));
  let hidden = 0;
  const groups = model.groups.map((g, gi) => {
    const writtenBy = new Map(g.written.map((x) => [x.name, x]));
    const controls = g.schema ? g.schema.controls : g.written.map((x) => ({ name: x.name }));
    const rows = [];
    for (const c of controls) {
      const s = writtenBy.get(c.name);
      if (!s && (c.expert || c.taken === false) && !more) { hidden++; continue; }
      const p = printed[printedKey(g, c)];
      const typedNow = unquote(s ? valueOf(s.text, c.name).text : valueIn(inheritedOf(g, c)));
      rows.push(html`<${Row} key=${`${gi}.${c.name}`} g=${g} c=${c} s=${s} editable=${editable} api=${api} put=${put} model=${model} printed=${p ? { ...p, stale: p.typed !== typedNow } : null} />`);
    }
    // written controls the schema does not know (a definition's arguments)
    if (g.schema) for (const x of g.written) if (!g.schema.controls.some((c) => c.name === x.name)) rows.push(html`<${Row} key=${`${gi}.${x.name}.w`} g=${g} c=${{ name: x.name }} s=${x} editable=${editable} api=${api} put=${put} model=${model} />`);
    return html`${g.heading ? html`<div class="grp"><span>${g.heading}</span>${g.remove && editable ? html`<button type="button" class="x" title=${g.remove.title} aria-label=${g.remove.title} onClick=${g.remove.run}><${Icon} name="x" size=${14} /></button>` : null}</div>` : null}${rows}`;
  });
  return html`<div class="body">
      ${model.note ? html`<div class="note">${model.note}</div>` : null}
      ${groups}
      ${model.add && editable ? html`<${AddMenu} add=${model.add} />` : null}
      ${model.tail ?? null}
      ${hidden || more ? html`<button type="button" class="linkish more" onClick=${() => setMore(!more)}>${more ? 'fewer controls' : `${hidden} more control${hidden === 1 ? '' : 's'}`}</button>` : null}
    </div>
    ${model.actions?.length ? html`<div class="entry-actions">${model.actions.map((a) => html`<button type="button" key=${a.label} class=${`btn${a.danger ? ' danger' : ''}`} title=${a.title} onClick=${a.run}>${a.label}</button>`)}</div>` : null}`;
}
