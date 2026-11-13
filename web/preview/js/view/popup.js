// Small popups for the panel's rows: a menu of actions (a row's ⋯, the Add list) and a popover with one field (a
// rename).  There is at most one open: it is one signal, `current`, and `PopupHost` (mounted once, in the page) draws
// it.  `openMenu` and `openNameField` say what to show and where; they are commands, and what they open is drawn from
// that state, so there is no popup element anyone holds.
//
// Both sit over the page, close on Esc, on a press outside and on Tab, and give the keyboard back to what opened
// them.  A menu is a list the arrow keys walk: Up, Down, Home, End, Enter; an item that cannot be chosen stays in the
// list with its reason under it, so what is missing is never a mystery.

import { signal } from '@preact/signals-core';
import { html, render, useEffect, useLayoutEffect, useRef, useState } from './html.js';

/** @type {import('@preact/signals-core').Signal<any>} */
const current = signal(null);

/** Close the popup that is open, if one is; `restore` gives the keyboard back to what opened it. */
export function closePopup({ restore = true } = {}) {
  const p = current.peek();
  if (!p) return;
  current.value = null;
  p.anchor.removeAttribute('aria-expanded');
  if (restore && document.contains(p.anchor)) p.anchor.focus({ preventScroll: true });
  p.onClose?.();
}

function open(spec) {
  closePopup();
  ensureHost();
  spec.anchor.setAttribute('aria-expanded', 'true');
  current.value = spec;
}

let hostMounted = false;
function ensureHost() {
  if (hostMounted) return;
  hostMounted = true;
  const div = document.createElement('div');
  document.body.append(div);
  render(html`<${PopupHost} />`, div);
}

/** Put `box` by `anchor`: under it (above it when there is no room), `align` its left or right edge with the anchor's, kept on screen. */
export function place(box, anchor, align) {
  const a = anchor.getBoundingClientRect();
  const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
  box.style.maxHeight = '';
  const room = { below: vh - a.bottom - 10, above: a.top - 10 };
  const want = Math.min(box.scrollHeight, 460);
  const up = room.below < want && room.above > room.below;
  const h = Math.min(want, up ? room.above : room.below);
  box.style.maxHeight = `${Math.max(120, h)}px`;
  const w = box.offsetWidth;
  let left = align === 'right' ? a.right - w : a.left;
  left = Math.max(8, Math.min(left, vw - w - 8));
  box.style.left = `${left}px`;
  const top = up ? a.top - Math.min(box.offsetHeight, h) - 4 : a.bottom + 4;
  box.style.top = `${Math.max(8, top)}px`;
}

/**
 * A menu of `items` by `anchor`.  An item is `{label, sub, swatch, hint, danger, disabled, why, run}` (`why` is the
 * reason an item that cannot be chosen is not), `{head}` (a small heading) or `{sep: true}`.  Choosing an item closes the
 * menu and runs it.  `label` names the menu for a screen reader.
 */
export function openMenu(anchor, items, { label = 'Actions', cls = '', align = 'right', onClose } = {}) {
  open({ kind: 'menu', anchor, items, label, cls, align, onClose });
}

/** A popover by `anchor` with one field: `flaw(value)` says why a name will not do; `save(value)` writes it. */
export function openNameField(anchor, { title, value, hint, flaw, save }) {
  open({ kind: 'name', anchor, title, value, hint, flaw, save, align: 'left' });
}

