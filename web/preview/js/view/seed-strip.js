// @ts-check
// The seed strip (docs/preview.html, "The seed"): five small pictures of a tile, each built low at its seed by the program
// on demand (js/thumbs.js), the current one first, a square on the seeds you own.  The tile level's seed row and a map piece's
// Seed level both draw it from here, with the one `seedStripOf` model (editor/scene-thumbs.js): one component, never two.

import { html } from './html.js';
import { Icon } from './icons.js';
import { Thumb } from '../thumbs.js';

/** A seed nobody has rolled yet. */
export const newSeed = () => 1 + Math.floor(Math.random() * 9999);

/** Five seeds to look at: the one the tile has now first (`null`: its own default), then four new ones. @param {number | null} current */
export const candidates = (current) => {
  const seeds = [current];
  while (seeds.length < 5) { const x = newSeed(); if (!seeds.includes(x)) seeds.push(x); }
  return seeds;
};

/** The dice: press it to look at five arrangements (with a strip to open) or, with none, to roll one at once. @param {{ roll?: (() => void) | null, onRoll?: () => void }} p */
export function SeedDice({ roll, onRoll }) {
  return html`<button type="button" class="dice" title=${roll ? 'Show five arrangements to choose from' : 'Roll a new seed: another arrangement'} aria-label=${roll ? 'Show five seeds' : 'Roll a new seed'} onClick=${roll ?? onRoll}><${Icon} name="dice" size=${16} /></button>`;
}

/**
 * What the dice shows: five small pictures of the tile, the current one first.  A square marks a seed you own.  Choosing one
 * writes the seed, one step of history; choosing the current one writes nothing.  `s` is a `seedStripOf` model.
 */
export function SeedStrip({ s, seeds, ro, onPick, onMore, onClose }) {
  return html`<div class="seed-strip" role="group" aria-label="Seeds to choose from">${seeds.map((seed) => {
    const now = seed === s.current;
    return html`<button type="button" key=${seed ?? 'own'} class=${`seed-card${now ? ' on' : ''}`} disabled=${ro && !now} aria-pressed=${String(now)}
      title=${now ? 'The seed the tile has now' : `Use seed ${seed}`} onClick=${() => onPick(seed)}>
      <${Thumb} svc=${s.svc} spec=${{ ...s.spec, seed: seed ?? undefined }} onError=${s.onError} />
      <b>${seed ?? 'default'}</b>${now ? html`<span class="now">now</span>` : null}${seed != null && s.owned.has(seed) ? html`<i class="own" title="You have printed this one"></i>` : null}</button>`;
  })}</div>
  <div class="seed-strip-acts"><button type="button" class="btn" onClick=${onMore}>Roll again</button><button type="button" class="btn" onClick=${onClose}>Close</button></div>`;
}
