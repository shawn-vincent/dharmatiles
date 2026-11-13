// The property level: one control of a thing, with room.  A control gets a level of its own when it has more to say than a
// row can: a range and the odds along it, a height worked out where it stands.  The page is the control's name, what it
// takes, its value in a field wide enough to read (checked by the program as you type), the forms it can be written in,
// and what it is as the source writes it.  Nothing here decides a value: the program checks it (`api.checkValue`), the
// document takes it (`api.replace` over the written bytes, `api.set` when nothing is written yet).
//
//   api.writable(), api.canMine(), api.replace(start, len, text, end), api.set(at, name, value),
//   api.checkValue(element, control, typed) -> Promise<string|null>, api.showSource()
//
// `PropertyPage({model, api})` is a function of `model`: `{element, label, c (the program's row for the control), s (what
// is written: {name, text, span} or undefined), at (the byte the node's name starts at), weightings, names (the `$` names)}`.
// The value being typed is the field's own (a draft); a new state of the document is drawn over the page as it is, so the
// typing, the caret and the focus stay where they are.

import { parseValue, valueOf } from '../lens.js';
import { asNumber, asRange, debounce, formOf, unquote, written } from '../value.js';
import { html, useEffect, useRef, useState } from './html.js';
import { inheritedPhrase } from './sheet.js';

const CHECK_PAUSE_MS = 200;
const enc = new TextEncoder();
const bytes = (s) => enc.encode(s).length;

/** The range inside a weighting call (`low(3dmm .. 6dmm)` is `3dmm .. 6dmm`), or the text as it is. */
export function innerRange(typed, names) {
  const t = typed.trim();
  const m = t.match(/^([A-Za-z_]\w*)\(([\s\S]*)\)$/);
  if (!m || !names.includes(m[1])) return { name: null, range: t };
  // the first argument, up to a comma outside any brackets
  let depth = 0, end = m[2].length;
  for (let i = 0; i < m[2].length; i++) {
    const c = m[2][i];
    if (c === '(' || c === '[') depth++;
    else if (c === ')' || c === ']') depth--;
    else if (c === ',' && depth === 0) { end = i; break; }
  }
  return { name: m[1], range: m[2].slice(0, end).trim() };
}

/** `range` weighted by `name` with its defaults (flat is the range alone). */
export const weightedAs = (range, name) => (name === 'flat' ? range : `${name}(${range})`);

