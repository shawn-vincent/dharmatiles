// @ts-check
// The whole map the page's tests draw: `fixtures/tile-map.json` is the `TileMap` the program makes of `fixtures/tile-map.dtile`
// (crates/dharmatiles/src/preview/tests/map_wire.rs holds the two together), so a test never feeds the overlay a hand-made fragment.
// A test that needs another map changes this one with a spread, which tsc holds to the `TileMap` type.
import { readFileSync } from 'node:fs';

/** @type {import('../js/api').TileMap} */
export const MAP = JSON.parse(readFileSync(new URL('./fixtures/tile-map.json', import.meta.url), 'utf8'));

/**
 * A whole map of the fixture's, with its own size, lattice and regions.
 * @param {number} cols @param {number} rows @param {number} cells @param {import('../js/api').MapRegion[]} [regions]
 * @returns {import('../js/api').TileMap}
 */
export const mapOf = (cols, rows, cells, regions = []) => ({ ...MAP, cols, rows, cells, regions });

/**
 * A whole region of the fixture's, on its own lattice.
 * @param {number} nx @param {number} ny @param {string} bits @param {string} [name]
 * @returns {import('../js/api').MapRegion}
 */
export const regionOf = (nx, ny, bits, name = MAP.regions[0].name) => ({ ...MAP.regions[0], name, nx, ny, bits });

/** The document the map is of, as the program read it. */
export const TEXT = readFileSync(new URL('./fixtures/tile-map.dtile', import.meta.url), 'utf8');

/**
 * The byte span of the `nth` occurrence of `needle` in TEXT, as the map writes spans: its start and its length.
 * @param {string} needle @param {number} [nth] @returns {[number, number]}
 */
export const span = (needle, nth = 0) => {
  let at = -1;
  for (let k = 0; k <= nth; k++) at = TEXT.indexOf(needle, at + 1);
  if (at < 0) throw new Error(`${needle} is not in the fixture document`);
  return [Buffer.byteLength(TEXT.slice(0, at)), Buffer.byteLength(needle)];
};
