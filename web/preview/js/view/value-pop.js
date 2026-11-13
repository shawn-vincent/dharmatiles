// The helper under a typed value: what the control takes, the program's check of what is typed (asked of the server;
// nothing is checked here), the forms a number can be written in, and what can go at the caret (its words).  Arrows pick,
// Enter or Tab takes.  It is a function of what is typed (`typed`, `caret`) and of the field it hangs from; the field's
// owner (view/sheet.js `TypeIn`) keeps the pick and the check, and draws this into a box of the page.

import { formOf } from '../value.js';
import { html, useLayoutEffect, useRef } from './html.js';

export function ValuePopBox({ c, dflt, input, typed, items, index, check, onForm, onPick }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  const form = formOf(typed);
  // placed by the field: under it (above when there is no room), as wide as it, kept on the window
  useLayoutEffect(() => {
    const place = () => {
      const el = box.current;
      if (!el) return;
      const f = input.getBoundingClientRect();
      el.style.width = `${Math.max(f.width, 240)}px`;
      el.style.left = `${Math.max(4, Math.min(f.left, window.innerWidth - el.offsetWidth - 4))}px`;
      const below = f.bottom + 4 + el.offsetHeight <= window.innerHeight - 4;
      el.style.top = `${below ? f.bottom + 4 : Math.max(4, f.top - 4 - el.offsetHeight)}px`;
    };
    place();
    window.addEventListener('scroll', place, true);
    return () => window.removeEventListener('scroll', place, true);
  });
  // the field keeps the focus: a click in the helper is not a blur
  return html`<div class="value-pop" ref=${box} onPointerDown=${(e) => e.preventDefault()} onMouseDown=${(e) => e.preventDefault()}>
    <div class="vp-takes">${[[c.unit, c.takes].filter(Boolean).join(', '), dflt && `default ${dflt}`].filter(Boolean).join(' · ')}</div>
    <div class="vp-check" hidden=${!check}>${check || ''}</div>
    ${c.ranges ? html`<div class="words vp-forms"><button type="button" class=${form === 'number' ? 'on' : undefined} onClick=${() => onForm('number')}>one number</button><button type="button" class=${form === 'range' ? 'on' : undefined} onClick=${() => onForm('range')}>a range</button></div>` : null}
    <div class="vp-list" role="listbox" hidden=${!items.length}>${items.map((it, k) => html`<div class=${`it${k === index ? ' on' : ''}`} role="option" aria-selected=${String(k === index)} key=${k} onPointerDown=${(e) => { e.preventDefault(); onPick(k); }}><span class="nm">${it.label}</span><span class="ds">${it.about || it.kind}</span></div>`)}</div>
  </div>`;
}
