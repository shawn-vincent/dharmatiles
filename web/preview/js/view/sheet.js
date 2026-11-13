// The property sheet: the controls of the selected thing (a stack entry, a line, a region, the tile itself) fill the
// panel.  Every number in a value is scrubbed by dragging it sideways (Shift for fine steps) or typed after a click; a
// word control is a row of its words; a switch flips.  Each change is a change to the document's text, made through
// `api`:
//
//   api.writable()                      may this file be changed?
//   api.replace(start, len, text, end)  replace bytes [start, start+len) of the document; `end` when a drag ends
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
import { asNumber, asRange, debounce, isNumeric, kdlString, unquote, valueItems, written } from '../value.js';
import { html, render, useEffect, useRef, useState } from './html.js';
import { SizeGridBox, parseSize } from './size-grid.js';
import { ValuePopBox } from './value-pop.js';
import { Thumb } from '../thumbs.js';

const STEP = { mm: 0.1, level: 0.1, degrees: 1, percent: 1, count: 1, 'per square': 0.05, number: 0.05 };
const PX = { count: 10, degrees: 3, percent: 3 }; // pixels a step takes; 4 otherwise
const NUM = /-?\d+(?:\.\d+)?/g;
const CHECK_PAUSE_MS = 200; // how long typing pauses before the program is asked about the value
const enc = new TextEncoder();
const bytes = (s) => enc.encode(s).length;
const decimals = (s) => (s.includes('.') ? s.split('.')[1].length : 0);
const dflt = (c) => c.default1 ?? c.default;

// a die showing five
const DICE = html`<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="1.5" width="13" height="13" rx="3" fill="none" stroke="currentColor" stroke-width="1.4" />${[[5, 5], [11, 5], [8, 8], [5, 11], [11, 11]].map(([x, y]) => html`<circle cx=${x} cy=${y} r="1.25" fill="currentColor" />`)}</svg>`;

// ---- a value typed in place ------------------------------------------------------------------------------------

/** A value being typed: the field, the helper under it (or the size grid), and the writes it makes as it goes. */
function TypeIn({ g, c, s, v, api, put, onDone }) {
  const text = c.unit === 'text'; // plain words: typed without quotes, written as a KDL string
  // a number, range or expression: typed bare, quoted as it is written
  const num = isNumeric(c);
  const shown = v ? v.text : (dflt(c) ?? '');
  const initial = text || num ? unquote(shown) : shown;
  const [val, setVal] = useState(initial);
  const [caret, setCaret] = useState(initial.length);
  const [pick, setPick] = useState(0);
  const [check, setCheck] = useState('');
  const input = useRef(/** @type {HTMLInputElement | null} */ (null));
  const mount = useRef(/** @type {HTMLElement | null} */ (null));
  const st = useRef({ val: initial, done: false, asked: 0, typed: initial, pointed: null,
    // the field's changes land in the source as they are made (as a drag's do): the span the value has now
    live: { start: s && v ? s.span[0] + bytes(s.text.slice(0, v.at)) : 0, len: v ? bytes(v.text) : 0, wrote: false } });
  const items = c.unit === 'size' ? [] : num ? valueItems(val, caret, c).items : [];
  const range = num ? valueItems(val, caret, c) : { items: [], from: 0, to: 0 };
  const write = num && s && v && !c.viaSet && !text ? (typed) => {
    const out = written(typed);
    api.replace(st.current.live.start, st.current.live.len, out, false);
    st.current.live.len = bytes(out);
    st.current.live.wrote = true;
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
    if (x.live.wrote) {
      // the source holds what was last written; it ends as one step of history, or is put back
      api.replace(x.live.start, x.live.len, keep ? written(x.val) : v.text, true);
    } else if (keep && x.val !== initial) {
      const typed = text ? x.val.trim() : null;
      if (text && !typed) put(g, c.name, null);
      else if (s && !c.viaSet) api.replace(s.span[0] + bytes(s.text.slice(0, v.at)), bytes(v.text), text ? kdlString(typed) : num ? written(x.val) : x.val.trim(), true);
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
      render(html`<${ValuePopBox} c=${c} dflt=${dflt(c)} input=${input.current} typed=${val} items=${items} index=${pick} check=${check}
        onForm=${(f) => { const next = f === 'number' ? asNumber(val) : asRange(val); set(next); write?.(next); askLater.current(next); input.current?.focus(); }} onPick=${(k) => take(k)} />`, mount.current);
    }
  });
  useEffect(() => () => { if (mount.current) { render(null, mount.current); mount.current.remove(); mount.current = null; } }, []);
  return html`<input class="edit" ref=${input} value=${val} spellcheck=${false} onInput=${onInput} onClick=${(e) => setCaret(e.target.selectionStart ?? val.length)} onKeyDown=${onKeyDown} onBlur=${() => finish(true)} />`;
}

