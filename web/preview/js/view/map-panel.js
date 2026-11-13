// The map's panel (docs/maps.html): the levels of a map, one at a time, as the tile's are: the map (its pieces
// by tile, what the shelf says of them, the variants and the way to the plates), one piece (its place, turn,
// level, seed and print setting, with Open tile), and a property of it (the level's names, the seeds you
// own).  Where you are is the selection and the property open on it, so a click on a piece in the view brings the
// panel to it and Back climbs.
//
// `MapPanel` is a function of the model (`m`, js/editor/model.js `mapPanel`: signals) and of the actions (`act`,
// what the buttons do: each one is a change of the document, editor/scene-edit.js, or of the view).  It draws
// what the model says and keeps nothing: a new state of the map from the program is drawn over the panel as it
// is, so a word being typed is not lost (field.js).

import * as M from '../map-model.js';
import { haveLine, systemName, tileHave } from '../inventory.js';
import { Field } from './field.js';
import { html } from './html.js';
import { Button, Section, Seg, plural } from './widgets.js';

/** The have or to-print pill of a piece (caution yellow where only the other colour mode is on the shelf). */
export const Pill = ({ state }) => {
  const word = { have: 'have', need: 'to print', other: 'have, other colour', skip: 'left off' }[state] ?? state;
  const title = {
    have: 'A print you own is this piece (tile, seed, system and colour mode): it is left off the plates',
    need: 'It prints',
    other: 'You own this tile and seed in the other colour mode; it prints unless you leave it off',
    skip: 'print=skip: it stays on the map and off the plates',
  }[state];
  return html`<span class=${`ipill ${state === 'have' ? 'have' : state === 'other' ? 'caution' : 'need'}`} title=${title}>${word}</span>`;
};

function Head({ m, act }) {
  const level = m.level.value;
  const sel = m.selected.value;
  const crumbs = [{ label: m.mapName.value, go: () => { act.setProp(null); act.select([]); } }];
  if (level !== 'map') {
    const p = sel[0];
    crumbs.push({ label: sel.length > 1 ? `${sel.length} pieces` : (p.id ? `${p.name} · ${p.id}` : p.name), go: () => act.setProp(null) });
  }
  if (level === 'prop') crumbs.push({ label: { bottom: 'Bottom', seed: 'Seed', print: 'Print' }[m.mprop.value] ?? 'Property' });
  return html`<div class="lt"><div class="heading"><span class="cap">Map</span>
    <nav class="mp-crumbs">${crumbs.map((c, i) => html`${i ? html`<span class="sep">›</span>` : null}<button type="button" class=${`crumb${i === crumbs.length - 1 ? ' cur' : ''}`} onClick=${c.go}>${c.label}</button>`)}</nav></div></div>`;
}

function MapLevel({ m, act }) {
  const ro = m.readOnly.value;
  const ps = m.pieces.value;
  const sc = m.scene.value;
  const states = m.marks.value;
  const placed = ps.filter((p) => p.at);
  const tray = ps.filter((p) => !p.at);
  return html`
    <${Section} title="On the map">
      <p class="inv-sum"><b>${sc?.shelfExists ? `${sc.have} in inventory` : 'No inventory yet'}</b>${` · ${sc?.toPrint ?? 0} to print`}</p>
      <div class="mp-row">
        <${Button} text="Add tiles" run=${act.toggleDrawer} title="The catalog: drag a tile onto the map, or press one to put it down" disabled=${ro} />
        <${Button} text="Print what’s missing" run=${() => act.chooseMode('plates')} cls="btn on" title="The plates for this map: only the pieces your inventory lacks" />
      </div>
    <//>
    <${Section} title=${placed.length ? 'Pieces' : 'No pieces yet'}>
      ${placed.length ? null : html`<p class="note">Open the catalog and put tiles on the map. Each is a piece with a place, a turn and a seed; nothing prints until you ask for the plates.</p>`}
      ${M.groupsOf(placed).map((g) => {
        const have = g.pieces.filter((p) => states.get(p.id)?.state === 'have').length;
        return html`<button type="button" class="mp-group" key=${g.canon} title=${`Select the ${plural(g.pieces.length, 'piece')} of ${g.name}`} onClick=${() => act.select(g.pieces.map((p) => p.id))}>
          <b>${g.name}</b><span class="mp-n">${`× ${g.pieces.length}`}</span>
          <span class="mp-pills">${have ? html`<span class="ipill have">${`${have} have`}</span>` : null}${g.pieces.length - have ? html`<span class="ipill need">${`${g.pieces.length - have} to print`}</span>` : null}</span></button>`;
      })}
    <//>
    ${tray.length ? html`<${Section} title="In the tray">
      <p class="note">Pieces with no place yet: they print, and wait to be put on the map.</p>
      ${tray.map((p) => html`<div class="mp-tray" key=${p.id}>
        <b>${p.name}</b><span class="mp-n">${p.count > 1 ? `× ${p.count}` : ''}</span>
        <${Button} text="Place" run=${() => act.place(p.id)} disabled=${ro || !!p.error} title=${p.count > 1 ? 'Put one copy on the map at the nearest free place, with a seed of its own' : 'Put it on the map at the nearest free place'} />
        ${p.count > 1 ? html`<${Button} text="Place all" run=${() => act.place(p.id, true)} disabled=${ro || !!p.error} title=${`Put all ${p.count} on the map, each with a seed of its own`} />` : null}
      </div>`)}
    <//>` : null}
    <${Section} title="Variants">
      <p class="note">A new variant is the same tile with a new seed: a new print. New pieces already get a seed you do not own.</p>
      <div class="mp-row wrap">
        <${Button} text="New seed for every piece I don’t have" run=${() => act.variants('missing')} disabled=${ro || !placed.length} title="Each piece not on your shelf gets a fresh variant; what you own stays as it is" />
        <${Button} text="Whole board" run=${() => act.variants('all')} disabled=${ro || !placed.length} title="A new variant of every piece" />
      </div>
    <//>`;
}

