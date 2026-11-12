// Completion for the source pane, from the element schema and the document's
// own names (docs/design/editor.html, "The text").  Three places:
//
//   at the start of a node   element names, the document's definitions, the
//                            language's own words (stack, region, line…)
//   on a node's line         its element's controls not yet written, and
//                            the place words (at=, side=…)
//   after `name=`            the control's words, or the document's regions
//                            (at=: regions, lines and edges)

const WORDS = [
  ['stack', 'the entries, in build order'], ['region', 'an area bounded by lines'], ['line', 'a line through points'],
  ['define', 'a definition of your own'], ['use', 'definitions from another file'], ['param', 'a definition\'s parameter'],
  ['about', 'a sentence about the tile'], ['through', 'the points a line passes'], ['boundedBy', 'the lines round a region'], ['fill', 'a point inside a region'],
];
const PLACE = [
  ['at', 'where it goes: a point, a line, a region or foot(cliff)'], ['side', 'which side of its line it stands'],
  ['seed', 'pins its look while the tile seed changes'], ['id', 'its name among siblings'], ['weight', 'its share of a scatter'],
];
const EDGES = ['north', 'south', 'east', 'west'];

const NO_NAMES = { defs: [], regions: [], lines: [], points: [] };

/**
 * What can go at the caret: {from, to, items: [{label, insert, about, kind}]},
 * or null.  `caret` is TextPane.caret(); `force` lists even with nothing typed;
 * `names` are the definitions, regions, lines and points the program's parser
 * found in the text now (`/api/names`).
 */
export function suggest(caret, elements, force = false, language = null, names = NO_NAMES) {
  const before = caret.before;
  // the node the caret is in on this line: after the last `{` or `;`
  const seg = before.slice(Math.max(before.lastIndexOf('{'), before.lastIndexOf(';')) + 1);
  const line = before.slice(before.lastIndexOf('\n') + 1);
  if (/\/\//.test(line)) return null; // in a comment
  if (before.split('"').length % 2 === 0) return language ? inValue(caret, before, line, language, force) : null; // in a string
  const n = { ...NO_NAMES, ...names };
  const el = (name) => elements.find((e) => e.name === name);
  const rank = (items, word) => items
    .filter((i) => i.label.startsWith(word) && i.label !== word)
    .sort((a, b) => a.label.length - b.label.length || a.label.localeCompare(b.label));
  // a value: `name=par`
  const val = seg.match(/([\w+\-.]+)=("?)([\w+\-.]*)$/);
  if (val) {
    const [, ctl, quote, word] = val;
    const node = seg.trim().split(/\s+/)[0];
    let items = [];
    if (ctl === 'at') {
      items = [
        ...n.regions.map((r) => ({ label: r, insert: r, about: 'a region', kind: 'region' })),
        ...n.lines.map((l) => ({ label: l, insert: l, about: 'a line', kind: 'line' })),
        ...EDGES.map((e) => ({ label: e, insert: e, about: 'a tile edge', kind: 'edge' })),
        ...n.points.map((q) => ({ label: q, insert: q, about: 'a point', kind: 'point' })),
      ];
    } else {
      const c = el(node)?.controls.find((x) => x.name === ctl);
      items = (c?.words || []).map((w) => ({ label: w.word, insert: w.word, about: w.about, kind: 'word' }));
    }
    if (!items.length) return null;
    return { from: caret.index - word.length - quote.length, to: caret.index, items: rank(items, word) };
  }
  const word = (before.match(/[A-Za-z_][\w+\-.]*$/) || [''])[0];
  if (!word && !force) return null;
  const lead = seg.slice(0, seg.length - word.length);
  let items;
  if (!lead.trim() || /^\s*\/-\s*$/.test(lead)) {
    // a node's name
    items = [
      ...elements.map((e) => ({ label: e.name, insert: e.name, about: e.about, kind: e.role, family: e.family })),
      ...n.defs.map((d) => ({ label: d, insert: d, about: 'a definition', kind: 'definition', family: 'plant' })),
      ...WORDS.map(([w, a]) => ({ label: w, insert: w, about: a, kind: 'language' })),
    ];
  } else {
    // a control on the node's line
    if (!/\s$/.test(lead)) return null;
    const node = lead.trim().split(/\s+/)[0];
    const e = el(node);
    const written = new Set([...seg.matchAll(/([\w+\-.]+)=/g)].map((m) => m[1]));
    items = [
      ...(e?.controls || []).filter((c) => !c.expert || c.name.startsWith(word)).map((c) => ({ label: c.name, insert: `${c.name}=`, about: c.about, kind: 'control', reopen: true })),
      ...(e ? PLACE : []).map(([w, a]) => ({ label: w, insert: `${w}=`, about: a, kind: 'place', reopen: true })),
    ].filter((i) => !written.has(i.label));
  }
  // dedupe by label
  const seen = new Set();
  items = items.filter((i) => !seen.has(i.label) && seen.add(i.label));
  const out = rank(items, word);
  return out.length ? { from: caret.index - word.length, to: caret.index, items: out.slice(0, 40) } : null;
}

/**
 * Inside a written value of a format 1 document (`top="$bo|"`, `size="8d|"`):
 * the `$` names, the functions and, straight after a number, the units, all
 * from the program's own word lists (`language`, in the map).  Other strings
 * (a description, a name) are the author's.
 */
function inValue(caret, before, line, language, force) {
  const quote = before.lastIndexOf('"');
  const lead = before.slice(0, quote).trimEnd();
  const first = line.trim().split(/\s+/)[0];
  if (!lead.endsWith('=') && !['through', 'fill', 'let', 'override', 'point', 'slope', 'pin'].includes(first)) return null;
  const word = (before.slice(quote + 1).match(/\$?[A-Za-z_]\w*$/) || [''])[0];
  const afterNumber = /\d$/.test(before.slice(quote + 1, before.length - word.length));
  if (!word && !force) return null;
  const items = [];
  if (afterNumber) items.push(...language.units.map((u) => ({ label: u, insert: u, about: 'a unit', kind: 'unit' })));
  else {
    items.push(...language.names.map((n) => ({ label: n, insert: n, about: 'a name set where it is worked out', kind: 'name' })));
    if (!word.startsWith('$')) items.push(...language.functions.map((f) => ({ label: f, insert: f, about: 'a function', kind: 'function' })));
  }
  const out = items
    .filter((i) => i.label.startsWith(word) && i.label !== word)
    .sort((a, b) => a.label.length - b.label.length || a.label.localeCompare(b.label));
  return out.length ? { from: caret.index - word.length, to: caret.index, items: out.slice(0, 40) } : null;
}
