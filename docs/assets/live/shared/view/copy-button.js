// Copy: the one control for copying a value anywhere in the chrome.  A small copy icon that turns into a check mark for a
// moment once the value is on the clipboard.  The look is in docs/assets/theme.css (.copy); a button may also carry a label
// ("Copy details"), which reads "Copied" while checked.  What phase it is in (pressed, failed) is the button's own.

import { html, useEffect, useRef, useState } from './html.js';
import { Icon } from './icons.js';

const DONE_MS = 1600;
const FAILED_MS = 5000;

/** A copy button for `text` (a string, or a function giving one, or a promise of one, when pressed). */
export function CopyButton({ text, label = '', title = 'Copy' }) {
  const [phase, setPhase] = useState(/** @type {{kind: 'done' | 'failed', why?: string} | null} */ (null));
  const timer = useRef(0);
  useEffect(() => () => clearTimeout(timer.current), []);
  const press = async () => {
    try {
      const value = await (typeof text === 'function' ? text() : text);
      await navigator.clipboard.writeText(value);
    } catch (e) {
      // Expected when the browser refuses the clipboard: say so on the button, where it was pressed.
      setPhase({ kind: 'failed', why: /** @type {Error} */ (e).message });
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setPhase(null), FAILED_MS); // the words go back, as they did when the panel was drawn again
      return;
    }
    setPhase({ kind: 'done' });
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setPhase(null), DONE_MS);
  };
  const shownTitle = phase?.kind === 'done' ? 'Copied' : phase?.kind === 'failed' ? `Could not copy (${phase.why}); select it and copy it yourself` : title;
  const words = phase?.kind === 'done' ? 'Copied' : phase?.kind === 'failed' ? 'Could not copy' : label;
  return html`<button type="button" class=${`${label ? 'btn copy labelled' : 'btn icon copy'}${phase ? ` ${phase.kind}` : ''}`} title=${shownTitle} aria-label=${title} onClick=${press}><${Icon} name=${phase?.kind === 'done' ? 'check' : 'copy'} size=${14} />${label ? html`<span>${words}</span>` : null}</button>`;
}
