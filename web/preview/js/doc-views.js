// What the page shows of a document, as functions of the document's state: the pieces of a map, the selection,
// the printer the title bar's chip names, the words of the status line.  Each is worked out when it is asked
// for, from the state it is given, and kept by nothing; the identity-keyed memo below only saves repeating the
// same work on the same objects.  No DOM, no stores: tests/documents.test.js compares each with a fresh working-out.

import * as M from './map-model.js';

const SYSTEMS = { db: 'DungeonBlocks', ol: 'OpenLOCK' };

/** The key of the geometry a piece is drawn from: its tile, seed, system and printer, and the tile's build. */
const gkeyOf = (p, scene, tiles) => {
  const t = tiles.find((x) => x.id === p.tileId);
  return `${p.tileId}|${p.seed ?? ''}|${p.system}|${scene.printer}|${t?.meshVersion ?? 0}`;
};

let memo = { scene: null, tiles: null, out: [] };
/** The pieces of `scene`, each with its turn and the key of its geometry; `tiles` is the program's list (the build each piece's tile is at). */
export function piecesOf(scene, tiles) {
  if (!scene) return [];
  if (memo.scene === scene && memo.tiles === tiles) return memo.out;
  const out = scene.pieces.map((p) => ({ ...p, rot: p.rot ?? 0, gkey: p.tileId && !p.error ? gkeyOf(p, scene, tiles) : null }));
  memo = { scene, tiles, out };
  return out;
}

/** The pieces whose ids are in `ids` (a selection), in the order they are on the map: a piece that is gone is not selected. */
export const selectedOf = (ids, pieces) => {
  const want = new Set(ids);
  return pieces.filter((p) => want.has(p.id));
};

/** The printer a map or print list names: `{name, label, what}`, or null where the document names none (the base target stands). */
export function printerOf(state, printers, { map = false } = {}) {
  const name = state?.plan?.scene?.printer ?? state?.plan?.plates?.printer ?? null;
  if (!name) return null;
  const known = printers.find((p) => p.name === name);
  return { name, label: known?.label ?? state?.plan?.plates?.label ?? name, what: map ? 'map' : 'print list' };
}

/** The status line's words for the target: the document's printer on a list or a map, else system and printer. */
export function targetWordsOf(doc, { system, printer }) {
  return doc ? `${doc.label} · this ${doc.what}` : `${SYSTEMS[system] || system} · ${printer}`;
}

/** The title bar's chip for a map: `3 in inventory · 11 to print`. */
export const chipOf = (pieces, inventory) => M.totalsText(pieces, inventory);

/** The status line's verdict on a map: how many pieces, and what they come to. */
export const verdictOf = (pieces, inventory) => {
  const placed = pieces.filter((p) => p.at);
  return `${placed.length} piece${placed.length === 1 ? '' : 's'} · ${M.totalsText(pieces, inventory)}`;
};