function PieceLevel({ m, act }) {
  const ro = m.readOnly.value;
  const sel = m.selected.value;
  const states = m.marks.value;
  const sc = m.scene.value;
  const p = sel[0];
  const many = sel.length > 1;
  const ids = sel.map((q) => q.id);
  return html`
    <${Section}>
      <div class="mp-title"><b>${many ? `${sel.length} pieces` : p.name}</b>${many ? null : html`${p.id ? html`<span class="mp-id">${p.id}</span>` : null}<${Pill} state=${states.get(p.id)?.state ?? 'need'} />`}</div>
      ${many ? null : html`<p class="note">${`${p.cols}×${p.rows} · ${systemName(p.system)} · seed ${p.seedUsed}${p.seed == null ? ' (the tile’s own)' : ''}`}</p>`}
      ${!many && p.error ? html`<p class="note bad">${p.error}</p>` : null}
      ${!many && p.pinned ? html`<p class="note">Pinned on a plate: the packer keeps it where it was dropped.</p>` : null}
    <//>
    <${Section} title="Piece">
      ${many ? null : html`<div class="pl-field"><span class="pl-lab">Place</span><span class="mp-val">${p.at ? M.atWord(p.at[0] / p.sq, p.at[1] / p.sq) : 'in the tray'}</span></div>`}
      <div class="pl-field"><span class="pl-lab">Turn</span>
        <span class="mp-btns">
          <${Button} text="⟲" run=${() => act.turn(ids, -1)} cls="pl-step" title="A quarter turn counter-clockwise (⇧R)" disabled=${ro} />
          <${Button} text="⟳" run=${() => act.turn(ids, 1)} cls="pl-step" title="A quarter turn clockwise (R)" disabled=${ro} />
          ${many ? null : html`<span class="mp-val">${`${p.rot}°`}</span>`}
        </span></div>
      ${many ? null : html`
        <button type="button" class="mp-prop" title="How high the tile’s zero (its ground level) stands above the map’s ground: a name from the standard heights, or a length. The connector hangs below it." onClick=${() => act.setProp('bottom')}>
          <span class="pl-lab">Bottom</span><span class="mp-val">${M.bottomWords(p, sc?.heights)}</span><span class="mp-go">›</span></button>
        <button type="button" class="mp-prop" title="Which variant of the tile: a new one, or one you already have" onClick=${() => act.setProp('seed')}>
          <span class="pl-lab">Seed</span><span class="mp-val">${String(p.seedUsed)}</span><span class="mp-go">›</span></button>
        <${Seg} label="Print" options=${[['missing', 'If missing', 'Print it unless your inventory has it'], ['always', 'Always', 'Print it whatever you have'], ['skip', 'Skip', 'Leave it off the plates']]} current=${p.print}
          on=${(v) => act.setPiece(p.id, { print: v }, `${p.name}: print ${v === 'missing' ? 'if missing' : v}`)} disabled=${ro} />`}
    <//>
    <${Section} title="Variants">
      <div class="mp-row wrap">
        <${Button} text="New variant" run=${() => act.variants('piece')} disabled=${ro} title="A new seed for the selected piece" />
        <${Button} text="All of this tile" run=${() => act.variants('tile')} disabled=${ro} title="A new seed for every piece of this tile, each its own" />
        <${Button} text="Whole board" run=${() => act.variants('all')} disabled=${ro} />
      </div>
    <//>
    <${Section}>
      <div class="mp-row wrap">
        ${many ? null : html`<${Button} text="Open tile" run=${() => act.openTile(p.id)} cls="btn on" disabled=${!p.tileId} title="The tile itself, in the tile editor: the map is one click back" />`}
        <${Button} text="Duplicate" run=${() => act.duplicate(ids)} disabled=${ro} title="A copy next to it, with the same seed (D)" />
        <${Button} text="Remove" run=${() => act.remove(ids)} cls="btn danger" disabled=${ro} title="Take it off the map (Backspace)" />
      </div>
    <//>`;
}

