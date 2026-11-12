// Changing a map (docs/maps.html): placing, moving, turning, raising, duplicating and removing pieces, new
// variants, and a piece's words.  Every one is an edit of the map's text made by the program (`/api/maps/*`,
// which keeps every other byte); each answers with the text it replaced (`before`) and with the map as it now
// reads, so the view changes at once and the toast's Undo (or ⌘Z) writes the old text back, as the print
// lists' edits do (editor/lists.js).

import { failed, showError } from '../problems.js';
import { canonOf } from '../inventory.js';
import { sizeSquares } from '../folders.js';
import * as M from '../map-model.js';
import { toast, host } from './app.js';
import { say } from './inventory.js';
import { isMapView, server, tile, view } from './stores.js';
import { pieceAt, pieces, sceneNow, selected, takeScene, select } from './scene.js';

/** The edits made, newest last: [{list, before}], for ⌘Z. */
const undone = [];

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** One call: the answer's body, or null after its refusal is shown (a mistake in a document, in words, at a toast). */
async function call(label, run) {
  try {
    const r = await run();
    if (r.status !== 200) {
      if (r.status !== 0 && !failed(label, r)) toast(`${label}: ${r.body?.error ?? 'refused'}`, null, { ms: 9000 });
      return null;
    }
    if (r.body.ok === false) { say(r.body, label); return null; }
    return r.body;
  } catch (e) { showError(label, e); return null; }
}

/** The text a change replaced is written back (Undo). */
export async function restore(list, before) {
  const r = await call('Could not undo', () => host.listsRestore({ list, text: before }));
  if (r?.scene) takeScene(r.scene);
}

/** ⌘Z on the map: the last change taken back.  False when there is none. */
export async function undoLast() {
  const u = undone.pop();
  if (!u) return false;
  await restore(u.list, u.before);
  return true;
}

/**
 * Make the changes `steps` (each a function returning the call's promise) as one change: the toast says
 * `note`, and its Undo goes back to the text before the first.
 */
async function change(label, note, steps) {
  let first = null, last = null;
  for (const step of steps) {
    const b = await call(label, step);
    if (!b) break;
    first ??= b;
    last = b;
  }
  if (!last) return null;
  takeScene(last.scene);
  undone.push({ list: first.list, before: first.before });
  toast(note, () => { undone.pop(); restore(first.list, first.before); });
  return last;
}

const listId = () => tile()?.id;
const asNote = (what, n) => `${what} ${plural(n, 'piece')}`;

/** Set words of the pieces `ps` (each `{index, set}`) in one change. */
export function setWords(label, note, edits) {
  if (!edits.length) return null;
  return change(label, note, [() => host.mapsBatch({ list: listId(), edits })]);
}

/** A piece's own words. */
export const setPiece = (p, set, note) => setWords('Could not change the piece', note ?? `Changed ${p.name}`, [{ index: p.index, set }]);

/** Turn pieces a quarter (`dir` 1 clockwise, -1 counter-clockwise), each to the nearest free place its new footprint fits. */
export function turn(ps, dir) {
  const others = pieces().filter((q) => q.at && !ps.includes(q));
  const edits = [];
  let moved = 0;
  for (const p of ps) {
    if (!p.at) continue;
    const rot = M.turned(p.rot, dir);
    const sq = M.snapSquares(p, p.at[0], p.at[1]);
    const free = ps.length === 1 ? M.nearestFree(others, p, sq, rot) : { ...sq, moved: 0 };
    if (free.moved > 0) moved += 1;
    edits.push({ index: p.index, set: { rot, at: M.atWord(free.x, free.y) } });
  }
  const what = ps.length === 1 ? `Turned ${ps[0].name}` : asNote('Turned', ps.length);
  return setWords('Could not turn the piece', moved ? `${what}; it moved to the nearest free place` : what, edits);
}

