// The map's levels (docs/maps.html): the map (its pieces by tile, what the shelf says of them, the variants and the way
// to the plates), one piece (its place, turn, level, seed and print setting, with Open tile), and a property of it (the
// level's names, the seeds you own).  Where you are is the selection and the property open on it, so a click on a piece
// in the view brings the panel to it and Back climbs.
//
// These are views, drawn each into its section of the levels shell (editor/levels.js; the page's `lvl-<id>`): the
// shell decides which one is on screen and how it comes and goes, so a view here never shows, hides or slides itself and
// never reads which level is on screen.  What a level showed is kept by `held` while it slides out (the piece just
// left), not by the shell.
//
// Each level is a function of the model (`m`, js/editor/model.js `mapPanel`: signals) and of the actions (`act`,
// what the buttons do: each one is a change of the document, editor/scene-edit.js, or of the view).  It draws
// what the model says and keeps nothing: a new state of the map from the program is drawn over the panel as it
// is, so a word being typed is not lost (field.js).

import * as M from '../map-model.js';
import { FILTERS, SORTS, sectionsOf } from '../map-list.js';
import { FolderList } from './places-panel.js';
import { haveLine, systemName, tileHave } from '../inventory.js';
import { Field } from './field.js';
import { html, useEffect, useRef } from './html.js';
import { Icon } from './icons.js';
import { Button, Section, Seg, plural } from './widgets.js';

/** The last non-empty value of `v`: a level leaving the screen keeps what it showed while it slides out. */
function held(v) {
  const last = useRef(v);
  if (v) last.current = v;
  return last.current;
}

/** The have or to-print pill of a piece (caution yellow where only the other colour mode is on the shelf). */
const TITLES = {
  have: 'A print you own is this piece (tile, seed, system and colour mode): it is left off the plates',
  need: 'It prints',
  other: 'You own this tile and seed in the other colour mode; it prints unless you leave it off',
  skip: 'print=skip: it stays on the map and off the plates',
};
export const Pill = ({ state }) => {
  const word = { have: 'have', need: 'to print', other: 'have, other colour', skip: 'left off' }[state] ?? state;
  const title = TITLES[state];
  return html`<span class=${`ipill ${state === 'have' ? 'have' : state === 'other' ? 'caution' : 'need'}`} title=${title}>${word}</span>`;
};

/** The have, to-print and caution pills of a row of the list: what the shelf says of its pieces (the states are apart: a piece is in one). */
const RowPills = ({ row }) => html`<span class="mp-pills">
  ${row.skip ? html`<${Pill} state="skip" />` : null}
  ${row.have ? html`<span class="ipill have" title=${TITLES.have}>${`${row.have} have`}</span>` : null}
  ${row.toPrint - row.other ? html`<span class="ipill need" title=${TITLES.need}>${`${row.toPrint - row.other} to print`}</span>` : null}
  ${row.other ? html`<span class="ipill caution" title=${TITLES.other}>${`${row.other} other colour`}</span>` : null}</span>`;

/**
 * The map's pieces by tile, in the sections placed, not placed and left off (map-list.js), under the sort, the filter
 * and the search the model holds for this map.  A row lights its pieces in the view while the pointer is on it
 * (`act.hover`), selects them when pressed, and is marked, and brought into view in the list, while one of its pieces
 * is selected: so it reads the selection the same wherever the selection was made.  `compact` is the Add tiles level's list: the
 * rows alone (no sort, filter or search), the row of the tile just put on the map lit for a moment and kept in view.
 */