function PropLevel({ m, act }) {
  const ro = m.readOnly.value;
  const p = m.selected.value[0];
  const sc = m.scene.value;
  const kind = m.mprop.value;
  const inv = m.inventory.value;
  if (kind === 'bottom') {
    const choose = (name, text, sub) => {
      const on = (p.bottom ?? null) === name;
      return html`<button type="button" key=${name ?? 'ground'} class=${`mp-choice${on ? ' on' : ''}`} aria-pressed=${String(on)} disabled=${ro} onClick=${() => act.setPiece(p.id, { bottom: name }, `${p.name}: ${text}`)}><b>${text}</b><span class="mp-sub">${sub}</span></button>`;
    };
    return html`<${Section} title="Bottom">
      <p class="note">How high the tile’s zero (its ground level) stands above the map’s ground. At zero the connector is under the ground; a raised piece hangs its connector in the air, and a riser under it comes later.</p>
      ${choose(null, 'Ground level', 'zero')}
      ${(sc?.heights ?? []).filter((h) => /Height$/.test(h.name)).map((n) => choose(`$${n.name}`, `$${n.name}`, `${Number(n.dmm.toFixed(2))} dmm`))}
      <div class="pl-field"><span class="pl-lab">A length</span>
        <${Field} type="text" placeholder="20dmm" spellcheck=${false} disabled=${ro} aria-label="A bottom as a length with units"
          value=${p.bottom && !p.bottom.startsWith('$') ? p.bottom : ''}
          onCommit=${(v) => { if (v.trim()) act.setPiece(p.id, { bottom: v.trim() }, `${p.name}: ${v.trim()}`); }} /></div>
    <//>`;
  }
  if (kind === 'seed') {
    const mine = p.ownedSeeds ?? [];
    const h = inv ? tileHave(inv, p.canon, p.system) : null;
    return html`<${Section} title="Seed">
      <p class="note">A seed is a variant of the tile. Pick one you own to make this piece the same as one you have; otherwise it is a new print.</p>
      ${mine.length
        ? html`<p class="inv-have">${h ? haveLine(h) : ''}</p>
          <div class="mp-chips">${mine.map((o) => html`<button type="button" key=${o.seed} class=${`mp-chip${o.seed === p.seedUsed ? ' on' : ''}`} title=${`Same as the ${o.have} you have: this piece is on your shelf`} aria-pressed=${String(o.seed === p.seedUsed)} disabled=${ro}
            onClick=${() => act.setPiece(p.id, { seed: o.seed }, `${p.name}: seed ${o.seed}, same as one you have`)}>${`${o.seed}${o.have > 1 ? ` ×${o.have}` : ''}`}</button>`)}</div>`
        : html`<p class="note">You own none of this tile in this system and colour mode.</p>`}
      <div class="pl-field"><span class="pl-lab">Seed</span>
        <${Field} type="number" min="0" step="1" disabled=${ro} aria-label="Seed" value=${String(p.seedUsed)}
          onCommit=${(v) => { if (v !== '') act.setPiece(p.id, { seed: Number(v) }, `${p.name}: seed ${v}`); }} /></div>
      <div class="mp-row wrap">
        <${Button} text="New variant" run=${() => act.variants('piece')} disabled=${ro} />
        <${Button} text="The tile’s own seed" run=${() => act.setPiece(p.id, { seed: null }, `${p.name}: the tile’s own seed`)} disabled=${ro || p.seed == null} />
      </div>
    <//>`;
  }
  return html`<${Section} title="Print"><//>`;
}

export function MapPanel({ m, act }) {
  if (!m.tile.value || !m.on.value) return null; // not the map's turn: the panel draws nothing
  const scene = m.scene.value;
  const level = m.level.value;
  return html`<${Head} m=${m} act=${act} />
    <div class="body">
      ${!scene
        ? html`<div class="empty">${m.waiting.value}</div>`
        : html`${m.readOnly.value ? html`<p class="note">This file cannot be changed here.</p>` : null}
          ${level === 'map' ? html`<${MapLevel} m=${m} act=${act} />` : level === 'piece' ? html`<${PieceLevel} m=${m} act=${act} />` : html`<${PropLevel} m=${m} act=${act} />`}`}
    </div>`;
}
