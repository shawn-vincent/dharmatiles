// The title bar's parts that say something: the Undo and Redo menus (what each step would take back or make again), the
// text's save state with the choice a file changed on disk asks for, and the export card's line about what was written.
// Each is a function of what it is given; the editor draws them from the history, the document and the export's state.

import { Fragment, html } from './html.js';
import { Thumb } from '../thumbs.js';

/**
 * One entry of the history as a menu button: what it did, how far away it is, and the text it took out and put in.  With `pictures`
 * (`{svc, now, spec(text), onError}`, given only while the menu is open and the text is a tile's own) each row also shows the tile
 * as it is now and as the row would leave it: the program builds each text low, one at a time off the edit's path, and
 * what the menu no longer shows is not built (js/thumbs.js).  A text with a mistake in it says so in place of a picture.
 */
export function HistoryMenu({ items, onPick, pictures = null }) {
  return html`${items.map(({ entry, target, distance, direction, text }) => html`<button type="button" key=${`${direction}${target}`} class=${pictures ? 'with-pic' : undefined} title=${`${direction} to ${entry.label}`} onClick=${() => onPick(direction, target)}>
    ${pictures && text != null ? html`<span class="pics" aria-hidden="true"><${Thumb} svc=${pictures.svc} spec=${pictures.spec(pictures.now)} className="h-pic" onError=${pictures.onError} /><i class="h-to"></i><${Thumb} svc=${pictures.svc} spec=${pictures.spec(text)} className="h-pic" onError=${pictures.onError} /></span>` : null}
    <span class="action">${entry.label}</span><span class="distance">${distance === 1 ? '1 step' : `${distance} steps`}</span>
    ${entry.change?.removed || entry.change?.added ? html`<span class="change">${entry.change.removed ? html`<code class="removed">${`- ${entry.change.removed}`}</code>` : null}${entry.change.added ? html`<code class="added">${`+ ${entry.change.added}`}</code>` : null}</span>` : null}</button>`)}`;
}

/** Where the text stands in words; a file that changed on disk offers the two ways out. */
export function SaveState({ words, conflict, onTake, onKeep }) {
  if (conflict) return html`changed on disk: <button type="button" onClick=${onTake}>take the disk’s</button> or <button type="button" onClick=${onKeep}>keep mine</button>`;
  return words ? html`${words.text}` : null;
}

/** What the export did: that it is building, why it cannot, what was written (the files, the whole path on hover) or where it would write. */
export function ExportWhy({ busy, error, files, why, out }) {
  if (busy) return html`Building through the gate…`;
  if (error) return html`${error}`;
  if (files) {
    const shown = files.filter((f) => /\.(stl|3mf)$/.test(f));
    return html`Written: ${shown.map((f, i) => html`<${Fragment} key=${f}>${i ? ', ' : ''}<code title=${f}>${f.split(/[\\/]/).slice(-3).join('/')}</code><//>`)}`;
  }
  return html`${why || (out ? `Writes to ${out.shown}` : 'Writes to out/, as dharmatiles build does.')}`;
}
