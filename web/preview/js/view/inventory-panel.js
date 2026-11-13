// The inventory (docs/inventory.html) as two components: `HaveBlock`, which the tile level shows ("3 printed · seeds
// 7, 12, 97" and **I have this**), and `InventoryPanel`, the inventory document's own view: its pieces grouped by
// tile with the filters (system, colour, have), the per-line actions (gone, reprint), a tile that moved with its
// one-click re-link, and **Source**, the file's own KDL.
//
// Both are functions of the model (js/editor/model.js `inventoryPanel`) and of `act`, what the buttons do (every
// action is one line added at the end of the log, never an edit of an earlier line).  What a user is choosing (the
// filters, the face, the piece a reprint form is open under, the colour for I have this) is the model's, so it is the
// same when the panel is drawn again; the list a reprint goes into and the tile a re-link points to are the form's own.

import {
  SYSTEMS, canonOf, colourName, groups, haveLine, nameOf, numbered, relinkCandidates, systemName, tileHave, totalsLine, exists,
} from '../inventory.js';
import { html, useState } from './html.js';
import { Seg, plural } from './widgets.js';

/** What the inventory says of tile `t`, and the one button that adds to it. */
export function HaveBlock({ t, m, act }) {
  const inv = m.inventory.value;
  const system = m.system.value;
  const h = tileHave(inv, canonOf(t.path, inv?.home), system);
  const colour = m.colour.value;
  return html`<section class="inv-block"><h3>${`Inventory · ${systemName(system)}`}</h3>
    <p class=${`inv-have${h.have ? '' : ' none'}`}>${exists(inv) ? haveLine(h) : 'No inventory yet. The first print you add makes the file.'}</p>
    ${h.edited ? html`<p class="inv-edited" title="The tile changed since the print you have was made. It still counts as had; this is only to tell you.">Edited since printing</p>` : null}
    <div class="inv-ask"><${Seg} label="Colour" options=${[['single', 'Single colour', 'One filament, no paint'], ['multi', 'Multicolour', 'Each material in its own filament']]} current=${colour} on=${(c) => act.pickColour(c)} />
      <button type="button" class="btn" title=${`Add one good print of this tile (its seed now, ${systemName(system)}, ${colourName(colour)}) to your inventory, without exporting anything`} onClick=${(e) => act.haveThis(t, colour, e.currentTarget)}>I have this</button></div>
  </section>`;
}

function PieceRow({ g, p, system, m, act }) {
  const key = `${p.tile}|${p.seed}|${p.system}|${p.colour}`;
  const open = m.ui.value.reprint === key;
  return html`<div class="inv-wrap">
    <div class=${`inv-piece${p.have ? '' : ' off'}`} title=${system && p.system !== system ? systemName(p.system) : undefined}>
      <span class="inv-what"><b>${`seed ${p.seed}`}</b><span class="inv-colour">${colourName(p.colour)}</span></span>
      <span class="inv-nums"><span class="inv-count">${`×${p.have}`}</span>${p.failed ? html`<span class="inv-tag">${`${p.failed} failed`}</span>` : null}${p.gone ? html`<span class="inv-tag">${`${p.gone} gone`}</span>` : null}${p.last ? html`<span class="inv-tag date">${p.last}</span>` : null}</span>
      <span class="inv-acts">
        <button type="button" class="linkish" disabled=${p.have === 0} title=${p.have ? 'One of these is lost or broken: a line is added to the log, nothing is edited away' : 'None of these is on the shelf'} onClick=${() => act.gone(g, p)}>Gone</button>
        <button type="button" class="linkish" title="Add this exact piece (its seed) to a print list" onClick=${() => act.toggleReprint(key)}>Reprint</button>
      </span></div>
    ${open ? html`<${Reprint} p=${p} m=${m} act=${act} />` : null}
  </div>`;
}

/** The small form under a piece: which print list it goes into (or a new one). */
function Reprint({ p, m, act }) {
  const [list, setList] = useState('');
  return html`<div class="inv-reprint"><span class="pl-lab">${`Seed ${p.seed} into`}</span>
    <select class="word-select" aria-label="Print list" value=${list} onChange=${(e) => setList(e.target.value)}><option value="">A new print list</option>${m.lists.value.map((l) => html`<option value=${l.id} key=${l.id}>${l.name}</option>`)}</select>
    <button type="button" class="btn" onClick=${() => act.reprint(p, list)}>Add</button>
    <button type="button" class="linkish" onClick=${() => act.toggleReprint(null)}>Cancel</button></div>`;
}