/** The weightings that can be written with nothing but a range: every argument has a default. */
export const simpleWeightings = (list) => (list || []).filter((w) => !w.rest && (w.args || []).every((a) => a.default != null) && /^[a-z]+\(range/.test(w.call));

export function PropertyPage({ model, api }) {
  const { c, s } = model;
  const ro = !(api.writable() || api.canMine?.());
  const v = s ? valueOf(s.text, c.name) : null;
  // what it is where nothing is written: the program's (`model.inherited`), said as a phrase
  const dflt = inheritedPhrase(model.inherited ?? null);
  const shown = v ? v.text : '';
  const written0 = unquote(shown);
  const [draft, setDraft] = useState(/** @type {string | null} */ (null)); // typed, not yet committed
  const [check, setCheck] = useState('');
  const input = useRef(/** @type {HTMLInputElement | null} */ (null));
  const asked = useRef(0);
  const askLater = useRef(debounce((typed) => {
    const ask = ++asked.current;
    api.checkValue?.(model.element, c.name, typed)?.then((why) => { if (ask === asked.current) setCheck(why || ''); });
  }, CHECK_PAUSE_MS));
  useEffect(() => () => askLater.current.cancel(), []);
  const value = draft ?? written0;
  const weights = model.weightings || [];
  const weightNames = weights.map((w) => w.name);

  const startByte = () => s.span[0] + bytes(s.text.slice(0, v.at));
  const commit = (text) => {
    const next = text.trim();
    setDraft(null);
    if (next === written0.trim()) return;
    if (!next) { if (s) api.set(model.at, c.name, null); return; }
    if (s && v) api.replace(startByte(), bytes(v.text), written(next), true);
    else api.set(model.at, c.name, parseValue(next));
  };
  const type = (/** @type {string} */ next) => { setDraft(next); askLater.current(next); };

  const { range: r, name } = innerRange(value, weightNames);
  const form = formOf(r);
  const simple = simpleWeightings(weights);
  const takes = [['unit', c.unit], ['takes', c.takes], ['inherited', dflt || null], ['limits', c.limits ? `${c.limits[0]} to ${c.limits[1] ?? '…'}` : null]].filter(([, x]) => x);
  return html`
    <div class="body">
      ${c.about ? html`<p class="pabout">${c.about}</p>` : null}
      <div class="cap plab">Value</div>
      <div class="pfield"><input class="pin" ref=${input} type="text" spellcheck=${false} autocomplete="off" aria-label=${`${c.name}, as written`} value=${value} placeholder=${dflt || `no default for ${c.name}`} disabled=${ro}
        onInput=${(e) => type(e.target.value)}
        onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); commit(value); } if (e.key === 'Escape') { e.stopPropagation(); setDraft(null); input.current?.blur(); } }}
        onBlur=${() => { if (draft !== null) commit(draft); }} />
        <div class="pcheck" hidden=${!check}>${check}</div></div>
      ${c.ranges ? html`<div class="seg pforms"><button type="button" class=${formOf(value) === 'number' ? 'on' : undefined} disabled=${ro} onClick=${() => commit(asNumber(innerRange(value, weightNames).range))}>One number</button><button type="button" class=${form === 'range' && !name ? 'on' : undefined} disabled=${ro} onClick=${() => commit(asRange(innerRange(value, weightNames).range))}>A range</button></div>` : null}
      ${c.weighted && weights.length ? html`<div class="cap plab">How likely each value is</div>
        <div class="words pweights">${simple.map((w) => html`<button type="button" key=${w.name} class=${name ? (w.name === name ? 'on' : undefined) : (w.name === 'flat' && form === 'range' ? 'on' : undefined)} title=${`${w.call}\n${w.about}`} disabled=${ro} data-weight=${w.name}
          onClick=${() => commit(weightedAs(formOf(innerRange(value, weightNames).range) === 'number' ? asRange(innerRange(value, weightNames).range) : innerRange(value, weightNames).range, w.name))}>${w.name}</button>`)}</div>
        <details class="pmore"><summary>Every form</summary><dl class="pforms-help">${weights.map((w) => html`<dt key=${`t${w.name}`}>${w.call}</dt><dd key=${`d${w.name}`}>${w.about}</dd>`)}</dl></details>` : null}
      ${c.unit === 'height' && model.names?.heights?.length ? html`<div class="cap plab">Names you can use</div>
        <div class="words pnames">${model.names.heights.map((n) => html`<button type="button" key=${n} disabled=${ro} onClick=${() => {
          const el = input.current;
          const at = el?.selectionStart ?? value.length, to = el?.selectionEnd ?? at;
          type(value.slice(0, at) + n + value.slice(to));
          queueMicrotask(() => { el?.focus(); el?.setSelectionRange(at + n.length, at + n.length); });
        }}>${n}</button>`)}</div>` : null}
      ${takes.length ? html`<div class="cap plab">What it takes</div><dl class="ptakes">${takes.map(([k, x]) => html`<dt key=${`t${k}`}>${k}</dt><dd key=${`d${k}`}>${x}</dd>`)}</dl>` : null}
      <div class="cap plab">As written</div>
      <div class="pwritten"><code>${s ? s.text : `${c.name} is not written: the default stands`}</code></div>
    </div>
    <div class="entry-actions"><button type="button" class="btn" onClick=${() => api.showSource?.()}>Show in source</button>
      ${s && !ro && !c.required ? html`<button type="button" class="btn" title=${`Take ${c.name} out`} onClick=${() => api.set(model.at, c.name, null)}>Back to the default</button>` : null}</div>`;
}
