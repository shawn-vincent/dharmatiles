// Completion for the source pane, from the element schema and the document's
// own names (docs/design/editor.html, "The text").  Three places:
//
//   at the start of a node   element names, the document's definitions, the
//                            language's own words (stack, region, line…)
//   on a node's line         its element's controls not yet written, and
//                            the place words (in=, at=, along=…)
//   after `name=`            the control's words, or the document's regions
//                            (in=) and lines and edges (along=)

const WORDS = [
  ['stack', 'the entries, in build order'], ['region', 'an area bounded by lines'], ['line', 'a line through points'],
  ['define', 'a definition of your own'], ['use', 'definitions from another file'], ['param', 'a definition\'s parameter'],
  ['about', 'a sentence about the tile'], ['through', 'the points a line passes'], ['bounded-by', 'the lines round a region'], ['fill', 'a point inside a region'],
];
const PLACE = [
  ['in', 'the region it acts in'], ['at', 'where it stands, in squares'], ['along', 'a line or edge it follows'],
  ['against', 'the kind of thing it gathers at'], ['seed', 'pins its look while the tile seed changes'], ['id', 'its name among siblings'], ['weight', 'its share of a scatter'],
];
const EDGES = ['north', 'south', 'east', 'west'];

/** The names the document makes: definitions, regions and lines. */
function names(text) {
  const grab = (re) => [...text.matchAll(re)].map((m) => m[1]);
  return {
    defs: [...grab(/^\s*define\s+([\w+\-.]+)/gm), ...[...text.matchAll(/^\s*use\s+"[^"]*"\s+([^\n/]*)/gm)].flatMap((m) => m[1].trim().split(/\s+/).filter(Boolean))],
    regions: grab(/\bregion\s+([\w+\-.]+)/g),
    lines: grab(/\bline\s+([\w+\-.]+)/g),
  };
}

/**
 * What can go at the caret: {from, to, items: [{label, insert, about, kind}]},
 * or null.  `caret` is TextPane.caret(); `force` lists even with nothing typed.
 */
export function suggest(caret, text, elements, force = false) {
  const before = caret.before;
  // the node the caret is in on this line: after the last `{` or `;`
  const seg = before.slice(Math.max(before.lastIndexOf('{'), before.lastIndexOf(';')) + 1);
  const line = before.slice(before.lastIndexOf('\n') + 1);
  if (/\/\//.test(line) || (before.split('"').length % 2 === 0)) return null; // in a comment or a string
  const n = names(text);
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
    if (ctl === 'in') items = n.regions.map((r) => ({ label: r, insert: r, about: 'a region', kind: 'region' }));
    else if (ctl === 'along') items = [...n.lines.map((l) => ({ label: l, insert: l, about: 'a line', kind: 'line' })), ...EDGES.map((e) => ({ label: e, insert: e, about: 'a tile edge', kind: 'edge' }))];
    else if (ctl === 'against') items = elements.map((e) => ({ label: e.name, insert: e.name, about: e.about, kind: e.role }));
    else {
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
      ...elements.map((e) => ({ label: e.name, insert: e.name, about: e.about, kind: e.role })),
      ...n.defs.map((d) => ({ label: d, insert: d, about: 'a definition', kind: 'definition' })),
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