function PieceList({ m, act, compact = false }) {
  const ro = m.readOnly.value;
  const ps = m.pieces.value;
  const how = m.list.value;
  const secs = sectionsOf(ps, m.marks.value, how);
  const picked = new Set(m.selected.value.map((p) => p.id));
  const box = useRef(/** @type {any} */ (null));
  const key = [...picked].join(',');
  const added = compact ? m.added.value : null;
  const fresh = !!added && m.fresh.value;
  // bring the first marked row into view inside the list itself, never moving the panel around it
  useEffect(() => {
    const el = box.current, row = el?.querySelector('.mp-group.on');
    if (!el || !row || el.scrollHeight <= el.clientHeight) return;
    const top = row.offsetTop - el.offsetTop;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + row.offsetHeight > el.scrollTop + el.clientHeight) el.scrollTop = top + row.offsetHeight - el.clientHeight;
  }, [key]);
  // the row of a tile just put down is brought into view in the list above the folders
  useEffect(() => { if (fresh) box.current?.querySelector('.mp-group.just0, .mp-group.just1')?.scrollIntoView({ block: 'nearest' }); }, [fresh, added?.n]);
  const Row = (row) => {
    const on = row.ids.some((id) => picked.has(id));
    const hot = fresh && row.pieces.some((p) => p.tileId === added.tileId);
    return html`<button type="button" class=${`mp-group${on ? ' on' : ''}${hot ? ` just${added.n % 2}` : ''}`} key=${row.key} aria-pressed=${String(on)} title=${`Select the ${plural(row.pieces.length, 'piece')} of ${row.name}`}
      data-ids=${row.ids.join(',')} onClick=${() => act.select(row.ids)} onPointerEnter=${() => act.hover(row.ids)} onPointerLeave=${() => act.hover([])} onFocus=${() => act.hover(row.ids)} onBlur=${() => act.hover([])}>
      <b>${row.name}</b><span class="mp-n">${`× ${row.count}`}</span><${RowPills} row=${row} /></button>`;
  };
  const Head = ({ text, n, note }) => html`<h4 class="cap mp-h" title=${note}>${text}<span class="mp-n">${n}</span></h4>`;
  const Wrap = compact ? ({ children }) => html`<div class="mp-compact"><h4 class="cap mp-h">${ps.length ? 'On the map' : 'Nothing on the map yet'}<span class="mp-n">${ps.length ? plural(ps.filter((p) => p.at).length, 'piece') : ''}</span></h4>${children}</div>`
    : ({ children }) => html`<${Section} title=${ps.length ? 'Pieces' : 'No pieces yet'}>${children}<//>`;
  return html`<${Wrap}>
    ${ps.length ? null : html`<p class="note">${compact ? 'Press a tile below to put it on the map.' : 'Press Add tiles and put tiles on the map.'} Each is a piece with a place, a turn and a seed; nothing prints until you ask for the plates.</p>`}
    ${ps.length ? html`
      ${compact ? null : html`<${Seg} label="Sort" options=${SORTS} current=${how.sort} on=${(v) => act.list({ sort: v })} />
      <${Seg} label="Show" options=${FILTERS} current=${how.filter} on=${(v) => act.list({ filter: v })} />
      <div class="pl-field"><span class="pl-lab">Find</span>
        <${Field} type="search" commit="input" placeholder="Part of a tile’s name" spellcheck=${false} aria-label="Find a tile on the map by name" value=${how.query} onCommit=${(v) => act.list({ query: v })} /></div>`}
      <div class="mp-list" ref=${box}>
        ${secs.shown ? null : html`<p class="note">No piece matches.</p>`}
        ${secs.placed.length ? html`<${Head} text="Placed" n=${secs.placed.reduce((n, r) => n + r.count, 0)} />${secs.placed.map((r) => Row(r))}` : null}
        ${secs.tray.length ? html`<${Head} text="Not placed" n=${secs.tray.reduce((n, r) => n + r.count, 0)} note="Pieces with no place yet: they print, and wait to be put on the map" />${secs.tray.map((r) => html`<div class="mp-tray" key=${r.key}>
          <b>${r.name}</b><span class="mp-n">${r.count > 1 ? `× ${r.count}` : ''}</span>
          <${Button} text="Place" run=${() => act.place(r.key)} disabled=${ro || !!r.pieces[0].error} title=${r.count > 1 ? 'Put one copy on the map at the nearest free place, with a seed of its own' : 'Put it on the map at the nearest free place'} />
          ${r.count > 1 ? html`<${Button} text="Place all" run=${() => act.place(r.key, true)} disabled=${ro || !!r.pieces[0].error} title=${`Put all ${r.count} on the map, each with a seed of its own`} />` : null}
        </div>`)}` : null}
        ${secs.off.length ? html`<${Head} text="Left off" n=${secs.off.reduce((n, r) => n + r.count, 0)} note="print=skip: these stay on the map and off the plates" />${secs.off.map((r) => Row(r))}` : null}
      </div>` : null}
  <//>`;
}

function MapView({ m, act }) {
  const ro = m.readOnly.value;
  const ps = m.pieces.value;
  const sc = m.scene.value;
  const placed = ps.filter((p) => p.at);
  return html`
    <${Section} title="On the map">
      <p class="inv-sum"><b>${sc?.shelfExists ? `${sc.have} in inventory` : 'No inventory yet'}</b>${` · ${sc?.toPrint ?? 0} to print`}</p>
      <div class="mp-row">
        <${Button} text="Add tiles" run=${act.openAdd} title="Choose tiles from your folders and the library: press one to put it on the map, or drag it to where it should stand" disabled=${ro} />
        <${Button} text="Print what’s missing" run=${() => act.chooseMode('plates')} cls="btn on" title="The plates for this map: only the pieces your inventory lacks" />
      </div>
    <//>
    <${PieceList} m=${m} act=${act} />
    <${Section} title="Variants">
      <p class="note">A new variant is the same tile with a new seed: a new print. New pieces already get a seed you do not own.</p>
      <div class="mp-row wrap">
        <${Button} text="New seed for every piece I don’t have" run=${() => act.variants('missing')} disabled=${ro || !placed.length} title="Each piece not on your shelf gets a fresh variant; what you own stays as it is" />
        <${Button} text="Whole board" run=${() => act.variants('all')} disabled=${ro || !placed.length} title="A new variant of every piece" />
      </div>
    <//>`;
}

