// Copy: the one control for copying a value anywhere in the chrome.  A small
// copy icon that turns into a check mark for a moment once the value is on
// the clipboard.  The look is in docs/assets/theme.css (.copy); a button may
// also carry a label ("Copy details"), which reads "Copied" while checked.

import { CopyButton } from './view/copy-button.js';
import { html, render } from './view/html.js';

/** A copy button (the component is view/copy-button.js) as an element, for the one place that is still drawn by hand. */
export function copyButton(text, opts = {}) {
  const holder = document.createElement('span');
  holder.style.display = 'contents';
  render(html`<${CopyButton} text=${text} ...${opts} />`, holder);
  return holder;
}

/** The parts of a version as the About box shows them: `0.9.0` and `+123.gfd383a7.dirty`. */
export function versionParts(v) {
  const at = v.indexOf('+');
  return at < 0 ? { semver: v, build: '', dirty: false } : { semver: v.slice(0, at), build: v.slice(at), dirty: /(^|[+.])dirty$/.test(v.slice(at)) };
}