/** A tile that is not at its path any more: the one-click re-link, which adds a `renamed` line. */
function Relink({ g, m, act }) {
  const cands = relinkCandidates(m.tiles.value, g.tile);
  const same = cands.filter((c) => nameOf(canonOf(c.path, '')) === nameOf(g.tile));
  const home = m.inventory.value?.home;
  const [to, setTo] = useState('');
  const chosen = to || cands[0]?.id;
  return html`<div class="inv-relink"><span class="inv-gone">Not at this path any more.</span>
    ${same.length === 1 ? html`<button type="button" class="btn" onClick=${() => act.relink(g, same[0].id, same[0].name)}>${`Re-link to ${canonOf(same[0].path, home)}`}</button>`
    : cands.length ? html`<select class="word-select" aria-label="The tile it is now" value=${chosen} onChange=${(e) => setTo(e.target.value)}>${cands.map((c) => html`<option value=${c.id} key=${c.id}>${canonOf(c.path, home)}</option>`)}</select>
      <button type="button" class="btn" onClick=${() => act.relink(g, chosen, cands.find((c) => c.id === chosen)?.name)}>Re-link</button>` : null}</div>`;
}

function Group({ g, system, m, act }) {
  return html`<section class="inv-group"><div class="inv-head"><b>${g.name}</b><span class="inv-path">${g.tile}</span>
      ${g.edited ? html`<span class="inv-tag edited" title="The tile changed since the print was made. It still counts; this is only to tell you.">edited since printing</span>` : null}</div>
    ${g.missing ? html`<${Relink} g=${g} m=${m} act=${act} />` : null}
    ${g.pieces.map((p) => html`<${PieceRow} key=${`${p.tile}|${p.seed}|${p.system}|${p.colour}`} g=${g} p=${p} system=${system} m=${m} act=${act} />`)}</section>`;
}

function Source({ text }) {
  return html`<pre class="inv-source" tabIndex="0">${numbered(text).map(([n, line]) => html`<div class="ln" key=${n}><span class="no">${String(n)}</span><span class="tx">${line || ' '}</span></div>`)}</pre>
    <p class="note">Edit the file by hand in any editor: this view follows it. Counts are never stored; they are worked out from these lines.</p>`;
}

export function InventoryPanel({ m, act }) {
  const t = m.tile.value;
  if (!t || !m.on.value) return null; // not the inventory's turn: the panel draws nothing
  const d = m.data.value;
  const f = m.filters.value;
  const failed = !!d.problems?.length || t.status === 'failed';
  const face = failed && m.ui.value.face === 'view' && d.text ? 'source' : m.ui.value.face;
  const gs = !failed && face !== 'source' ? groups({ pieces: d.pieces }, f) : [];
  const others = d.pieces.filter((p) => p.system !== f.system).length;
  return html`<div class="lt"><div class="heading"><span class="cap">Inventory</span><b>${t.path.split(/[\\/]/).pop()}</b></div>
      <div class="seg inv-face" role="group" aria-label="What the panel shows">${[['view', 'Tile'], ['source', 'Source']].map(([v, label]) => html`<button type="button" key=${v} aria-pressed=${String(face === v)} title=${v === 'view' ? 'What you have, by tile' : 'The file itself: KDL, one line a print, a loss or a rename'} onClick=${() => act.setFace(v)}>${label}</button>`)}</div></div>
    <div class="body">
      ${failed ? html`<p class="note bad">This inventory does not read, so nothing is counted from it.</p>${(d.problems ?? t.errors ?? []).slice(0, 4).map((e, i) => html`<p class="note bad" key=${i}>${e.message}${e.help ? html`<span class="pl-help">${` ${e.help}`}</span>` : null}</p>`)}` : null}
      ${face === 'source' ? html`<${Source} text=${d.text} />` : !failed ? html`
        <div class="inv-filters">
          <${Seg} label="System" options=${SYSTEMS.map(([v, n]) => [v, n, `Only ${n} pieces: DungeonBlocks and OpenLOCK never count for each other`])} current=${f.system} on=${(v) => act.filter({ system: v })} />
          <${Seg} label="Colour" options=${[[null, 'Both'], ['single', 'Single'], ['multi', 'Multi']]} current=${f.colour} on=${(v) => act.filter({ colour: v })} />
          <${Seg} label="Show" options=${[['all', 'All'], ['have', 'On the shelf'], ['off', 'Not on the shelf']]} current=${f.show} on=${(v) => act.filter({ show: v })} />
        </div>
        <p class="inv-totals">${gs.length ? totalsLine(gs) : 'Nothing matches these filters.'}</p>
        ${gs.map((g) => html`<${Group} key=${g.tile} g=${g} system=${f.system} m=${m} act=${act} />`)}
        ${others ? html`<p class="note">${`${plural(others, 'piece line')} in ${systemName(f.system === 'db' ? 'ol' : 'db')} not shown: the other system is a switch away.`}</p>` : null}` : null}
    </div>`;
}
