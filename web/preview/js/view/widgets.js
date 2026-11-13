// The small pieces every panel is made of, as components: a button, a section with a heading, a row of
// buttons with one pressed (`Seg`), a plural.  Same markup and classes the panels have always had.

import { html } from './html.js';

export const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export const Button = ({ text, run, title, cls = 'btn', disabled = false, type = 'button' }) => html`<button type=${type} class=${cls} title=${title} disabled=${disabled} onClick=${run}>${text}</button>`;

export const Section = ({ title, cls = '', children }) => html`<section class=${`pl-sec${cls ? ` ${cls}` : ''}`}>${title ? html`<h3>${title}</h3>` : null}${children}</section>`;

/** A row of buttons, one pressed: `options` are [value, text, title]; pressing another one calls `on(value)`. */
export const Seg = ({ label, options, current, on, disabled }) => html`<div class="pl-field"><span class="pl-lab">${label}</span>
  <div class="seg" role="group" aria-label=${label}>${options.map(([v, text, title]) => html`<button type="button" key=${v} aria-pressed=${String(v === current)} title=${title} disabled=${!!disabled} onClick=${() => { if (v !== current) on(v); }}>${text}</button>`)}</div></div>`;

/** A note under a heading: `cls` adds its tone (`ok`, `bad`, `warn`). */
export const Note = ({ cls = '', children }) => html`<p class=${`note${cls ? ` ${cls}` : ''}`}>${children}</p>`;