function PieceView({ m, act, sel }) {
  const ro = m.readOnly.value;
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
      ${many ? null : html`<div class="pl-field"><span class="pl-lab">Place</span><span class="mp-val">${p.at ? M.placeWords(p) : 'in the tray'}</span></div>`}
      <div class="pl-field"><span class="pl-lab">Turn</span>
        <span class="mp-btns">
          <${Button} text=${html`<${Icon} name="rotate-ccw" size=${14} />`} run=${() => act.turn(ids, -1)} cls="pl-step" title="A quarter turn counter-clockwise (⇧R)" disabled=${ro} />
          <${Button} text=${html`<${Icon} name="rotate-cw" size=${14} />`} run=${() => act.turn(ids, 1)} cls="pl-step" title="A quarter turn clockwise (R)" disabled=${ro} />
          ${many ? null : html`<span class="mp-val">${`${p.rot}°`}</span>`}
        </span></div>
      ${many ? null : html`
        <button type="button" class="mp-prop" title="How high the tile’s zero (its ground level) stands above the map’s ground: a name from the standard heights, or a length. The connector hangs below it." onClick=${() => act.setProp('bottom')}>
          <span class="pl-lab">Bottom</span><span class="mp-val">${M.bottomWords(p, sc?.heights)}</span><span class="mp-go"><${Icon} name="chevron-right" size=${14} /></span></button>
        <button type="button" class="mp-prop" title="Which variant of the tile: a new one, or one you already have" onClick=${() => act.setProp('seed')}>
          <span class="pl-lab">Seed</span><span class="mp-val">${String(p.seedUsed)}</span><span class="mp-go"><${Icon} name="chevron-right" size=${14} /></span></button>
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

function PropView({ m, act, p, kind }) {
  const ro = m.readOnly.value;
  const sc = m.scene.value;
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

/**
 * The Add step's level, for one folder: above, the piece list, compact (the row of a tile just put down lights up; a row leaves Add for that
 * piece's level), and below, the folder's own list in its add mode (folders as rows, tiles as pictures).  The path to it is the panel's one
 * trail and the step to another folder is the shell's slide between this section and the other one (`slot`): this draws a folder, and keeps
 * it while it slides out.  On a short panel the list folds to one line (the style sheet does it by the panel's height; both are in the page).
 */
function AddView({ m, act, slot }) {
  const live = m.add.value;
  const a = held(live && live.slot === slot ? live : null);
  const placed = m.pieces.value.filter((p) => p.at).length;
  const added = m.added.value;
  if (!a) return null;
  return html`<div class="addlvl">
    <div class="add-up"><${PieceList} m=${m} act=${act} compact=${true} /></div>
    <div class="add-fold" aria-live="polite">${`${placed} on the map`}${added ? ` · ${added.name} ${m.fresh.value ? 'just added' : 'added last'}` : ''}</div>
    <div class="add-low">
      ${a.level.root == null ? null : html`<div class="add-filter"><${Field} type="search" commit="input" placeholder="Filter tiles" spellcheck=${false} aria-label="Filter the tiles of this place" value=${a.query} onCommit=${act.addFilter} /></div>`}
      <div class="add-body"><div class="flvl" role="listbox" aria-label="Tiles to add" tabindex="-1"><${FolderList} d=${a} on=${act.addOn} /></div></div>
    </div>
  </div>`;
}

/** What every level of the map draws inside its section: nothing when it is not the map's turn, the wait while the scene has not come, else the level. */
function Body({ m, children, bare = false }) {
  if (!m.tile.value || !m.on.value) return null;
  if (bare) return !m.scene.value ? html`<div class="body"><div class="empty">${m.waiting.value}</div></div>` : children;
  return html`<div class="body">
      ${!m.scene.value
        ? html`<div class="empty">${m.waiting.value}</div>`
        : html`${m.readOnly.value ? html`<p class="note">This file cannot be changed here.</p>` : null}${children}`}
    </div>`;
}

/** The map level. */
export const MapLevel = ({ m, act }) => html`<${Body} m=${m}><${MapView} m=${m} act=${act} /><//>`;

/** The piece level: the selected piece, or the last one while the level slides out. */
export function PieceLevel({ m, act }) {
  const sel = held(m.selected.value.length ? m.selected.value : null);
  return html`<${Body} m=${m}>${sel ? html`<${PieceView} m=${m} act=${act} sel=${sel} />` : null}<//>`;
}

/** The Add step's level in section `slot` (0 or 1: nav.js `ADD_PANES`): nothing to add to on a map that cannot be changed here. */
export function AddLevel({ m, act, slot }) {
  return html`<${Body} m=${m} bare=${true}>${m.readOnly.value ? html`<div class="body"><p class="note">This file cannot be changed here, so no tile can be added to it.</p></div>` : html`<${AddView} m=${m} act=${act} slot=${slot} />`}<//>`;
}

/** The property level: the property open on the selected piece, or the last one while the level slides out. */
export function PropLevel({ m, act }) {
  const sel = held(m.selected.value.length ? m.selected.value : null);
  const kind = held(m.mprop.value);
  return html`<${Body} m=${m}>${sel && kind ? html`<${PropView} m=${m} act=${act} p=${sel[0]} kind=${kind} />` : null}<//>`;
}
