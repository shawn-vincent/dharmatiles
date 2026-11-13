// @ts-check
// Changing a map (docs/maps.html): placing, moving, turning, raising, duplicating and removing pieces, new
// variants, and a piece's words.  Every one is an action on the map's document (js/documents.js): its `build`
// runs when the action's turn comes, looks the pieces up by id in the state it is given, and answers the calls
// to make; each call is an edit of the map's text made by the program (`/api/maps/*`) against the revision the
// page held when it was made.  The change joins the one history (title bar Undo and Redo, ⌘Z), and the toast's
// Undo is that history's, offered only while the change is still the last.

import * as M from '../map-model.js';
import { piecesOf } from '../doc-views.js';
import { toast } from './app.js';
import { docs, planOf, undoToast } from './docs.js';
import { isMapView, server, tile, view } from './stores.js';
import { pieceAt, sceneNow, selected, select } from './scene.js';
import { steps as stepsOf } from '../steps.js';

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const listId = () => tile()?.id;
const asNote = (what, n) => `${what} ${plural(n, 'piece')}`;

/** The pieces of the map in `state`, as that state has them. */
const piecesIn = (state) => piecesOf(state?.plan?.scene ?? null, server.get().tiles);
/** The pieces `ids` names in `state` (one that is not there any more is not returned). */
const lookup = (state, ids) => {
  const by = new Map(piecesIn(state).map((p) => [p.id, p]));
  return ids.map((id) => by.get(id)).filter(Boolean);
};

/**
 * Make a change to the map in view: `build(state)` is called when its turn comes (after any change still on its
 * way) with the map's state as it is then, and answers `{label, note, steps}` or null.  The toast says `note`.
 */
async function change(build) {
  const id = listId();
  if (!id) return null;
  const r = await docs.edit(id, build);
  if (!r) return null;
  if (r.note) toast(r.note, () => undoToast(id, r.entry));
  return r.last;
}

const batch = (edits) => stepsOf.mapsBatch({ edits });

/** Set words of the pieces named in `edits` (each `{id, set}`) in one change; `note` says what was done. */
export function setWords(label, note, edits) {
  if (!edits.length) return null;
  return change(() => ({ label, note, steps: [batch(edits)] }));
}

/** A piece's own words. */
export const setPiece = (id, set, note) => change((state) => {
  const [p] = lookup(state, [id]);
  if (!p) return null;
  return { label: 'Could not change the piece', note: note ?? `Changed ${p.name}`, steps: [batch([{ id, set }])] };
});

/**
 * Turn pieces a quarter (`dir` 1 clockwise, -1 counter-clockwise).  One piece goes to the nearest free place its new
 * footprint fits, which the program settles in the write; several turn where they stand.
 */
export function turn(ids, dir) {
  return change((state) => {
    const ps = lookup(state, ids).filter((p) => p.at);
    const edits = ps.map((p) => {
      const rot = M.turned(p.rot, dir);
      const sq = M.snapSquares(p, p.at[0], p.at[1]);
      return { id: p.id, set: ps.length === 1 ? { rot, near: sq } : { rot, at: M.atWord(sq.x, sq.y) } };
    });
    if (!edits.length) return null;
    const what = ps.length === 1 ? `Turned ${ps[0].name}` : asNote('Turned', ps.length);
    return { label: 'Could not turn the piece', note: (a) => (a[0]?.shifted ? `${what}; it moved to the nearest free place` : what), steps: [batch(edits)] };
  });
}

/** Put pieces down at places (squares of each piece), `moves`: [{id, x, y, rot?}]; one piece's place taken is the nearest free one (the program settles it). */
export function moveTo(moves, note) {
  return change((state) => {
    const ps = piecesIn(state);
    const ms = moves.map((m) => ({ ...m, p: ps.find((q) => q.id === m.id) })).filter((m) => m.p);
    const edits = ms.map((m) => ({
      id: m.id,
      set: { ...(ms.length === 1 ? { near: { x: m.x, y: m.y } } : { at: M.atWord(m.x, m.y) }), ...(m.rot != null ? { rot: m.rot } : {}) },
    }));
    if (!edits.length) return null;
    const what = note ?? (ms.length === 1 ? `Moved ${ms[0].p.name}` : asNote('Moved', ms.length));
    return { label: 'Could not move the piece', note: (a) => (a[0]?.shifted ? `${what} to the nearest free place` : what), steps: [batch(edits)] };
  });
}

/** A copy of each piece, put next to it in the nearest free place (settled by the program, each after the last); it starts with the same seed. */
export function duplicate(ids) {
  return change((state) => {
    const ps = lookup(state, ids).filter((p) => p.at);
    const steps = ps.map((p) => {
      const sq = M.snapSquares(p, p.at[0], p.at[1]);
      return stepsOf.mapsDup({ id: p.id, near: { x: sq.x + Math.round(p.size[0] / p.sq), y: sq.y } });
    });
    if (!steps.length) return null;
    return { label: 'Could not duplicate the piece', note: ps.length === 1 ? `Copied ${ps[0].name}` : asNote('Copied', ps.length), steps };
  });
}

/** Take pieces off the map (each by its id: the ids the rest keep stay true while it goes). */
export function remove(ids) {
  select([]);
  return change((state) => {
    const ps = lookup(state, ids);
    if (!ps.length) return null;
    return {
      label: 'Could not take the piece out',
      note: ps.length === 1 ? `Took ${ps[0].name} out` : asNote('Took out', ps.length),
      steps: ps.map((p) => stepsOf.mapsRemove({ id: p.id })),
    };
  });
}

