/* controls.js — owns every control word, once. The explorer, the presets, randomize, the zero-identity check, the docs
   tables and the solo rule all read this one table; a rename is a one-line change here (`word`, or `label` for the screen).
   Row: { stage, word, label, unit, min, max, def, step, doc, strength?, show? }.
   stage 'stone' = the whole stone. A stage's own row with strength:true is its strength; the rest are its settings.
   A param is keyed by its word for the stone and the stage's strength ('chips'), and 'stage.word' for a setting ('chips.depth'),
   so `depth` can mean one thing in two stages. */
var Stone = Stone || {};
(function (S) {
  const STAGES = [
    { id: 'cuts', name: 'Outline cuts', doc: 'The bounds. Big flat cuts decide the space the stone fills: the box inside a set of half-spaces, shared out over the top, sides and bottom.' },
    { id: 'chunks', name: 'Chunks', doc: 'The space is sliced into strata, each stratum cut across into pieces; the exposed face of a piece sits back and leans, a piece may lose a corner, and small exposed pieces fall away.' },
    { id: 'chips', name: 'Chips', doc: 'Runs of small flat facets of unequal size along the real outer edges of each piece: one big chip among small ones.' },
    { id: 'pockets', name: 'Pockets', doc: 'Dishes over the whole outer surface: a Voronoi cell around each seed scattered on it, found in 3D so the cells run round edges and corners, each deepest at its seed and level with the surface on its rim. Never cuts more than 40% of the stone behind it.' },
    { id: 'grain', name: 'Grain', doc: 'Fine roughness in patches: solid noise at every point, stretched along the strata, so edges and corners go fuzzy with the faces. The only stage that can stand proud of the surface, by up to its depth.' },
    { id: 'wear', name: 'Wear', doc: 'Rounds every edge of every piece, so a groove opens along each joint. Never rounds the seat.' }
  ];
  const pct = (v) => Math.round(v * 100) + '%';
  const CONTROLS = [
    { stage: 'stone', word: 'seed', unit: '', min: 1, max: 9999, def: 1, step: 1, doc: 'The layout: every draw is keyed by it, a stage and a purpose.' },
    { stage: 'stone', word: 'width', unit: 'mm', min: 4, max: 40, def: 24, step: 1, doc: 'Width of the box the stone fills.' },
    { stage: 'stone', word: 'breadth', unit: 'mm', min: 4, max: 40, def: 24, step: 1, doc: 'Front-to-back size of the box the stone fills.' },
    { stage: 'stone', word: 'height', unit: 'mm', min: 4, max: 40, def: 21, step: 1, doc: 'Height of the box the stone fills.' },
    { stage: 'stone', word: 'dip', unit: '°', min: 0, max: 90, def: 50, step: 1, doc: 'How steeply the strata lean: 0 flat layers, 90 upright. Chunks cut along it and grain streaks along it.' },
    { stage: 'stone', word: 'facing', unit: '°', min: 0, max: 360, def: 30, step: 1, doc: 'Which way the strata face around the stone.' },
    { stage: 'stone', word: 'patch', unit: 'mm', min: 2, max: 14, def: 7, step: 0.1, doc: 'Size of the patches anything varies in over the stone: wear radius, grain cover.' },

    { stage: 'cuts', strength: true, word: 'cuts', unit: '', min: 0, max: 1, def: 0.5, step: 0.01, doc: 'Share of the 36 cut slots laid (8 on the top, 12 round the sides, 16 on the bottom).', show: (P) => S.region.counts(S.settingsOf(P, 'cuts')).total + ' cuts' },
    { stage: 'cuts', main: true, label: 'preserve top', word: 'preserveTop', unit: '% of height', min: 0, max: 100, def: 0, step: 1, doc: 'How much of the stone, measured in from its top, no cut may reach: no cut that touches a top face goes deeper into it than the rest. 100% leaves every top alone; 0% lets cuts go as deep as they like.' },
    { stage: 'cuts', main: true, label: 'preserve sides', word: 'preserveSides', unit: '% of half-width', min: 0, max: 100, def: 0, step: 1, doc: 'The same for the sides, measured in from each side face as a share of the stone\'s half-width. 100% leaves every side alone.' },
    { stage: 'cuts', main: true, label: 'preserve bottom', word: 'preserveBottom', unit: '% of height', min: 0, max: 100, def: 0, step: 1, doc: 'The same for the bottom, measured in from the bottom as a share of the height. 100% leaves the bottom alone, except where a stone on a stone must lie within what it stands on: that always wins.' },
    { stage: 'cuts', word: 'bite', unit: '', min: 0, max: 1, def: 0.5, step: 0.01, doc: 'How far a cut goes in: 0 only nicks corners and edges, 1 makes the cuts meet and the block becomes facets.' },
    { stage: 'cuts', word: 'aim', unit: '', min: 0, max: 1, def: 0.3, step: 0.01, doc: '0 cuts anywhere on the faces, 1 cuts only edges and corners.' },

    { stage: 'chunks', strength: true, word: 'chunks', unit: '', min: 0, max: 1, def: 0.5, step: 0.01, doc: 'Share of the planned cracks laid, strata and cross cracks alike. 0 is off: the space is one piece.', show: (P) => pct(P.chunks) + ' of the cracks' },
    { stage: 'chunks', word: 'strata', unit: 'mm', min: 2.5, max: 14, def: 6, step: 0.1, doc: 'Spacing of the parallel cracks.' },
    { stage: 'chunks', word: 'across', unit: 'mm', min: 4, max: 32, def: 12, step: 0.1, doc: 'Spacing of the cross cracks, so the size of the pieces.' },
    { stage: 'chunks', word: 'uneven', unit: '', min: 0, max: 1, def: 0.3, step: 0.01, doc: 'How much the strata thickness varies: 0 even, 1 very mixed.' },
    { stage: 'chunks', word: 'jitter', unit: '', min: 0, max: 1, def: 0.4, step: 0.01, doc: 'Irregularity of the cross-crack points: 0 is a regular grid.' },
    { stage: 'chunks', word: 'carry', unit: '', min: 0, max: 1, def: 0.5, step: 0.01, doc: 'Share of the cross-crack points shared with the next stratum, so those cracks line up across the boundary.' },
    { stage: 'chunks', word: 'halved', unit: '', min: 0, max: 1, def: 0.3, step: 0.01, doc: 'Share of the pieces that get one more cut through the middle.' },
    { stage: 'chunks', word: 'setback', unit: 'mm', min: 0, max: 4, def: 0.8, step: 0.1, doc: 'Deepest an exposed face sits below the surface; each piece is set back 15% to 100% of this. Nothing ever goes outside the space.' },
    { stage: 'chunks', word: 'tilt', unit: '°', min: 0, max: 7, def: 3.5, step: 0.1, doc: 'The most an exposed face leans.' },
    { stage: 'chunks', word: 'corners', unit: '', min: 0, max: 1, def: 0.4, step: 0.01, doc: 'Share of the pieces that lose a corner on the outside.' },
    { stage: 'chunks', word: 'fall', unit: 'mm', min: 0, max: 5, def: 1.5, step: 0.1, doc: 'Exposed pieces smaller than this fall away, outside in. A piece is as big as the cube root of its volume. Pieces resting on the seat never fall this way; 0 means none fall.' },

    { stage: 'chips', strength: true, word: 'chips', unit: 'per cm', min: 0, max: 24, def: 10, step: 0.1, doc: 'Chips per centimetre of sharp outer edge.', show: (P) => (+P.chips).toFixed(1) + ' chips per cm of edge' },
    { stage: 'chips', word: 'depth', unit: 'mm', min: 0.2, max: 2.5, def: 1, step: 0.05, doc: 'How far a chip bites, from the surface.' },
    { stage: 'chips', word: 'length', unit: 'mm', min: 0.5, max: 5, def: 2, step: 0.05, doc: 'How far a chip runs along the edge.' },
    { stage: 'chips', word: 'blunt', unit: '°', min: 0, max: 90, def: 35, step: 1, doc: 'Edges that turn less than this (a cube edge turns 90) get no chips.' },

    { stage: 'pockets', strength: true, word: 'pockets', unit: 'mm', min: 0, max: 3, def: 1.5, step: 0.1, doc: 'How deep a scoop is, from the surface: its sag. It is limited softly by the cell (about 0.3 of its width, 1.9 mm at the default size 6.5, 2.7 mm at size 9), so 3 mm is the top of the range and 1.5 mm already gives about 1.2. 0 is off.', show: (P) => (+P.pockets).toFixed(1) + ' mm deep' },
    { stage: 'pockets', word: 'size', unit: 'mm', min: 4, max: 9, def: 6.5, step: 0.1, doc: 'Width of a pocket: the spacing of the seeds.' },
    { stage: 'pockets', word: 'cover', unit: '', min: 0, max: 1, def: 1, step: 0.01, doc: 'Share of the cells dished, switched on in a fixed order; a cell with no dish stays at the surface. 1 covers the whole surface.' },

    { stage: 'grain', strength: true, word: 'grain', unit: 'mm', min: 0, max: 0.6, def: 0.2, step: 0.01, doc: 'Depth of the grain. It can stand proud of the surface by this much.', show: (P) => (+P.grain).toFixed(2) + ' mm deep' },
    { stage: 'grain', word: 'cover', unit: '', min: 0, max: 1, def: 0.6, step: 0.01, doc: 'Share of the surface grained, in patches.' },

    { stage: 'wear', strength: true, word: 'wear', unit: 'mm', min: 0, max: 1.2, def: 0.5, step: 0.05, doc: 'Rounding radius of every edge of every piece. The seat stays sharp.', show: (P) => (+P.wear).toFixed(2) + ' mm radius' },
    { stage: 'wear', word: 'uneven', unit: '', min: 0, max: 1, def: 0.5, step: 0.01, doc: 'How much the radius varies over the stone: 0 is even.' },
    { stage: 'wear', word: 'exposure', unit: '', min: 0, max: 1, def: 0.4, step: 0.01, doc: 'How much more corners and top edges wear than sheltered faces.' }
  ];
  CONTROLS.forEach((c) => { c.key = c.stage === 'stone' || c.strength ? c.word : c.stage + '.' + c.word; c.label = c.label || c.word; });
  const BY_KEY = {}; CONTROLS.forEach((c) => { BY_KEY[c.key] = c; });
  const settingsRows = (stage) => CONTROLS.filter((c) => c.stage === stage && !c.strength);
  const strengthRow = (stage) => CONTROLS.find((c) => c.stage === stage && c.strength);

  const defaults = () => { const P = {}; CONTROLS.forEach((c) => { P[c.key] = c.strength ? 0 : c.def; }); return P; };
  /* The only place the switches act. off: {stage: true}; solo: a stage id or ''. Solo of a stage at 0 shows its default strength. */
  function effective(P, off, solo) {
    const Q = Object.assign({}, P);
    STAGES.forEach((s) => {
      const r = strengthRow(s.id);
      if (solo) Q[s.id] = s.id === solo ? (P[s.id] > 0 ? P[s.id] : r.def) : 0;
      else if (off && off[s.id]) Q[s.id] = 0;
    });
    return Q;
  }
  const isOn = (P, stage) => P[stage] > 0;
  /* A stage sees only its own strength and settings, under their plain words. */
  function settingsOf(P, stage) {
    const o = { strength: P[stage] }, pre = stage + '.';
    CONTROLS.forEach((c) => { if (c.stage === stage && !c.strength) o[c.word] = P[pre + c.word]; });
    return o;
  }
  /* The stone-wide settings, under their plain words */
  const stoneOf = (P) => ({ seed: P.seed | 0, size: [P.width, P.breadth, P.height], dip: P.dip, facing: P.facing, patch: P.patch });
  const SHAPES = { box: [24, 24, 21], slab: [30, 24, 11], column: [16, 16, 30] };
  const TIERS = { draft: 0.5, fine: 0.3, check: 1.0 };   // voxel size in mm
  const clamp = (P) => { CONTROLS.forEach((c) => { P[c.key] = Math.min(c.max, Math.max(c.min, P[c.key])); }); return P; };

  /* Presets: the six reference panels, converted from the earlier form by formula (chips 24*s^1.4 per cm, wear 3.15*w mm, tilt as it was). */
  const mk = (o) => Object.assign(defaults(), o);
  /* Presets: one for each family of the reference photographs, tuned by eye beside its panel (look pass, 17 Nov). Boxes are about 17 x 15 x 14 mm:
     against the 28 mm figure the reference stones are that size. Strata 2.5 to 3.5 mm with wear 0.1 and grain 0.1 read crisp; wear and grain above 0.3 turn pieces to cobbles. */
  const PRESETS = {
    cube: { label: 'Cube, all 0', P: mk({}) },
    diag: { label: 'A · diagonal cleavage', ref: 'A', P: mk({ seed: 6, width: 18, breadth: 16, height: 16, dip: 55, facing: 135, cuts: 0.15, chunks: 1, 'chunks.strata': 2.5, 'chunks.across': 10, 'chunks.uneven': 0.5, 'chunks.jitter': 0.8, 'chunks.setback': 0.7, 'chunks.tilt': 2, chips: 14, grain: 0.1, wear: 0.1 }) },
    vert: { label: 'B · upright cleavage', ref: 'B', P: mk({ seed: 4, width: 18, breadth: 16, height: 17, dip: 86, facing: 30, cuts: 0.25, chunks: 1, 'chunks.strata': 2.5, 'chunks.across': 12, 'chunks.uneven': 0.8, 'chunks.setback': 1.2, 'chunks.tilt': 5, chips: 12, grain: 0.1, wear: 0.1 }) },
    mass: { label: 'C · massive', ref: 'C', P: mk({ seed: 6, width: 18, breadth: 16, height: 14, dip: 40, facing: 30, cuts: 1, bite: 1, above: 1, around: 1, below: 0.6, aim: 0, chunks: 0.7, 'chunks.strata': 10, 'chunks.across': 12, 'chunks.setback': 2.5, 'chunks.tilt': 7, 'chunks.jitter': 1, chips: 20, 'chips.depth': 1.8, 'chips.length': 3, grain: 0.15, wear: 0.1 }) },
    lay: { label: 'D · stacked layers', ref: 'D', P: mk({ seed: 3, width: 19, breadth: 17, height: 17, dip: 0, facing: 30, cuts: 0.2, chunks: 1, 'chunks.strata': 3, 'chunks.across': 30, 'chunks.uneven': 0.9, 'chunks.jitter': 1, 'chunks.carry': 0.9, 'chunks.setback': 1.6, 'chunks.tilt': 2, chips: 12, grain: 0.1, wear: 0.12 }) },
    blk: { label: 'E · blocky', ref: 'E', P: mk({ seed: 14, width: 19, breadth: 16, height: 15, dip: 10, facing: 30, cuts: 0.6, bite: 0.8, aim: 0.2, chunks: 1, 'chunks.strata': 8, 'chunks.across': 11, 'chunks.uneven': 0.9, 'chunks.jitter': 1, 'chunks.halved': 0.6, 'chunks.corners': 0.7, 'chunks.setback': 2, 'chunks.tilt': 7, chips: 12, grain: 0.1, wear: 0.1 }) },
    pit: { label: 'F · pitted', ref: 'F', P: mk({ seed: 1, width: 18, breadth: 16, height: 15, dip: 30, facing: 30, cuts: 0.6, bite: 0.6, chunks: 0.2, 'chunks.strata': 12, 'chunks.across': 14, 'chunks.setback': 2, chips: 10, pockets: 1.4, 'pockets.size': 5.5, 'pockets.cover': 1, grain: 0.1, wear: 0.15 }) },
    col: { label: 'L · upright columns', ref: 'L', P: mk({ seed: 1, width: 18, breadth: 16, height: 18, dip: 88, facing: 30, cuts: 0.3, chunks: 1, 'chunks.strata': 3.2, 'chunks.across': 30, 'chunks.uneven': 0.9, 'chunks.jitter': 0.9, 'chunks.setback': 1.5, 'chunks.tilt': 6, 'chunks.fall': 0.5, chips: 14, grain: 0.1, wear: 0.1 }) },
    pln: { label: 'P · one diagonal plane', ref: 'P', P: mk({ seed: 3, width: 18, breadth: 16, height: 17, dip: 52, facing: 135, cuts: 0.4, bite: 0.6, chunks: 1, 'chunks.strata': 3, 'chunks.across': 30, 'chunks.uneven': 0.8, 'chunks.jitter': 0.9, 'chunks.setback': 0.9, 'chunks.tilt': 3, chips: 14, grain: 0.1, wear: 0.1 }) },
    one: { label: 'The single block', ref: 'S', P: mk({ seed: 3, width: 20, breadth: 20, height: 16, dip: 88, facing: 15, cuts: 0, chunks: 1, 'chunks.strata': 2.8, 'chunks.across': 12, 'chunks.uneven': 0.8, 'chunks.jitter': 0.9, 'chunks.setback': 1.2, 'chunks.tilt': 5, 'chunks.fall': 0.7, chips: 16, grain: 0.1, wear: 0.1 }) }
  };
  /* The starting point: everything moderately on, and it reads as a stone. */
  PRESETS.starter = { label: 'Starter stone', P: mk({ seed: 3, cuts: 0.4, chunks: 0.7, 'chunks.strata': 3.5, 'chunks.across': 12, 'chunks.setback': 1, chips: 10, pockets: 0, grain: 0.15, wear: 0.2 }) };

  /* random values for every control, from the page's own Math.random (not a stone draw) */
  function randomize(P, rnd) {
    CONTROLS.forEach((c) => { if (c.key === 'seed') return; const v = c.min + rnd() * (c.max - c.min); P[c.key] = c.step >= 1 ? Math.round(v) : Math.round(v / c.step) * c.step; });
    P.seed = 1 + Math.floor(rnd() * 9999); return P;
  }
  S.controls = { CONTROLS, STAGES, BY_KEY, SHAPES, TIERS, PRESETS, settingsRows, strengthRow, defaults, effective, isOn, clamp, randomize };
  S.settingsOf = settingsOf; S.stoneOf = stoneOf;
})(Stone);
