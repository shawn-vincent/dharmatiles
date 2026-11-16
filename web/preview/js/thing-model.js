// @ts-check
// The thing view's facts, as plain functions (docs/preview.html, "One thing alone"): which thing the cursor or the selection is on,
// the seeds of the strip, the words for a size.  No DOM and no store is read here, so the editor's state (editor/thing.js) and the
// tests ask the same code.

/** How many draws the strip shows. */
export const STRIP = 12;

/**
 * Which thing of the document the page is on: a `define` that is one thing, or a stack entry that is one (a rock, a tree, a flower, a
 * call of a definition of one), or one of the things a scatter holds.  This is the request's `which` (`ThingRef`), and what to call it.
 * @typedef {{ which: import('./api').ThingRef, name: string, element: string, things: string[], child: number | null }} Target
 */

/**
 * @param {object} s what the page holds
 * @param {boolean} s.thingFile   the document is a tile or a part (not a print list, a map or the inventory)
 * @param {'tile' | 'source'} s.face
 * @param {{ key: string, label: string, element: string, things: { what: string }[] } | null} s.item  the selected stack entry
 * @param {string | null} s.generator  `thing` or `layer`: the selected entry's element, by the program's schema
 * @param {number} s.child  the scatter's thing the view is on
 * @param {{ id: string, what: string, name: string | null, at: number, len: number }[] | null} s.nodes  every node of the source last checked
 * @param {string[] | null} s.things  the definitions that are one thing
 * @param {number} s.caret  where the source's caret is
 * @returns {Target | null}
 */
export function targetOf(s) {
  if (!s.thingFile) return null;
  const define = () => {
    const names = s.things ?? [];
    const n = (s.nodes ?? []).find((x) => x.what === 'define' && x.name != null && names.includes(x.name) && x.at <= s.caret && s.caret <= x.at + x.len);
    return n ? /** @type {Target} */ ({ which: { kind: 'define', name: /** @type {string} */ (n.name) }, name: /** @type {string} */ (n.name), element: 'define', things: [], child: null }) : null;
  };
  const entry = () => {
    const it = s.item;
    if (!it) return null;
    if (s.generator === 'thing') return /** @type {Target} */ ({ which: { kind: 'entry', key: it.key, child: null }, name: it.label, element: it.element, things: [], child: null });
    // a scatter holds things: one of them is drawn, the first until another is chosen
    if (it.things.length) {
      const child = Math.min(Math.max(0, s.child), it.things.length - 1);
      return /** @type {Target} */ ({ which: { kind: 'entry', key: it.key, child }, name: it.things[child].what, element: it.things[child].what, things: it.things.map((t) => t.what), child });
    }
    return null;
  };
  // in the source, the caret says; in the levels, the selection does
  return s.face === 'source' ? define() ?? entry() : entry() ?? define();
}

/** The seeds of the strip's page `page` (from 0): twelve in a row, the first page seeds 1 to 12. @param {number} page */
export const stripSeeds = (page) => Array.from({ length: STRIP }, (_, i) => page * STRIP + i + 1);

/** A length as the page says it: to a tenth, no trailing zero. @param {number} v */
export const num = (v) => String(Math.round(v * 10) / 10);

/** A box as `x × y × z`. @param {{ x: number, y: number, z: number }} b */
export const boxText = (b) => `${num(b.x)} × ${num(b.y)} × ${num(b.z)}`;

/**
 * The words for one end of the extent: printed millimetres (the printer's), and the dmm the document writes beside them, never one as the other.
 * @param {{ x: number, y: number, z: number }} printed @param {{ x: number, y: number, z: number }} written
 */
export const extentText = (printed, written) => ({ printed: `${boxText(printed)} mm`, written: `${boxText(written)} dmm` });

/** The box the camera frames: the mesh's own, grown to the largest the thing can draw (the footprint round the origin), so a new seed does not move the frame. @param {{ x: number, y: number, z: number }} most */
export const frameOf = (most) => ({ lo: [-most.x / 2, -most.y / 2, 0], hi: [most.x / 2, most.y / 2, most.z] });

/** A standard base64 string as the bytes it names. @param {string} b64 */
export function bytesOf(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
}
