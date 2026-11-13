// The inventory, as the page reads it: what the program's snapshot says you have printed (docs/inventory.html).
// This is the model and nothing else (no page here): how a tile's path is named in the log, what a tile has
// of each system, the have and need pills, the lines the tile level and the plates say, and what the inventory
// document's view lists once its filters are applied.  editor/inventory.js draws it.
//
// The program adds the log up (the pieces: tile, seed, system, colour mode, how many are on the shelf, how many
// were printed, failed or lost); the page never counts a log itself.  A piece is the same piece when its tile,
// seed, system and colour mode are the same: DungeonBlocks and OpenLOCK never count for each other, and a
// multicolour piece wanted where only single colour is owned is "have, single colour", left for you to decide.

const slashes = (s) => String(s || '').replace(/\\/g, '/');
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export const SYSTEMS = [['db', 'DungeonBlocks'], ['ol', 'OpenLOCK']];
export const systemName = (s) => (s === 'ol' ? 'OpenLOCK' : 'DungeonBlocks');
export const colourName = (c) => (c === 'multi' ? 'multicolour' : 'single colour');

/**
 * How the log names a tile: `lib:tiles/ground/1x1-grass` for the library's, `home:meadows/glade` for one in
 * My tiles (`home` is the folder, from the snapshot's inventory), else its path; the `.dtile` is left off.
 * The program does the same (dt_eval::inventory::canonical): the two must agree.
 */
export function canonOf(path, home) {
  let p = slashes(path).replace(/\.dtile$/i, '');
  if (p.startsWith('lib:') || p.startsWith('home:')) return p;
  const h = slashes(home).replace(/\/$/, '');
  if (h && p.startsWith(`${h}/`)) p = `home:${p.slice(h.length + 1)}`;
  return p;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/**
 * A date as a person says it: "today", "13 Nov" for this year, "13 Nov 2025" otherwise.  `iso` is the log's `2026-11-13`
 * (anything else is returned as written); `now` is a Date, the clock unless a test sets it.
 * @param {string} iso @param {Date} [now]
 */
export function dateWords(iso, now = new Date()) {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(iso));
  if (!m) return String(iso);
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (y === now.getFullYear() && mo === now.getMonth() + 1 && d === now.getDate()) return 'today';
  return `${d} ${MONTHS[mo - 1]}${y === now.getFullYear() ? '' : ` ${y}`}`;
}

/** What the shelf holds of one piece, as a sentence: `On the shelf: 2 · printed 13 Nov · single colour · seed 9486`. */
export function pieceLine(p) {
  const when = p.last ? (dateWords(p.last) === 'today' ? 'printed today' : `printed ${dateWords(p.last)}`) : '';
  return [`On the shelf: ${p.have}`, when, colourName(p.colour), `seed ${p.seed}`].filter(Boolean).join(' · ');
}

/** A tile of the log as a short name: the last part of its path. */
export const nameOf = (canon) => String(canon).replace(/^(lib|home):/, '').split('/').pop();

/** Does the program say there is an inventory (its file exists)? */
export const exists = (inv) => !!inv?.exists;

/** The pieces of the inventory that are about `canon` (any system), or all of them. */
export const piecesOf = (inv, canon) => (inv?.pieces || []).filter((p) => canon == null || p.tile === canon);

/**
 * What you have of a tile in a system: `{have, single, multi, seeds, printed, failed, gone, edited, missing}`;
 * `seeds` are the distinct seeds on the shelf, each `{seed, colour, have}`, newest first as the log wrote them.
 */
export function tileHave(inv, canon, system) {
  const ps = piecesOf(inv, canon).filter((p) => p.system === system);
  const out = { have: 0, single: 0, multi: 0, seeds: [], printed: 0, failed: 0, gone: 0, edited: false, missing: false, any: ps.length > 0 };
  for (const p of ps) {
    out.printed += p.printed; out.failed += p.failed; out.gone += p.gone;
    out.missing ||= !!p.missing;
    if (p.have > 0) {
      out.have += p.have;
      out[p.colour === 'multi' ? 'multi' : 'single'] += p.have;
      out.seeds.push({ seed: p.seed, colour: p.colour, have: p.have });
      out.edited ||= !!p.edited;
    }
  }
  return out;
}

/** `7, 12 ×2, 97`: the seeds on the shelf, each with its count when it is more than one. */
export const seedsText = (seeds) => {
  const by = new Map();
  for (const s of seeds) by.set(s.seed, (by.get(s.seed) || 0) + s.have);
  return [...by].map(([seed, n]) => (n > 1 ? `${seed} ×${n}` : String(seed))).join(', ');
};

