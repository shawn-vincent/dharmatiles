// @ts-check
// What the page shows of a document, as functions of the document's state: the pieces of a map, the selection,
// the printer the title bar's chip names, the words of the status line.  Each is worked out when it is asked
// for, from the state it is given, and kept by nothing; the identity-keyed memo below only saves repeating the
// same work on the same objects.  No DOM, no stores: tests/documents.test.js compares each with a fresh working-out.

/** @typedef {import('./api').SceneJson} SceneJson */
/** @typedef {import('./api').ScenePiece} ScenePiece */
/** @typedef {import('./editor/stores.js').ServerTile} Tile */
/** @typedef {ScenePiece & { rot: number, gkey: string | null }} Piece  a piece of the scene, with its turn and the key of its geometry */

/** @type {Record<string, string>} */
const SYSTEMS = { db: 'DungeonBlocks', ol: 'OpenLOCK' };

/** The key of the geometry a piece is drawn from: its tile, seed, system and printer, and the tile's build. */
const gkeyOf = (/** @type {ScenePiece} */ p, /** @type {SceneJson} */ scene, /** @type {Tile[]} */ tiles) => {
  const t = tiles.find((x) => x.id === p.tileId);
  return `${p.tileId}|${p.seed ?? ''}|${p.system}|${scene.printer}|${t?.meshVersion ?? 0}`;
};

/** @type {{ scene: SceneJson | null, tiles: Tile[] | null, out: Piece[] }} */
let memo = { scene: null, tiles: null, out: [] };
/** The pieces of `scene`, each with its turn and the key of its geometry; `tiles` is the program's list (the build each piece's tile is at). */
/** @param {SceneJson | null | undefined} scene @param {Tile[]} tiles @returns {Piece[]} */
export function piecesOf(scene, tiles) {
  if (!scene) return [];
  if (memo.scene === scene && memo.tiles === tiles) return memo.out;
  const out = scene.pieces.map((p) => ({ ...p, rot: p.rot ?? 0, gkey: p.tileId && !p.error ? gkeyOf(p, scene, tiles) : null }));
  memo = { scene, tiles, out };
  return out;
}

/** The pieces whose ids are in `ids` (a selection), in the order they are on the map: a piece that is gone is not selected. */
export const selectedOf = (/** @type {string[]} */ ids, /** @type {Piece[]} */ pieces) => {
  const want = new Set(ids);
  return pieces.filter((p) => want.has(p.id));
};

/** The printer a map or print list names: `{name, label, what}`, or null where the document names none (the base target stands). */
/**
 * @param {any} state  the document's state (js/documents.js)
 * @param {{ name: string, label?: string }[]} printers
 * @param {{ map?: boolean }} [opts]
 */
export function printerOf(state, printers, { map = false } = {}) {
  const name = state?.plan?.scene?.printer ?? state?.plan?.plates?.printer ?? null;
  if (!name) return null;
  const known = printers.find((p) => p.name === name);
  return { name, label: known?.label ?? state?.plan?.plates?.label ?? name, what: map ? 'map' : 'print list' };
}

/** The status line's words for the target: the document's printer on a list or a map, else system and printer. */
/** @param {{ label: string, what: string } | null | undefined} doc @param {{ system: string, printer: string }} target */
export function targetWordsOf(doc, { system, printer }) {
  return doc ? `${doc.label} · this ${doc.what}` : `${SYSTEMS[system] || system} · ${printer}`;
}

/** The status line's verdict on a map: how many pieces (what they come to in print is the panel's to say). */
export const verdictOf = (/** @type {Pick<SceneJson, 'placed'> | null | undefined} */ scene) => {
  const placed = scene?.placed ?? 0;
  return `${placed} piece${placed === 1 ? '' : 's'}`;
};
