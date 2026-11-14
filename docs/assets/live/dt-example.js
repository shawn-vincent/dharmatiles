// <dt-example>: a live example on a page.  The page is complete without this script: the generator (`cargo xtask docs`) wrote the
// poster's picture and the source as real text inside the element.  This file only makes the element come alive when it is
// touched, and costs almost nothing before that (the engine is 3.5 MB of WebAssembly and is not fetched until wanted):
//
//   cold      the poster and the source, as the page was written
//   waking    touched: the engine module is fetched, compiled once and shared by every card on the page
//   live      ./card.js builds the card (source, tile, controls, messages, overlay) over the poster
//
// Scrolling past a card does nothing.  Pointer down on the picture, or pointer up on the source with nothing selected (so the
// source can still be selected and copied), or a press of the Edit button, wakes it.  After the page is idle the module is
// fetched quietly (not on a metered link), so the first touch is quick.
//
// Attributes: src (the example's file under docs/, the one the tests build), controls (`wobble, seed`, or `wobble:0..12`),
// overlay (the toggles that start on: lines points regions squares), highlight (marks pinned for the sentence being read:
// `line shore, point 2`), expect-error (the code the source must give: the tests hold it), playground (the source may come
// from the address: `#src=…`).

import { Engine } from './engine.js';

// the docs folder: this file is docs/assets/live/dt-example.js
const root = new URL('../../', import.meta.url);
export const engine = new Engine(root);

class DtExample extends HTMLElement {
  connectedCallback() {
    if (this.dataset.ready) return;
    this.dataset.ready = 'yes';
    this.dataset.state = 'cold';
    if (this.hasAttribute('playground')) this.#fromAddress();
    const view = this.querySelector('.dtx-view');
    const code = this.querySelector('.dtx-code');
    if (view) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'btn sm dtx-wake';
      b.textContent = 'Edit this example';
      b.addEventListener('click', () => this.wake());
      view.append(b);
      view.addEventListener('pointerdown', () => this.wake());
    }
    // marks the page picks are on the picture from the start, drawn from the map beside the poster, when it comes into view
    if (view && (this.hasAttribute('overlay') || this.hasAttribute('highlight')) && this.querySelector('.dtx-poster')) {
      const io = new IntersectionObserver((seen) => {
        if (!seen.some((e) => e.isIntersecting)) return;
        io.disconnect();
        import('./poster-overlay.js').then((m) => m.drawOnPoster(this)).catch((e) => {
          // loud, in the card: a poster that should carry marks and does not is a defect
          const p = document.createElement('p');
          p.className = 'dtx-fail';
          p.setAttribute('role', 'alert');
          p.textContent = `The marks on this picture could not be drawn: ${e.message}`;
          this.prepend(p);
        });
      });
      io.observe(this);
    }
    // a card with no picture (an example that must fail) wakes from its source
    code?.addEventListener('pointerup', () => { if (getSelection()?.isCollapsed) this.wake(); });
    code?.setAttribute('tabindex', '0');
    code?.addEventListener('focus', () => this.wake());
  }

  /** A playground's source can come in the address, as `#src=` and base64url. */
  #fromAddress() {
    const m = /^#src=([A-Za-z0-9_-]+)$/.exec(location.hash);
    if (!m) return;
    try {
      const bin = atob(m[1].replace(/-/g, '+').replace(/_/g, '/'));
      const text = new TextDecoder().decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
      const code = this.querySelector('.dtx-code code');
      if (code) code.textContent = text;
      // the poster was of another tile
      this.querySelector('.dtx-poster')?.remove();
    } catch (e) {
      throw new Error(`the playground's address holds a source that cannot be read: ${e.message}`);
    }
  }

  /** Bring the card to life; every touch after the first does nothing. */
  async wake() {
    if (this.live || this.waking) return;
    this.waking = true;
    this.dataset.state = 'waking';
    try {
      const { Card } = await import('./card.js');
      this.live = new Card(this, engine);
    } catch (e) {
      // loud, in the card: never a card that silently stays a picture
      this.dataset.state = 'failed';
      const p = document.createElement('p');
      p.className = 'dtx-fail';
      p.setAttribute('role', 'alert');
      p.textContent = `This example could not start: ${e.message}`;
      this.prepend(p);
      throw e;
    }
  }
}

if (!customElements.get('dt-example')) customElements.define('dt-example', DtExample);

// after the page has settled, fetch the module quietly; nothing is compiled or started
addEventListener('load', () => {
  const idle = globalThis.requestIdleCallback ?? ((f) => setTimeout(f, 2000));
  idle(() => engine.prefetch());
});
