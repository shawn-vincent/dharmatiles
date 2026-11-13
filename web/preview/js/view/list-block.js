// The tile level's block for print lists: add the tile to one.  `ListBlock({t, lists, addTo})` is a function of the tile and
// of the print lists there are; which list is picked is the block's own.

import { html, useState } from './html.js';

export function ListBlock({ t, lists, addTo }) {
  const [pick, setPick] = useState(null);
  const chosen = pick ?? lists[0]?.id ?? '';
  return html`<section class="inv-block"><h3>Print list</h3>
    <div class="inv-ask"><select class="word-select" aria-label="Print list" value=${lists.some((l) => l.id === chosen) ? chosen : ''} onChange=${(e) => setPick(e.target.value)}>
        ${lists.map((l) => html`<option value=${l.id} key=${l.id}>${l.name}</option>`)}<option value="">A new list</option></select>
      <button type="button" class="btn" title="One more copy of this tile in the list; a tile already there at the same seed has its count raised" onClick=${() => addTo(t, chosen || null)}>Add to print list</button></div>
  </section>`;
}
