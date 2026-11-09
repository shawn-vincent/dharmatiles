// What a region's property sheet shows, from what its source says
// (docs/language.html, "Regions"): a region is its boundedBy and fill (the paint
// bucket), plus each `within` clause, less what it `except`s.  Pure: the sheet
// (sheet.js) adds the writers.

/** A written distance as the sheet reads it: `8-14 mm`, `6.5 mm`, or the expression as written. */
export function describeDistance(distance, mm, range) {
  const num = (n) => String(Number(n.toFixed(2)));
  if (range) return `${num(range[0])}–${num(range[1])} mm`;
  const text = String(distance?.text ?? '').replace(/^"|"$/g, '');
  if (/^-?\d+(\.\d+)?$/.test(text)) return `${text} mm`;
  return mm == null ? text : `${text} (${num(mm)} mm)`;
}

/** The one-line heading of a clause: `within 8-14 mm of westEdge`, `except crag`. */
export function clauseTitle(c) {
  if (c.kind === 'except') return `except ${c.regions?.text || '…'}`;
  return `within ${describeDistance(c.distance, c.mm, c.range)} of ${c.of?.text || '…'}`;
}

/** Which kind of region the source makes: `bucket` (lines and fill points), `places` (within and except only), or `mixed`. */
export function regionKind(sh) {
  const clauses = sh?.clauses || [];
  if (!clauses.length) return 'bucket';
  return sh.bounded_by || sh.fill ? 'mixed' : 'places';
}

/** How the clauses combine, in plain words. */
export function combineNote(sh) {
  const clauses = sh?.clauses || [];
  const within = clauses.some((c) => c.kind === 'within'), except = clauses.some((c) => c.kind === 'except');
  const kind = regionKind(sh);
  if (kind === 'bucket') return 'The area the lines enclose around the fill points.';
  const base = kind === 'mixed' ? 'The painted area (boundedBy and fill), with every within clause added' : within ? 'Every within clause added together: all that lies within its distance of its place' : 'The whole tile';
  if (kind === 'places' && !within) return `${base}, less the regions it excepts.`;
  return except ? `${base}, then the except regions taken away.` : `${base}.`;
}

/**
 * The places a `within` may be round, grouped the way the language names them: edges, lines, regions
 * (not `self`), points.  `current` is added when it is something else (a literal point).
 */
export function placeChoices(places, self, current) {
  const order = ['edge', 'line', 'region', 'tile', 'point'];
  const words = (places || [])
    .filter((p) => p.name !== self)
    .sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind))
    .map((p) => ({ word: p.name, about: `${p.kind === 'edge' ? 'a tile edge' : p.kind === 'tile' ? 'the whole tile' : `the ${p.kind}`} ${p.name}` }));
  if (current && !words.some((w) => w.word === current)) words.unshift({ word: current, about: 'as written' });
  return words;
}

/**
 * The groups of a region's sheet that its clauses make: for each `within`, its distance and its place; for each
 * `except`, the regions.  Each {clause, heading, controls (the lens's schema), written (settings as `name=text`
 * with spans)}; `as` turns a {name, text, span} into the setting the lens reads.
 */
export function clauseGroups(m, sh, map, as) {
  const regions = (map?.regions || []).filter((r) => r.name !== m.name).map((r) => ({ word: r.name, about: `the region ${r.name}` }));
  const groups = [];
  for (const c of sh.clauses || []) {
    const written = [];
    let controls;
    if (c.kind === 'except') {
      controls = [{ name: 'except', unit: 'several', words: regions, required: true, about: 'the regions taken out of this one' }];
      if (c.regions?.span) written.push(as({ name: 'except', text: c.regions.text, span: c.regions.span }));
    } else {
      controls = [
        { name: 'distance', unit: 'mm', viaSet: true, required: true, about: 'how far from the place it reaches, in mm at the 35 mm square; a range, "[8 .. 14]", draws the depth along the place between the two' },
        { name: 'of', unit: 'word', viaSet: true, required: true, words: placeChoices(map?.places, m.name, c.of?.text), about: 'the place it reaches round: a point (a disc), a line or tile edge (a strip beside it), a region (a ring round it)' },
      ];
      if (c.distance?.span) written.push(as({ name: 'distance', text: c.distance.text, span: c.distance.span }));
      if (c.of?.span) written.push(as({ name: 'of', text: c.of.written, span: c.of.span }));
    }
    groups.push({ clause: c, heading: clauseTitle(c), controls, written });
  }
  return groups;
}
