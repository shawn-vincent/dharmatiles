// A text or number field whose words the model gives and the user may be in the middle of changing.
//
// What the user has typed and not yet committed is the field's own (a draft), and it is the only thing a field
// keeps.  While there is a draft the field shows it, whatever the model says meanwhile: the program may tell the
// panel a new document state at any moment (a build finished, an outside change) and the half-typed word stays
// where it is, with the focus.  The draft is committed (`onCommit(text)`) and the field shows the model's word
// again: on Enter (`commit="enter"`, the default) or, for a number field, when the browser says it changed
// (`commit="change"`: Enter or leaving it).  Leaving the field without committing drops the draft.  Without a
// draft the field shows the model's word, so a change the program makes shows at once.

import { html, useRef, useState } from './html.js';

/**
 * @param {{value: string | number, onCommit: (text: string) => void, commit?: 'enter' | 'change', [attr: string]: any}} props
 */
export function Field({ value, onCommit, commit = 'enter', ...attrs }) {
  const draft = useRef(/** @type {string | null} */ (null));
  const [, redraw] = useState(0);
  const shown = draft.current ?? String(value);
  const done = (/** @type {string} */ text) => { draft.current = null; redraw((n) => n + 1); onCommit(text); };
  return html`<input ...${attrs} value=${shown}
    onInput=${(/** @type {any} */ e) => { draft.current = e.target.value; }}
    onKeyDown=${(/** @type {any} */ e) => { if (commit === 'enter' && e.key === 'Enter') done(e.target.value); }}
    onChange=${(/** @type {any} */ e) => { if (commit === 'change') done(e.target.value); }}
    onBlur=${() => { if (draft.current !== null) { draft.current = null; redraw((n) => n + 1); } }} />`;
}