/** The line the tile level and the folder rows tell: `3 printed · seeds 7, 12, 97`, or nothing yet. */
export function haveLine(h) {
  if (!h.have) return h.failed || h.gone ? `none on the shelf (${[h.failed ? `${h.failed} failed` : '', h.gone ? `${h.gone} gone` : ''].filter(Boolean).join(', ')})` : 'none printed yet';
  const modes = h.single && h.multi ? ` (${h.single} single colour, ${h.multi} multicolour)` : h.multi ? ' in multicolour' : '';
  return `${h.have} printed${modes} · seeds ${seedsText(h.seeds)}`;
}

/**
 * The pill on a tile's row in a folder: `have 2` (a filled one) or `need` (an outline), for the target
 * system; none when there is no inventory yet (nothing to have or need).  The title says which system and what.
 */
export function pill(inv, path, system) {
  if (!exists(inv) || !inv.pieces) return null;
  const h = tileHave(inv, canonOf(path, inv.home), system);
  const sys = systemName(system);
  if (!h.have) return { kind: 'need', text: 'need', title: `None on your shelf for ${sys}${h.failed ? `; ${plural(h.failed, 'failed print')} kept in the log` : ''}` };
  return { kind: 'have', text: `have ${h.have}`, title: `${haveLine(h)} (${sys})${h.edited ? '; edited since printing' : ''}` };
}

/**
 * What one tile of a print list says against the shelf (the plates panel): the kind the plan sent
 * (`{want, have, count, otherColour}`) as `{kind, text}`: `have 2 · 1 to print`, or the caution
 * `have, single colour` where the shelf holds only the other colour mode.
 */
export function wantLine(k, mode) {
  const parts = [];
  if (k.have > 0) parts.push(`have ${k.have}`);
  parts.push(`${k.count} to print`);
  const other = k.otherColour > 0 && k.have === 0;
  if (other) parts.unshift(`have, ${mode === 'multicolour' ? 'single colour' : 'multicolour'}`);
  return { caution: other, text: parts.join(' · ') };
}

/** The plates' totals: how many pieces the inventory leaves off and how many print. */
export function plateTotals(p) {
  const kinds = p?.kinds || [];
  const held = (p?.held || []).reduce((n, h) => n + h.have, 0) + kinds.reduce((n, k) => n + (k.have || 0), 0);
  const toPrint = kinds.reduce((n, k) => n + k.count, 0);
  return { held, toPrint };
}

// ---- the inventory document's view ---------------------------------------------------------------------

/** The filters of the view: a system, a colour mode (null: both), and what to list (`have`, `off` the shelf, or `all`). */
export const defaultFilters = (system) => ({ system: system || 'db', colour: null, show: 'all' });

/** Does a piece pass the filters? */
export function passes(p, f) {
  if (f.system && p.system !== f.system) return false;
  if (f.colour && p.colour !== f.colour) return false;
  if (f.show === 'have' && p.have === 0) return false;
  if (f.show === 'off' && p.have > 0) return false;
  return true;
}

/**
 * The pieces under the filters, grouped by tile: `[{tile, name, missing, edited, pieces, have, failed, gone}]`, tiles
 * sorted by name and each tile's pieces by seed.  `missing` is a tile that is not at its path any more.
 */
export function groups(inv, f) {
  const by = new Map();
  for (const p of inv?.pieces || []) {
    if (!passes(p, f)) continue;
    if (!by.has(p.tile)) by.set(p.tile, { tile: p.tile, name: nameOf(p.tile), missing: false, edited: false, pieces: [], have: 0, failed: 0, gone: 0 });
    const g = by.get(p.tile);
    g.pieces.push(p);
    g.have += p.have; g.failed += p.failed; g.gone += p.gone;
    g.missing ||= !!p.missing;
    g.edited ||= !!p.edited;
  }
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
  const list = [...by.values()].sort((a, b) => collator.compare(a.name, b.name) || collator.compare(a.tile, b.tile));
  for (const g of list) g.pieces.sort((a, b) => a.seed - b.seed || (a.colour < b.colour ? -1 : 1));
  return list;
}

/** What the filters add up to, in words. */
export function totalsLine(gs) {
  const have = gs.reduce((n, g) => n + g.have, 0), failed = gs.reduce((n, g) => n + g.failed, 0), gone = gs.reduce((n, g) => n + g.gone, 0);
  return [`${plural(have, 'piece')} on the shelf`, failed ? `${failed} failed` : '', gone ? `${gone} gone` : ''].filter(Boolean).join(' · ');
}

/** The tiles listed (stems) that could be a missing tile's new path: the same name first, then the rest. */
export function relinkCandidates(tiles, missingCanon) {
  const stem = (t) => nameOf(canonOf(t.path, ''));
  const want = nameOf(missingCanon);
  const real = tiles.filter((t) => /\.dtile$/i.test(t.path));
  return [...real.filter((t) => stem(t) === want), ...real.filter((t) => stem(t) !== want)];
}

/** The text with line numbers, for Source: `[[1, 'dharmatiles version=…'], …]`. */
export const numbered = (text) => String(text ?? '').replace(/\n$/, '').split('\n').map((l, i) => [i + 1, l]);