function Menu({ p }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  useLayoutEffect(() => { if (box.current) place(box.current, p.anchor, p.align); }, [p]);
  useEffect(() => {
    // the first choosable item has the keyboard
    const live = [...(box.current?.querySelectorAll('.ritem') ?? [])];
    /** @type {any} */ (live.find((b) => !b.hasAttribute('aria-disabled')) || live[0])?.focus({ preventScroll: true });
  }, [p]);
  const onKeyDown = (/** @type {any} */ e) => {
    const live = [...box.current.querySelectorAll('.ritem')];
    const i = live.indexOf(document.activeElement);
    const go = (/** @type {number} */ k) => /** @type {any} */ (live[(k + live.length) % live.length])?.focus({ preventScroll: false });
    if (e.key === 'ArrowDown') go(i < 0 ? 0 : i + 1);
    else if (e.key === 'ArrowUp') go(i < 0 ? live.length - 1 : i - 1);
    else if (e.key === 'Home') go(0);
    else if (e.key === 'End') go(live.length - 1);
    else return;
    e.preventDefault();
  };
  return html`<div class=${`rmenu ${p.cls}`} role="menu" aria-label=${p.label} ref=${box} onKeyDown=${onKeyDown}>${p.items.map((it, i) => {
    if (it.sep) return html`<hr class="rsep" key=${i} />`;
    if (it.head) return html`<div class="cap rhead" key=${i}>${it.head}</div>`;
    const sub = it.disabled && it.why ? it.why : it.sub;
    return html`<button type="button" key=${i} class=${`ritem${it.danger ? ' danger' : ''}${it.disabled ? ' off' : ''}`} role="menuitem" tabIndex="-1" aria-disabled=${it.disabled ? 'true' : undefined}
      onClick=${() => { if (it.disabled) return; closePopup({ restore: !it.keepFocus }); it.run?.(); }}>
      ${it.swatch ? html`<i class=${`gl fam-${it.swatch}`}></i>` : null}
      <span class="tx"><span class="nm">${it.label}</span>${sub ? html`<span class=${`sub${it.disabled ? ' why' : ''}`} title=${sub}>${sub}</span>` : null}</span>
      ${it.hint ? html`<kbd>${it.hint}</kbd>` : null}</button>`;
  })}</div>`;
}

function NameBox({ p }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  const input = useRef(/** @type {HTMLInputElement | null} */ (null));
  const [value, setValue] = useState(p.value); // what is being typed: this popover's own
  const flaw = p.flaw(value);
  useLayoutEffect(() => { if (box.current) place(box.current, p.anchor, p.align); }, [p]);
  useEffect(() => { input.current?.focus(); input.current?.select(); }, [p]);
  const submit = () => { if (p.flaw(value)) return; closePopup(); p.save(value.trim()); };
  return html`<div class="rmenu rpop namebox" role="dialog" aria-label=${p.title} ref=${box}>
    <label class="rlab">${p.title}</label>
    <input class="pin" ref=${input} value=${value} spellcheck=${false} autocomplete="off" onInput=${(e) => setValue(e.target.value)} onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); submit(); } }} />
    <div class="rerr" role="alert">${flaw || ''}</div>
    <p class="rnote">${p.hint}</p>
    <div class="rbtns"><button type="button" class="btn" onClick=${() => closePopup()}>Cancel</button><button type="button" class="btn primary" disabled=${!!flaw} onClick=${submit}>Save</button></div>
  </div>`;
}

/** Draws the popup that is open, and closes it on Esc, a press outside, Tab out of it and a resize. */
export function PopupHost() {
  const p = current.value;
  const wrap = useRef(/** @type {HTMLElement | null} */ (null));
  useEffect(() => {
    if (!p) return undefined;
    const box = () => wrap.current?.firstElementChild;
    const away = (/** @type {any} */ e) => { if (!box()?.contains(e.target) && !p.anchor.contains(e.target)) closePopup({ restore: false }); };
    const key = (/** @type {any} */ e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closePopup(); } else if (e.key === 'Tab' && !box()?.contains(document.activeElement)) closePopup({ restore: false });
    };
    const resize = () => closePopup();
    document.addEventListener('pointerdown', away, true);
    document.addEventListener('keydown', key, true);
    addEventListener('resize', resize);
    return () => { document.removeEventListener('pointerdown', away, true); document.removeEventListener('keydown', key, true); removeEventListener('resize', resize); };
  }, [p]);
  if (!p) return null;
  return html`<div class="popup-host" ref=${wrap} style="display: contents">${p.kind === 'menu' ? html`<${Menu} p=${p} />` : html`<${NameBox} p=${p} />`}</div>`;
}