// ---- a value shown --------------------------------------------------------------------------------------------

/** A value: its numbers scrubbed by a drag, the whole of it typed after a click. */
function ValueCell({ g, c, s, v, ro, api, put, roll }) {
  const [typing, setTyping] = useState(false);
  const [live, setLive] = useState(/** @type {Record<number, string> | null} */ (null)); // the numbers as the drag has them now
  const drag = useRef(/** @type {any} */ (null));
  const moved = useRef(false);
  const shownRaw = v ? v.text : (dflt(c) ?? '');
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
  /** The value as the row shows it now: with the numbers of a drag in place of the written ones. */
  const textNow = (/** @type {Record<number, string> | null} */ l) => `${pre}${parts.map((p) => (p.num != null ? (l?.[p.k] ?? p.num) : p.text)).join('')}${post}`;

  const scrub = (/** @type {any} */ p) => ({
    onPointerDown: (/** @type {any} */ e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      moved.current = false;
      if (!s) { moved.current = true; put(g, c.name, parseValue(dflt(c) ?? '0')); return; } // write the default; drag again
      e.currentTarget.setPointerCapture(e.pointerId);
      api.onField?.(c.name, true);
      const start = Number(p.num);
      const step = STEP[c.unit] ?? 0.1;
      const dec = Math.max(decimals(p.num), decimals(String(step)));
      drag.current = { k: p.k, x: e.clientX, start, step, dec, len: bytes(v.text), valueStart: s.span[0] + bytes(s.text.slice(0, v.at)), cur: null };
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
      d.cur = txt;
      const l = { [d.k]: txt };
      setLive(l);
      const out = textNow(l);
      api.replace(d.valueStart, d.len, out, false);
      d.len = bytes(out);
      d.text = out;
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
    if (moved.current) api.replace(d.valueStart, d.len, d.text ?? textNow(live), true);
    setLive(null);
  };
  const dice = c.name === 'seed' && !ro;
  const unit = c.unit && !carriesUnit(shownRaw) && !['number', 'word', 'tag', 'count', 'text', 'raw', 'size', 'height', 'several'].includes(c.unit) ? (c.unit === 'degrees' ? '°' : c.unit === 'percent' ? '' : c.unit) : null;
  return html`${typing
    ? html`<${TypeIn} g=${g} c=${c} s=${s} v=${v} api=${api} put=${put} onDone=${() => setTyping(false)} />`
    : html`<span class=${`val${ro ? ' ro' : ''}`} onClick=${() => { if (!moved.current && !ro) setTyping(true); }}>
      ${pre ? html`<span class="q">${pre}</span>` : null}${parts.map((p, i) => (p.num != null
        ? html`<span class=${`num${drag.current?.k === p.k ? ' active' : ''}`} key=${i} ...${ro ? {} : scrub(p)}>${live?.[p.k] ?? p.num}</span>`
        : p.text))}${post ? html`<span class="q">${post}</span>` : null}${!shownRaw ? html`<span class="unit">not set</span>` : null}</span>`}
    ${dice ? html`<button type="button" class="dice" title=${roll ? 'Show five arrangements to choose from' : 'Roll a new seed: another arrangement'} aria-label=${roll ? 'Show five seeds' : 'Roll a new seed'} onClick=${roll ?? (() => put(g, c.name, newSeed()))}>${DICE}</button>` : null}
    ${unit ? html`<span class="unit">${unit}</span>` : null}`;
}

// ---- the seed strip -------------------------------------------------------------------------------------------

const newSeed = () => 1 + Math.floor(Math.random() * 9999);

/** Five seeds to look at: the one the tile has now first (`null`: its own default), then four new ones. @param {number | null} current */
const candidates = (current) => {
  const seeds = [current];
  while (seeds.length < 5) { const x = newSeed(); if (!seeds.includes(x)) seeds.push(x); }
  return seeds;
};

/**
 * What the dice shows (docs/preview.html, "The seed"): five small pictures of the tile, each built low at its seed by the
 * program on demand (js/thumbs.js), the current one first.  A square marks a seed you own.  Choosing one writes the
 * seed, one step of history; choosing the current one writes nothing.  `s` is the model's `seedStrip`.
 */
export function SeedStrip({ s, seeds, ro, onPick, onMore, onClose }) {
  return html`<div class="seed-strip" role="group" aria-label="Seeds to choose from">${seeds.map((seed) => {
    const now = seed === s.current;
    return html`<button type="button" key=${seed ?? 'own'} class=${`seed-card${now ? ' on' : ''}`} disabled=${ro && !now} aria-pressed=${String(now)}
      title=${now ? 'The seed the tile has now' : `Use seed ${seed}`} onClick=${() => onPick(seed)}>
      <${Thumb} svc=${s.svc} spec=${{ ...s.spec, seed: seed ?? undefined }} onError=${s.onError} />
      <b>${seed ?? 'default'}</b>${now ? html`<span class="now">now</span>` : null}${seed != null && s.owned.has(seed) ? html`<i class="own" title="You have printed this one"></i>` : null}</button>`;
  })}</div>
  <div class="seed-strip-acts"><button type="button" class="btn" onClick=${onMore}>Roll again</button><button type="button" class="btn" onClick=${onClose}>Close</button></div>`;
}

// ---- a row ----------------------------------------------------------------------------------------------------

function Row({ g, c, s, editable, api, put, model }) {
  // the tile's own seed row: its dice shows a strip of candidates under the row (`model.seedStrip`)
  const strip = c.name === 'seed' && model.seedStrip && model.groups[0] === g ? model.seedStrip : null;
  const [seeds, setSeeds] = useState(/** @type {(number | null)[] | null} */ (null));
  const ro = !editable || (s && s.file);
  const v = s ? valueOf(s.text, c.name) : null;
  const shown = v ? v.text : (dflt(c) ?? '');
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
        ${known ? null : html`<option value="" disabled>not set</option>`}${words.map((w) => html`<option value=${w.word} title=${w.about || ''} key=${w.word}>${w.word}</option>`)}</select>`;
    } else {
      cell = html`<span class="words">${words.map((w) => html`<button type="button" key=${w.word} class=${w.word === cur ? 'on' : undefined} title=${w.about || ''} disabled=${ro} onClick=${() => put(g, c.name, w.word)}>${w.word}</button>`)}</span>`;
    }
  } else if (c.unit === 'switch') {
    const on = /#?true/.test(shown);
    cell = html`<button type="button" class=${`flip${on ? ' on' : ''}`} disabled=${ro} onClick=${() => put(g, c.name, !on)}>${on ? 'on' : 'off'}</button>`;
  } else {
    cell = html`<${ValueCell} g=${g} c=${c} s=${s} v=${v} ro=${ro} api=${api} put=${put} roll=${strip ? () => setSeeds(seeds ? null : candidates(strip.current)) : null} />`;
  }
  return html`<div class=${`row${s ? '' : ' faint'}`} onMouseEnter=${() => api.onFieldHover?.(c.name, true)} onMouseLeave=${() => api.onFieldHover?.(c.name, false)}>
    <span class="nm" title=${[c.about, c.unit && `in ${c.unit}`, c.takes].filter(Boolean).join(' · ')}>${c.name}</span>
    <span class="cell">${cell}</span>
    ${s && !ro && !c.required ? html`<button type="button" class="x" title=${`Take ${c.name} out (back to its default)`} onClick=${() => put(g, c.name, null)}>×</button>` : null}
    ${api.detail?.(c) ? html`<button type="button" class="chev" title=${`More about ${c.name}`} aria-label=${`More about ${c.name}`} onClick=${() => api.openControl?.(Math.max(0, model.groups.indexOf(g)), c.name)}>›</button>` : null}
  </div>${strip && seeds ? html`<${SeedStrip} s=${strip} seeds=${seeds} ro=${ro}
    onPick=${(/** @type {number | null} */ x) => { setSeeds(null); if (x !== strip.current && x != null) put(g, 'seed', x); }}
    onMore=${() => setSeeds(candidates(strip.current))} onClose=${() => setSeeds(null)} />` : null}`;
}

/** A menu that adds something: `add` is {label, items: [{label, title, disabled, run}]}; choosing an item runs it, then the menu is ready again. */
function AddMenu({ add }) {
  return html`<select class="word-select add" aria-label=${add.label} value="" onChange=${(e) => { const it = add.items[Number(e.target.value)]; e.target.value = ''; if (it && !it.disabled) it.run(); }}>
    <option value="" disabled>${`${add.label}…`}</option>${add.items.map((it, i) => html`<option value=${String(i)} title=${it.title || ''} disabled=${!!it.disabled} key=${i}>${it.label}</option>`)}</select>`;
}

export function Sheet({ model, api, emptyTitle = 'Nothing selected', emptyText = 'Click something on the tile or put the cursor in its line to inspect and change it.' }) {
  const [more, setMore] = useState(false);
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
      rows.push(html`<${Row} key=${`${gi}.${c.name}`} g=${g} c=${c} s=${s} editable=${editable} api=${api} put=${put} model=${model} />`);
    }
    // written controls the schema does not know (a definition's arguments)
    if (g.schema) for (const x of g.written) if (!g.schema.controls.some((c) => c.name === x.name)) rows.push(html`<${Row} key=${`${gi}.${x.name}.w`} g=${g} c=${{ name: x.name }} s=${x} editable=${editable} api=${api} put=${put} model=${model} />`);
    return html`${g.heading ? html`<div class="grp"><span>${g.heading}</span>${g.remove && editable ? html`<button type="button" class="x" title=${g.remove.title} aria-label=${g.remove.title} onClick=${g.remove.run}>×</button>` : null}</div>` : null}${rows}`;
  });
  return html`<div class=${model.slim ? 'lt slim' : 'lt'}><div class="heading"><span class="cap">${model.kind || 'Properties'}</span>${model.slim ? null : html`<b>${model.title}</b>`}</div>
      ${model.href ? html`<a href=${model.href} target="_blank" rel="noopener" title=${`Help for ${model.title}`}>Help ↗</a>` : null}</div>
    <div class="body">
      ${model.note ? html`<div class="note">${model.note}</div>` : null}
      ${groups}
      ${model.add && editable ? html`<${AddMenu} add=${model.add} />` : null}
      ${model.tail ?? null}
      ${hidden || more ? html`<button type="button" class="linkish more" onClick=${() => setMore(!more)}>${more ? 'fewer controls' : `${hidden} more control${hidden === 1 ? '' : 's'}`}</button>` : null}
    </div>
    ${model.actions?.length ? html`<div class="entry-actions">${model.actions.map((a) => html`<button type="button" key=${a.label} class=${`btn${a.danger ? ' danger' : ''}`} title=${a.title} onClick=${a.run}>${a.label}</button>`)}</div>` : null}`;
}