/** Put pieces down at places (squares of each piece), `moves`: [{p, x, y, rot?}]; a place taken is the nearest free one. */
export function moveTo(moves, note) {
  const stay = pieces().filter((q) => q.at && !moves.some((m) => m.p === q));
  const edits = [];
  let shifted = 0;
  for (const m of moves) {
    const rot = m.rot ?? m.p.rot;
    const free = moves.length === 1 ? M.nearestFree(stay, m.p, { x: m.x, y: m.y }, rot) : { x: m.x, y: m.y, moved: 0 };
    if (free.moved > 0) shifted += 1;
    if (free.moved < 0) { toast('There is no free place for that piece on the map'); return null; }
    edits.push({ index: m.p.index, set: { at: M.atWord(free.x, free.y), ...(m.rot != null ? { rot: m.rot } : {}) } });
  }
  const what = note ?? (moves.length === 1 ? `Moved ${moves[0].p.name}` : asNote('Moved', moves.length));
  return setWords('Could not move the piece', shifted ? `${what} to the nearest free place` : what, edits);
}

/** A copy of each piece, put next to it in the nearest free place; it starts with the same seed. */
export function duplicate(ps) {
  let taken = pieces().filter((q) => q.at);
  const steps = [];
  for (const p of ps) {
    if (!p.at) continue;
    const sq = M.snapSquares(p, p.at[0], p.at[1]);
    const free = M.nearestFree(taken, p, { x: sq.x + Math.round(p.size[0] / p.sq), y: sq.y });
    taken = [...taken, { ...p, at: [free.x * p.sq, free.y * p.sq] }];
    steps.push(() => host.mapsDup({ list: listId(), index: p.index, at: M.atWord(free.x, free.y) }));
  }
  return change('Could not duplicate the piece', ps.length === 1 ? `Copied ${ps[0].name}` : asNote('Copied', ps.length), steps);
}

/** Take pieces off the map (last first, so the numbers the rest keep stay true while it goes). */
export function remove(ps) {
  const order = [...ps].sort((a, b) => b.index - a.index);
  select([]);
  return change('Could not take the piece out', ps.length === 1 ? `Took ${ps[0].name} out` : asNote('Took out', ps.length), order.map((p) => () => host.mapsRemove({ list: listId(), index: p.index })));
}

/** A tray piece (no place yet) goes on the map at the nearest free place to the corner. */
export function place(p) {
  const free = M.nearestFree(pieces().filter((q) => q.at), p, { x: 0, y: 0 });
  return setPiece(p, { at: M.atWord(free.x, free.y), rot: p.rot ?? 0 }, `Placed ${p.name}`);
}

// ---- seeds and variants -----------------------------------------------------------------------------

/** The seeds on the map for a piece's tile in its system. */
const usedSeeds = (p) => new Set(pieces().filter((q) => q.canon === p.canon && q.system === p.system).map((q) => q.seed_used));

/** A seed for a new piece of `p`'s tile: one you do not own, that the map does not use and that is not the tile's own. */
export function newSeedFor(p, extraUsed = new Set()) {
  const owned = new Set(M.ownedSeeds(server.get().inventory, p).map((s) => s.seed));
  const used = new Set([...usedSeeds(p), ...extraUsed]);
  return M.freshSeed(owned, used, p.seed_used);
}

/**
 * New variants: which pieces (`scope`): `piece` (the selected ones), `tile` (every piece of the selected tiles),
 * `all` (the whole board), `missing` (every piece you do not own: what you have stays as it is).  Each gets a
 * seed of its own; each is built (low first) and swaps in.
 */