/**
 * Tray pieces (no place yet) go on the map at the nearest free places to the corner: one copy, or `all` of a
 * line's copies.  A line with one copy is given its place; from a line with several, each copy placed is a new
 * piece line with a seed of its own (one you do not own, not on the map), and the tray line's count goes down.
 * The program settles every place and seed in its own write.  One change, one Undo.
 */
export function place(id, all = false) {
  return change((state) => {
    const [p] = lookup(state, [id]);
    if (!p) return null;
    const n = all ? p.count : 1;
    const corner = { x: 0, y: 0 };
    if (p.count <= 1) {
      return { label: 'Could not place the piece', note: `Placed ${p.name}`, steps: [stepsOf.mapsSet({ id, set: { near: corner, rot: p.rot ?? 0 } })] };
    }
    const left = p.count - n;
    // the tray line keeps the copies not placed; when none are left it becomes the last one placed, so its seed stays
    /** @type {any[]} */
    const steps = [stepsOf.listsCount({ id, count: Math.max(left, 1) })];
    const sys = sceneIn(state)?.system;
    for (let k = 0; k < (left > 0 ? n : n - 1); k++) {
      steps.push(stepsOf.mapsAdd({ tile: p.tileId, near: corner, rot: 0, seedWord: 'fresh', ...(p.system && p.system !== sys ? { system: p.system } : {}) }));
    }
    if (left === 0) steps.push(stepsOf.mapsSet({ id, set: { near: corner, rot: p.rot ?? 0 } }));
    return { label: 'Could not place the piece', note: n === 1 ? `Placed ${p.name}` : `Placed ${n} × ${p.name}, each with a seed of its own`, steps };
  });
}

const sceneIn = (state) => state?.plan?.scene ?? null;

// ---- seeds and variants -----------------------------------------------------------------------------

/**
 * New variants: which pieces (`scope`): `piece` (the selected ones), `tile` (every piece of the selected tiles),
 * `all` (the whole board), `missing` (every piece you do not own: what you have stays as it is).  Each gets a
 * seed of its own; each is built (low first) and swaps in.
 */
export function variants(scope) {
  return change((state) => {
    const ps = piecesIn(state);
    const all = ps.filter((p) => p.at && !p.error);
    const sel = selected();
    let who;
    if (scope === 'piece') who = sel;
    else if (scope === 'tile') who = all.filter((p) => sel.some((s) => s.canon === p.canon));
    else if (scope === 'missing') {
      who = all.filter((p) => ['need', 'other'].includes(p.shelf?.state));
    } else who = all;
    who = who.filter((p) => p.print !== 'skip' || scope !== 'missing');
    if (!who.length) { toast(scope === 'missing' ? 'Every piece on the map is on your shelf: nothing to change' : 'No pieces to change'); return null; }
    // a seed nobody owns, that the map does not use and no other piece of this change has: the program settles each in its write
    const edits = who.map((p) => ({ id: p.id, set: { freshSeed: true } }));
    const note = scope === 'piece' ? `New variant of ${who[0].name}` : scope === 'tile' ? `New variants of ${plural(who.length, 'piece')} of ${who[0].name}` : scope === 'missing' ? `New seed for ${plural(who.length, 'piece')} you don't have` : `New variants of ${plural(who.length, 'piece')}`;
    return { label: 'Could not make the variants', note, steps: [batch(edits)] };
  });
}

/**
 * A tile from the catalog goes on the map: at `want` (squares of its own system, from a drop) or the nearest free
 * place to the map's corner (the program settles the place).  It gets a seed you do not own, unless you own none of it
 * and the map does not use it: then the tile's own seed is already new (and is not written).
 */
export async function addTile(t, want = null) {
  if (!isMapView()) return null;
  return change((state) => {
    if (!sceneIn(state)) return null;
    return {
      label: 'Could not add the tile',
      note: (a) => `Put ${t.name} on the map${a[0]?.seed != null ? ` (seed ${a[0].seed}, one you do not have)` : ''}`,
      steps: [stepsOf.mapsAdd({ tile: t.id, near: want ?? { x: 0, y: 0 }, rot: 0, seedWord: 'if-needed' })],
    };
  });
}

/** Take the selected piece's tile into Tile mode, with the map as the place Back returns to. */
export async function openTile(id) {
  const p = pieceAt(id);
  if (!p?.tileId) return;
  const { select: open } = await import('./open.js');
  if ((await open(p.tileId)) !== false) view.set({ pageAt: 'edit' });
}

/** May the map be changed here (a file of yours, not the library's)? */
export const editable = () => { const t = tile(); return !!t && !t.path.startsWith('lib:') && (planOf(t)?.writable ?? true); };

/** The keys: R turns, D duplicates, Backspace or Delete takes out.  (⌘Z is the page's one Undo, editor/document.js.) */
export function onKey(e) {
  if (!isMapView()) return;
  const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';
  if (typing) return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const ids = selected().map((p) => p.id);
  if (!ids.length) return;
  if (e.key === 'r' || e.key === 'R') { e.preventDefault(); turn(ids, e.shiftKey ? -1 : 1); }
  else if (e.key === 'd' || e.key === 'D') { e.preventDefault(); duplicate(ids); }
  else if (e.key === 'Backspace' || e.key === 'Delete') { e.preventDefault(); remove(ids); }
}

export { pieceAt, sceneNow };
