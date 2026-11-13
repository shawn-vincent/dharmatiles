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
  return sh.boundedBy || sh.fill ? 'mixed' : 'places';
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
 * with spans)}; `as` turns a {name, text, span} into the setting the lens reads; `row(node, name)` is the
 * program's row for a control of a clause node (`within`'s distance, `except`'s regions): what it measures,
 * its help, whether it is required.  The words it may be are the tile's own, so they are added here.
 */
export function clauseGroups(m, sh, map, as, row) {
  const regions = (map?.regions || []).filter((r) => r.name !== m.name).map((r) => ({ word: r.name, about: `the region ${r.name}` }));
  const groups = [];
  for (const c of sh.clauses || []) {
    const written = [];
    let controls;
    if (c.kind === 'except') {
      controls = [{ ...row('except', 'except'), words: regions }];
      if (c.regions?.span) written.push(as({ name: 'except', text: c.regions.text, span: c.regions.span }));
    } else {
      // written through the server's edit route, not by replacing the text under the pointer: `viaSet`
      controls = [
        { ...row('within', 'distance'), viaSet: true },
        { ...row('within', 'of'), viaSet: true, words: placeChoices(map?.places, m.name, c.of?.text) },
      ];
      if (c.distance?.span) written.push(as({ name: 'distance', text: c.distance.text, span: c.distance.span }));
      if (c.of?.span) written.push(as({ name: 'of', text: c.of.written, span: c.of.span }));
    }
    groups.push({ clause: c, heading: clauseTitle(c), controls, written });
  }
  return groups;
}

/**
 * The clauses a region's "add clause" menu offers, each {kind, label, title, disabled, node}: `node` is the source it writes.
 * A `within` is a small disc round the tile's centre, an `except` names another region, a `boundedBy` names a line (with a
 * `fill` point beside it when the region has none, since the pair make the paint bucket).  One that cannot be written says why.
 */
export function clauseMenu(m, sh, map) {
  const other = (map?.regions || []).find((r) => r.name !== m.name);
  const line = (map?.lines || [])[0];
  const written = !!sh?.boundedBy?.span;
  return [
    { kind: 'within', label: 'within', title: 'Add everything within a distance of a place (a disc round a point, a strip beside a line, a ring round a region)', node: 'within 6 of=center' },
    other
      ? { kind: 'except', label: 'except', title: 'Take another region out of this one', node: `except ${other.name}` }
      : { kind: 'except', label: 'except', title: 'There is no other region to take out', why: 'no other region', disabled: true },
    written
      ? { kind: 'boundedBy', label: 'boundedBy', title: 'Already written: choose its lines above', why: 'already written', disabled: true }
      : !line
        ? { kind: 'boundedBy', label: 'boundedBy', title: 'Draw a line first', why: 'no line yet', disabled: true }
        : { kind: 'boundedBy', label: 'boundedBy', title: 'Bound the region by a line, filled from a point', node: sh?.fill?.span ? `boundedBy ${line.name}` : `boundedBy ${line.name}; fill center` },
  ];
}
