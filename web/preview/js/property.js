// The property level: one control of a thing, with room.  A control gets a level
// of its own when it has more to say than a row can: a range and the odds along it,
// a height worked out where it stands.  The page is the control's name, what it takes,
// its value in a field wide enough to read (checked by the program as you type), the
// forms it can be written in, and what it is as the source writes it.  Nothing here
// decides a value: the program checks it (`api.checkValue`), the document takes it
// (`api.replace` over the written bytes, `api.set` when nothing is written yet).
//
//   api.writable()            may this file be changed?
//   api.canMine()             a library tile: the first change copies it
//   api.replace(start, len, text, end)   as the sheet's own
//   api.set(at, name, value)  as the sheet's own (value null takes the control out)
//   api.checkValue(element, control, typed) -> Promise<string|null>
//   api.showSource()          flip to the source with the caret on this control's line

import { valueOf, parseValue } from './lens.js';
import { asNumber, asRange, debounce, formOf, unquote, written } from './value.js';

const CHECK_PAUSE_MS = 200;
const enc = new TextEncoder();
const bytes = (s) => enc.encode(s).length;

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

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

/**
 * Draw the level into `root`.  `model`: `{element, label, c (the program's row for the control), s
 * (what is written: {name, text, span} or undefined), at (the byte the node's name starts at),
 * weightings, names (the `$` names), set (the group's own writer, or null)}.
 */
export function showProperty(root, model, api) {
  const { c, s } = model;
  root.replaceChildren();
  const ro = !(api.writable() || api.canMine?.());
  const v = s ? valueOf(s.text, c.name) : null;
  const dflt = c.default1 ?? c.default ?? '';
  const shown = v ? v.text : '';
  const body = el('div', 'body');
  const head = el('div', 'lt');
  const heading = el('div', 'heading');
  heading.append(el('span', 'cap', model.label), el('b', '', c.name));
  head.append(heading);
  root.append(head, body);

  if (c.about) body.append(el('p', 'pabout', c.about));

  // the value
  const field = el('div', 'pfield');
  const input = el('input', 'pin');
  input.type = 'text'; input.spellcheck = false; input.autocomplete = 'off';
  input.setAttribute('aria-label', `${c.name}, as written`);
  input.value = unquote(shown);
  input.placeholder = dflt ? `default ${dflt}` : 'not set';
  input.disabled = ro;
  const check = el('div', 'pcheck');
  check.hidden = true;
  field.append(input, check);
  body.append(el('div', 'plab', 'Value'), field);

  const startByte = () => s.span[0] + bytes(s.text.slice(0, v.at));
  let last = input.value;
  const commit = (text) => {
    const next = text.trim();
    delete input.dataset.dirty;
    if (next === last.trim()) return;
    last = next;
    if (!next) { if (s) api.set(model.at, c.name, null); return; }
    if (s && v) api.replace(startByte(), bytes(v.text), written(next), true);
    else api.set(model.at, c.name, parseValue(next));
  };
  let asked = 0;
  const askLater = debounce((typed) => {
    const ask = ++asked;
    api.checkValue?.(model.element, c.name, typed)?.then((why) => {
      if (ask !== asked) return;
      check.textContent = why || '';
      check.hidden = !why;
    });
  }, CHECK_PAUSE_MS);
  input.oninput = () => { input.dataset.dirty = '1'; askLater(input.value); paint(); };
  input.onkeydown = (e) => {
    if (e.key === 'Enter') { e.preventDefault(); commit(input.value); }
    if (e.key === 'Escape') { e.stopPropagation(); input.value = last; delete input.dataset.dirty; paint(); input.blur(); }
  };
  input.onblur = () => commit(input.value);

  const weights = model.weightings || [];
  const weightNames = weights.map((w) => w.name);
  // the forms a number or a range can be written in
  let one = null, range = null;
  if (c.ranges) {
    const forms = el('div', 'seg pforms');
    one = el('button', '', 'One number'); range = el('button', '', 'A range');
    one.type = range.type = 'button';
    one.disabled = range.disabled = ro;
    one.onclick = () => { input.value = asNumber(innerRange(input.value, weightNames).range); commit(input.value); paint(); };
    range.onclick = () => { input.value = asRange(innerRange(input.value, weightNames).range); commit(input.value); paint(); };
    forms.append(one, range);
    body.append(forms);
  }

  // the odds along a range
  const simple = simpleWeightings(weights);
  const chips = [];
  if (c.weighted && weights.length) {
    body.append(el('div', 'plab', 'How likely each value is'));
    const box = el('div', 'words pweights');
    for (const w of simple) {
      const b = el('button', '', w.name);
      b.type = 'button'; b.title = `${w.call}\n${w.about}`; b.disabled = ro;
      b.onclick = () => {
        const { range: r } = innerRange(input.value, weightNames);
        input.value = weightedAs(formOf(r) === 'number' ? asRange(r) : r, w.name);
        commit(input.value); paint();
      };
      b.dataset.weight = w.name;
      chips.push(b); box.append(b);
    }
    body.append(box);
    const list = el('dl', 'pforms-help');
    for (const w of weights) { list.append(el('dt', '', w.call), el('dd', '', w.about)); }
    const more = el('details', 'pmore');
    more.append(el('summary', '', 'Every form'), list);
    body.append(more);
  }

  // a height worked out where it stands: the names it may use
  if (c.unit === 'height' && model.names?.heights?.length) {
    body.append(el('div', 'plab', 'Names you can use'));
    const box = el('div', 'words pnames');
    for (const n of model.names.heights) {
      const b = el('button', '', n);
      b.type = 'button'; b.disabled = ro;
      b.onclick = () => {
        const at = input.selectionStart ?? input.value.length, to = input.selectionEnd ?? at;
        input.value = input.value.slice(0, at) + n + input.value.slice(to);
        input.focus(); input.setSelectionRange(at + n.length, at + n.length);
        askLater(input.value);
      };
      box.append(b);
    }
    body.append(box);
  }

  // what it takes
  const takes = [['unit', c.unit], ['takes', c.takes], ['default', dflt || null], ['limits', c.limits ? `${c.limits[0]} to ${c.limits[1] ?? '…'}` : null]].filter(([, x]) => x);
  if (takes.length) {
    const dl = el('dl', 'ptakes');
    for (const [k, x] of takes) dl.append(el('dt', '', k), el('dd', '', x));
    body.append(el('div', 'plab', 'What it takes'), dl);
  }

  // as written
  body.append(el('div', 'plab', 'As written'));
  const wr = el('div', 'pwritten');
  wr.append(el('code', '', s ? s.text : `${c.name} is not written: the default stands`));
  body.append(wr);

  const actions = el('div', 'entry-actions');
  const src = el('button', 'btn', 'Show in source');
  src.type = 'button'; src.onclick = () => api.showSource?.();
  actions.append(src);
  if (s && !ro && !c.required) {
    const out = el('button', 'btn', 'Back to the default');
    out.type = 'button'; out.title = `Take ${c.name} out`;
    out.onclick = () => api.set(model.at, c.name, null);
    actions.append(out);
  }
  root.append(actions);

  function paint() {
    const { range: r, name } = innerRange(input.value, weightNames);
    const form = formOf(r);
    one?.classList.toggle('on', formOf(input.value) === 'number');
    range?.classList.toggle('on', form === 'range' && !name);
    for (const b of chips) b.classList.toggle('on', name ? b.dataset.weight === name : b.dataset.weight === 'flat' && form === 'range');
  }
  paint();
  return { input };
}