export function variants(scope) {
  const all = pieces().filter((p) => p.at && !p.error);
  const sel = selected();
  let who;
  if (scope === 'piece') who = sel;
  else if (scope === 'tile') who = all.filter((p) => sel.some((s) => s.canon === p.canon));
  else if (scope === 'missing') {
    const m = M.pills(pieces(), server.get().inventory);
    who = all.filter((p) => ['need', 'other'].includes(m.get(p.index)?.state));
  } else who = all;
  who = who.filter((p) => p.print !== 'skip' || scope !== 'missing');
  if (!who.length) { toast(scope === 'missing' ? 'Every piece on the map is on your shelf: nothing to change' : 'No pieces to change'); return null; }
  const fresh = new Map(); // tile+system -> seeds handed out now
  const edits = who.map((p) => {
    const k = `${p.canon}|${p.system}`;
    const got = fresh.get(k) ?? new Set();
    const seed = newSeedFor(p, got);
    got.add(seed);
    fresh.set(k, got);
    return { index: p.index, set: { seed } };
  });
  const note = scope === 'piece' ? `New variant of ${who[0].name}` : scope === 'tile' ? `New variants of ${plural(who.length, 'piece')} of ${who[0].name}` : scope === 'missing' ? `New seed for ${plural(who.length, 'piece')} you don't have` : `New variants of ${plural(who.length, 'piece')}`;
  return setWords('Could not make the variants', note, edits);
}

/**
 * A tile from the catalog goes on the map: at `want` (squares of its own system, from a drop) or the nearest free
 * place to the map's corner.  It gets a seed you do not own, unless you own none of it and the map does not
 * use it: then the tile's own seed is already new (and is not written).
 */
export async function addTile(t, want = null) {
  const sc = sceneNow();
  if (!sc || !isMapView()) return null;
  const system = sc.system;
  const sq = system === 'ol' ? 25.4 : 35;
  const [cols, rows] = sizeSquares(t) ?? [1, 1];
  const inv = server.get().inventory;
  const canon = canonOf(t.path, inv?.home);
  const probe = { canon, system, colour: sc.colour, size: [cols * sq, rows * sq], sq, at: null, rot: 0, height_mm: 0, seed_used: -1 };
  const placed = pieces().filter((q) => q.at);
  const free = M.nearestFree(placed, probe, want ?? { x: 0, y: 0 });
  if (free.moved < 0) { toast('There is no free place for that tile on the map'); return null; }
  const owned = M.ownedSeeds(inv, probe);
  const onMap = placed.find((q) => q.canon === canon && q.system === system);
  let seed = null;
  if (owned.length || onMap) {
    const used = new Set(placed.filter((q) => q.canon === canon && q.system === system).map((q) => q.seed_used));
    seed = M.freshSeed(new Set(owned.map((o) => o.seed)), used, onMap?.seed_used ?? -1);
  }
  return change('Could not add the tile', `Put ${t.name} on the map${seed != null ? ` (seed ${seed}, one you do not have)` : ''}`, [
    () => host.mapsAdd({ list: listId(), tile: t.id, at: M.atWord(free.x, free.y), rot: 0, ...(seed != null ? { seed } : {}) }),
  ]);
}

/** Take the selected pieces' tile into Tile mode, with the map as the place Back returns to. */
export async function openTile(p) {
  if (!p.tile_id) return;
  const { select: open } = await import('./open.js');
  if ((await open(p.tile_id)) !== false) view.set({ page: 'edit' });
}

/** May the map be changed here (a file of yours, not the library's)? */
export const editable = () => { const t = tile(); return !!t && !t.path.startsWith('lib:') && (t.plan?.writable ?? true); };

/** The keys: R turns, D duplicates, Backspace or Delete takes out, ⌘Z undoes the map's last change. */
export function onKey(e) {
  if (!isMapView()) return;
  const typing = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.isContentEditable || e.target.tagName === 'SELECT';
  if (typing) return;
  if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'z' && undone.length) {
    e.preventDefault(); e.stopImmediatePropagation(); undoLast(); return;
  }
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const ps = selected();
  if (!ps.length) return;
  if (e.key === 'r' || e.key === 'R') { e.preventDefault(); turn(ps, e.shiftKey ? -1 : 1); }
  else if (e.key === 'd' || e.key === 'D') { e.preventDefault(); duplicate(ps); }
  else if (e.key === 'Backspace' || e.key === 'Delete') { e.preventDefault(); remove(ps); }
  else if (e.key === 'Escape') select([]);
}

export { pieceAt, sceneNow };
