// Copy: the one control for copying a value anywhere in the chrome.  A small
// copy icon that turns into a check mark for a moment once the value is on
// the clipboard.  The look is in docs/assets/theme.css (.copy); a button may
// also carry a label ("Copy details"), which reads "Copied" while checked.

const DONE_MS = 1600;

/** A copy button for `text()` (a string, or a function giving one when pressed). */
export function copyButton(text, { label = '', title = 'Copy' } = {}) {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = label ? 'btn copy labelled' : 'btn icon copy';
  b.title = title;
  b.setAttribute('aria-label', title);
  const i = document.createElement('i');
  i.className = 'copy-glyph';
  i.setAttribute('aria-hidden', 'true');
  b.append(i);
  const words = document.createElement('span');
  if (label) { words.textContent = label; b.append(words); }
  let timer = 0;
  b.addEventListener('click', async () => {
    const value = typeof text === 'function' ? text() : text;
    try {
      await navigator.clipboard.writeText(value);
    } catch (e) {
      // Expected when the browser refuses the clipboard: say so on the button, where it was pressed.
      b.classList.remove('done');
      b.title = `Could not copy (${e.message}); select it and copy it yourself`;
      if (label) words.textContent = 'Could not copy';
      return;
    }
    b.classList.add('done');
    b.title = 'Copied';
    if (label) words.textContent = 'Copied';
    clearTimeout(timer);
    timer = setTimeout(() => {
      b.classList.remove('done');
      b.title = title;
      if (label) words.textContent = label;
    }, DONE_MS);
  });
  return b;
}

/** The parts of a version as the About box shows them: `0.9.0` and `+123.gfd383a7.dirty`. */
export function versionParts(v) {
  const at = v.indexOf('+');
  return at < 0 ? { semver: v, build: '', dirty: false } : { semver: v.slice(0, at), build: v.slice(at), dirty: /(^|[+.])dirty$/.test(v.slice(at)) };
}
