// The catalog drawer (docs/maps.html): the folder level with pictures, as a drawer along the bottom of the map.  The same
// folders as the panel's, so there is no second place to keep tiles: a card is a tile with its picture, its size and what you
// have of it (the pills of the folder rows), and it either goes on the map when pressed (the nearest free place) or is
// dragged onto the map and dropped where it should stand.
//
// `Drawer({m, act})` is a function of the model (`m`: the level and the filter, the program's list, the system, the plate
// colour) and of `act` (what a card or a crumb does).  A card's picture is asked for when the card is near the strip's visible
// part, and drawn when it comes.

import { sizeLine, stemOf } from '../folders.js';
import { html, useEffect, useRef, useState } from './html.js';

/** A card's picture: the tile's picture once it is near (`act.picture` answers its url), else the empty plate. */
function Pic({ t, plate, act }) {
  const box = useRef(/** @type {HTMLElement | null} */ (null));
  const [url, setUrl] = useState(/** @type {string | null} */ (null));
  const [bad, setBad] = useState(false);
  useEffect(() => {
    const el = box.current;
    if (!el) return undefined;
    let live = true;
    // asked for when it is near: the strip scrolls sideways, so the margin is on the sides
    const seen = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      seen.disconnect();
      act.picture(t, plate).then((u) => { if (live && u) setUrl(u); }).catch((e) => { if (live) setBad(true); act.pictureFailed(e); });
    }, { root: el.closest('.dr-strip'), rootMargin: '0px 400px 0px 400px' });
    seen.observe(el);
    return () => { live = false; seen.disconnect(); };
  }, [t.id, t.meshVersion, plate]);
  return html`<div class=${`dr-pic${bad ? ' bad' : ''}`} ref=${box} data-id=${t.id} data-key=${String(t.meshVersion ?? 0)}>${url ? html`<img alt="" src=${url} draggable=${false} />` : null}</div>`;
}

function Card({ t, sys, pill, plate, act }) {
  return html`<div class="dr-card" draggable=${true} title=${`${t.path.replace(/^lib:/, '')}\nPress to put it on the map; drag it to choose where`}
    onDragStart=${(e) => act.dragTile(e, t)} onClick=${() => act.addTile(t)}>
    <${Pic} t=${t} plate=${plate} act=${act} />
    <div class="dr-row"><b>${stemOf(t.path)}</b><span class="dr-size">${sizeLine(t)}</span></div>
    ${pill ? html`<span class=${`ipill ${pill.kind}`} title=${pill.title}>${pill.text}</span>` : null}</div>`;
}

export function Drawer({ m, act }) {
  const d = m.data.value;
  if (!d) return null; // not the drawer's turn
  const { level, query, lv, places, plate } = d;
  const parts = level.dir ? level.dir.split('/') : [];
  return html`<div class="dr-head"><b>Catalog</b>
      <select class="word-select" aria-label="Place" value=${String(level.root)} onChange=${(e) => act.goTo({ root: Number(e.target.value), dir: '' })}>${places.map((r) => html`<option value=${String(r.index)} key=${r.index}>${r.label}</option>`)}</select>
      <nav class="dr-crumbs"><button type="button" class="crumb" onClick=${() => act.goTo({ root: level.root, dir: '' })}>/</button>${parts.map((p, i) => html`<span class="sep">›</span><button type="button" class="crumb" key=${i} onClick=${() => act.goTo({ root: level.root, dir: parts.slice(0, i + 1).join('/') })}>${p}</button>`)}</nav>
      <input type="search" placeholder="Filter tiles" value=${query} aria-label="Filter the catalog" onInput=${(e) => act.filter(e.target.value)} />
      <button type="button" class="pane-x" title="Close the catalog" aria-label="Close the catalog" onClick=${act.close}>×</button></div>
    <div class="dr-strip">${!lv ? html`<p class="note">This place is not listed now.</p>` : html`
      ${lv.folders.map((f) => html`<button type="button" class="dr-folder" key=${`f${f.dir}`} onClick=${() => act.goTo({ root: level.root, dir: f.dir })}><b>${f.name}</b><span class="dr-size">${`${f.tiles} tile${f.tiles === 1 ? '' : 's'}`}</span></button>`)}
      ${lv.items.map((it) => html`<${Card} key=${`t${it.tile.id}`} t=${it.tile} sys=${d.sys} pill=${act.pill(it.tile)} plate=${plate} act=${act} />`)}
      ${!lv.items.length && !lv.folders.length ? html`<p class="note">${query ? 'No tile here has that in its name.' : 'No tiles in this folder.'}</p>` : null}`}</div>`;
}
