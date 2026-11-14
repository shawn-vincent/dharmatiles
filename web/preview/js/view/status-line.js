// @ts-check
// The status line: a dot, one verdict, the counts, and a bar for work in flight.  One component for the editor's bottom bar
// and the site's example cards, so both say a state the same way (js/messages.js `statusOf` makes the words; this puts them
// on the page).  `paintStatusLine` writes into the parts it is given: the editor's are the elements of its page (#statusbar), a
// card's are made by `buildStatusLine`.  It is a button: `aria-expanded` says whether the list it folds is open.

/**
 * @typedef {{ root: HTMLElement, main: HTMLElement, dot: HTMLElement, verdict: HTMLElement, counts: HTMLElement, bar: HTMLElement }} StatusParts
 * @typedef {{ verdict: string, counts?: string, tone: string, progress: number | null, expanded?: boolean }} StatusWords
 */

/**
 * Put `s` on the line.  `tone` is ok / note / bad / busy / idle; `progress` null hides the bar.
 * @param {StatusParts} parts @param {StatusWords} s
 */
export function paintStatusLine(parts, s) {
  parts.verdict.textContent = s.verdict;
  parts.counts.textContent = s.counts ?? '';
  parts.dot.className = `dot ${s.tone}`;
  parts.root.dataset.tone = s.tone;
  parts.bar.style.width = s.progress == null ? '0' : `${Math.round(s.progress * 100)}%`;
  parts.bar.toggleAttribute('hidden', s.progress == null);
  if (s.expanded != null) parts.main.setAttribute('aria-expanded', String(s.expanded));
}

/**
 * A status line of its own, for a card: the parts, the root to put on the page.  `onToggle` runs when the button is pressed.
 * @param {{ onToggle?: () => void, controls?: string }} [o]
 * @returns {StatusParts}
 */
export function buildStatusLine({ onToggle, controls } = {}) {
  /** @param {string} tag @param {string} cls @param {Record<string, string>} [attrs] */
  const make = (tag, cls, attrs = {}) => {
    const e = document.createElement(tag);
    e.className = cls;
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    return e;
  };
  const root = make('div', 'statusline', { role: 'status' });
  const bar = make('i', 'sl-bar', { 'aria-hidden': 'true' });
  bar.setAttribute('hidden', '');
  const main = make('button', 'sl-main', { type: 'button', 'aria-expanded': 'false', title: 'Messages' });
  if (controls) main.setAttribute('aria-controls', controls);
  const dot = make('i', 'dot idle', { 'aria-hidden': 'true' });
  const verdict = make('span', 'sl-verdict');
  const counts = make('span', 'sl-counts');
  main.append(dot, verdict, counts);
  if (onToggle) main.addEventListener('click', onToggle);
  root.append(bar, main);
  return { root, main, dot, verdict, counts, bar };
}
