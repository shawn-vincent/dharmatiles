//! The built-in elements and their controls, as data.
//!
//! Numbers behind words are documented here, next to the word.  Words
//! marked *provisional* have numbers chosen for this table that the element
//! may still tune; the words themselves are settled.

use crate::schema::{
    Axis, Body, Control, Cost, Element, Family, Footprint, Limits, Node, PlaceWord, Role, Word,
};

const fn c(name: &'static str, value: Axis, about: &'static str) -> Control {
    Control {
        name,
        value: Some(value),
        words: &[],
        subs: &[],
        required: false,
        expert: false,
        limits: None,
        ranges: false,
        weighted: false,
        planned: false,
        default: "",
        about,
    }
}

const fn group(name: &'static str, subs: &'static [Control], about: &'static str) -> Control {
    Control {
        name,
        value: None,
        words: &[],
        subs,
        required: false,
        expert: false,
        limits: None,
        ranges: false,
        weighted: false,
        planned: false,
        default: "",
        about,
    }
}

const fn w(word: &'static str, about: &'static str, sets: &'static [(&'static str, f64)]) -> Word {
    Word {
        word,
        about,
        sets,
        level: None,
    }
}

impl Control {
    const fn words(mut self, words: &'static [Word]) -> Self {
        self.words = words;
        self
    }
    const fn subs(mut self, subs: &'static [Control]) -> Self {
        self.subs = subs;
        self
    }
    const fn required(mut self) -> Self {
        self.required = true;
        self
    }
    const fn expert(mut self) -> Self {
        self.expert = true;
        self
    }
    /// Takes `lo` to `hi`.
    const fn limits(mut self, lo: f64, hi: f64) -> Self {
        self.limits = Some(Limits {
            min: lo,
            max: hi,
            above: false,
        });
        self
    }
    /// Takes numbers over `lo`, up to `hi` (infinite for no high end).
    const fn over(mut self, lo: f64, hi: f64) -> Self {
        self.limits = Some(Limits {
            min: lo,
            max: hi,
            above: true,
        });
        self
    }
    /// Part of the language, not built yet.
    const fn planned(mut self) -> Self {
        self.planned = true;
        self
    }
    /// Takes a weighted range as well (`low(1..6, 4)`): it draws, and its
    /// element reads the odds.
    const fn weighted(mut self) -> Self {
        self.ranges = true;
        self.weighted = true;
        self
    }
    const fn default(mut self, d: &'static str) -> Self {
        self.default = d;
        self
    }
}

// ---- the top of a body or a cloud ----------------------------------------

/// A body's `top=`: the band it fills over its region.  The library's edge
/// defaults (`library/parts/edges.dtile`) say what each body's is when none is
/// written.
const BODY_TOP: Control = c(
    "top",
    Axis::Height,
    "the band the body fills over its region, as a height: `$dirtEdgeHeight + 35dmm`; the library's edge top when not written",
);

/// A thing's `top=`: how high its top stands, worked out where it stands.
const THING_TOP: Control = c(
    "top",
    Axis::Height,
    "how high the top stands, as a height worked out where it stands: `$bottom + [4dmm .. 9dmm]`",
);

// ---- the tile's own nodes -----------------------------------------------

const NAME: Control = c("name", Axis::Word, "what the tile calls it").required();

/// The tile's own nodes, which the editor's sheet draws from the same shape
/// as an element's controls.
pub static NODES: &[Node] = &[
    Node {
        name: "tile",
        about: "the tile: its size and seed, and the description the library shows",
        doc: "language.html",
        controls: &[
            c(
                "size",
                Axis::Size,
                "squares across, then up: \"2x1\"; click to type or pick it from a grid, or drag a number",
            )
            .default("\"1x1\""),
            c(
                "seed",
                Axis::Number,
                "the layout: another seed, another arrangement of the same tile",
            )
            .default("1"),
            c(
                "about",
                Axis::Text,
                "a sentence on what the tile is; shown in the library",
            ),
        ],
    },
    Node {
        name: "line",
        about: "a line drawn through points: it bounds regions and places things along it",
        doc: "language.html#lines",
        controls: &[
            NAME,
            c("wobble", Axis::Mm, "how far the line may wander off its points").default("0"),
            c(
                "width",
                Axis::Mm,
                "makes the line a strip; an element at= it covers the strip",
            ),
            c(
                "through",
                Axis::Raw,
                "the points it runs through: \"x,y\" in squares from the south-west corner (format 1: \"(x, y)\" with units, from the north-west), corners, or edge middles (south, east, north, west); drag a number or click to type",
            )
            .required(),
        ],
    },
    Node {
        name: "region",
        about: "an area of the tile: what lines enclose around a fill point, what lies within a distance of a place, less what it excepts",
        doc: "language.html#regions",
        controls: &[
            NAME,
            c(
                "boundedBy",
                Axis::Names,
                "the lines that divide the tile into this region and the rest",
            )
            .required(),
            c(
                "fill",
                Axis::Raw,
                "a point inside the region, \"x,y\" in squares: the region is the area around it that the lines enclose",
            )
            .required(),
        ],
    },
    Node {
        name: "within",
        about: "a clause of a region: all that lies within a distance of a place",
        doc: "language.html#regions",
        controls: &[
            c(
                "distance",
                Axis::Mm,
                "how far from the place it reaches, in mm at the 35 mm square; a range, \"[8 .. 14]\", draws the depth along the place between the two",
            )
            .required(),
            c(
                "of",
                Axis::Word,
                "the place it reaches round: a point (a disc), a line or tile edge (a strip beside it), a region (a ring round it)",
            )
            .required(),
        ],
    },
    Node {
        name: "except",
        about: "a clause of a region: regions taken out of it",
        doc: "language.html#regions",
        controls: &[c(
            "except",
            Axis::Names,
            "the regions taken out of this one",
        )
        .required()],
    },
];

// ---- shared words -------------------------------------------------------

/// The kinds a scatter or bank may gather against (`at=foot(…)`).
pub static TAGS: &[&str] = &["cliff", "wall", "rock", "tree"];

// ---- dirt, grass --------------------------------------------------------

const BUMP_SIZE: Control = c(
    "bumpSize",
    Axis::Mm,
    "how wide the hills are in its band: broad and rolling, or choppy",
)
.limits(5.0, 70.0);
static CLODS: &[Word] = &[
    // a clod density of 0.7 is `some`; few and many are provisional. `none`
    // is plain dirt: no texture at all.
    w(
        "none",
        "no clods: plain, untextured dirt",
        &[("density", 0.0)],
    ),
    w(
        "few",
        "a scatter of clods (provisional)",
        &[("density", 0.35)],
    ),
    w("some", "clods here and there", &[("density", 0.7)]),
    w(
        "many",
        "crumbly, cloddy earth (provisional)",
        &[("density", 1.0)],
    ),
];

static FLOW: &[Word] = &[
    // a meander amplitude of (10, 30) degrees is `wavy`; the others are provisional.
    w(
        "combed",
        "blades lean together, tidily (provisional)",
        &[("meanderMin", 0.0), ("meanderMax", 10.0)],
    ),
    w(
        "wavy",
        "blades meander together, in phase",
        &[("meanderMin", 10.0), ("meanderMax", 30.0)],
    ),
    w(
        "wild",
        "blades every which way (provisional)",
        &[("meanderMin", 25.0), ("meanderMax", 60.0)],
    ),
];

// ---- rocks --------------------------------------------------------------

static ROCK_SHAPE: &[Word] = &[
    w("rounded", "river stones", &[]),
    w("angular", "broken rock", &[]),
    w("flat", "flat slabs", &[]),
];
static WEAR: &[Word] = &[
    w("crisp", "sharp edges", &[]),
    w("worn", "chipped and rounded edges", &[]),
];

const fn rock_controls(size: &'static str) -> [Control; 5] {
    [
        c("size", Axis::Mm, "rock width")
            .over(0.0, 200.0)
            .weighted()
            .default(size),
        c("shape", Axis::Word, "river stones, broken rock, flat slabs").words(ROCK_SHAPE),
        c("wear", Axis::Word, "sharp, or chipped and rounded edges")
            .words(WEAR)
            .planned(),
        c(
            "buried",
            Axis::Share,
            "how much of each rock is under the ground",
        )
        .limits(0.0, 1.0)
        .weighted(),
        THING_TOP,
    ]
}

// default rock widths: 6 to 15 mm
static ROCK: [Control; 5] = rock_controls("6..15");
static STONE: [Control; 5] = rock_controls("1..6");
static BOULDER: [Control; 5] = rock_controls("4.5..8");
static PEBBLE: [Control; 5] = rock_controls("1..2.2");

// ---- flower -------------------------------------------------------------

static FLOWER: [Control; 6] = [
    c("size", Axis::Mm, "width across the petals' tips")
        .limits(1.0, 20.0)
        .weighted()
        .default("5"),
    c("petals", Axis::Count, "petals round the centre")
        .limits(0.0, 12.0)
        .default("5"),
    c("centre", Axis::Mm, "radius of the centre dome")
        .limits(0.3, 8.0)
        .default("1.5"),
    c("stem", Axis::Mm, "height of the column under each dome")
        .limits(0.3, 6.0)
        .default("1"),
    c("dome", Axis::Mm, "height of the centre dome")
        .limits(0.2, 4.0)
        .default("1"),
    c("petal", Axis::Mm, "height of each petal dome")
        .limits(0.2, 4.0)
        .default("0.5"),
];

// ---- tree ---------------------------------------------------------------

static CROWN_SHAPE: &[Word] = &[
    // `round` is the tree's default envelope; the rest are provisional.
    w(
        "round",
        "a round crown",
        &[
            ("topPointiness", 0.0),
            ("topCurve", 1.4),
            ("bottomPointiness", 0.35),
            ("bottomCurve", 0.8),
        ],
    ),
    w(
        "cone",
        "pointed at the top, like a fir (provisional)",
        &[
            ("topPointiness", 1.0),
            ("topCurve", 0.8),
            ("bottomPointiness", 0.1),
            ("bottomCurve", 1.0),
        ],
    ),
    w(
        "wide",
        "a broad, flat-bottomed spread (provisional)",
        &[
            ("topPointiness", 0.0),
            ("topCurve", 2.0),
            ("bottomPointiness", 0.6),
            ("bottomCurve", 0.6),
        ],
    ),
    w(
        "tall",
        "a narrow column (provisional)",
        &[
            ("topPointiness", 0.3),
            ("topCurve", 1.0),
            ("bottomPointiness", 0.3),
            ("bottomCurve", 1.0),
        ],
    ),
];

static BRANCHING: &[Word] = &[
    // `normal` is the tree's defaults; open and dense are provisional.
    w(
        "open",
        "few long limbs (provisional)",
        &[
            ("split", 40.0),
            ("eagerness", 0.4),
            ("target", 0.25),
            ("balance", 0.8),
            ("exponent", 3.0),
            ("minRadius", 1.0),
        ],
    ),
    w(
        "normal",
        "between the two: some long limbs, some forks",
        &[
            ("split", 30.0),
            ("eagerness", 0.8),
            ("target", 0.33),
            ("balance", 1.0),
            ("exponent", 3.0),
            ("minRadius", 1.0),
        ],
    ),
    w(
        "dense",
        "many forks (provisional)",
        &[
            ("split", 22.0),
            ("eagerness", 1.0),
            ("target", 0.4),
            ("balance", 1.0),
            ("exponent", 2.6),
            ("minRadius", 0.9),
        ],
    ),
];

static LEAVES: &[Word] = &[
    w("on", "separate leaves over the clumps", &[]),
    w("clumps-only", "just foliage clumps", &[]),
    w("bare", "a winter tree", &[]),
];

static TREE: &[Control] = &[
    group(
        "trunk",
        &[
            c("height", Axis::Mm, "bare trunk below the crown")
                .limits(0.0, 200.0)
                .weighted()
                .default("18"),
            c("width", Axis::Mm, "trunk thickness at the ground")
                .limits(0.5, 40.0)
                .weighted(),
        ],
        "the bare trunk",
    ),
    group(
        "crown",
        &[
            c("width", Axis::Mm, "crown width")
                .limits(2.0, 200.0)
                .weighted()
                .default("30"),
            c("shape", Axis::Word, "silhouette")
                .words(CROWN_SHAPE)
                .default("round"),
            c(
                "clumps",
                Axis::Count,
                "clumps of foliage, one per branch end",
            )
            .limits(8.0, 200.0)
            .default("32"),
            c("clumpSize", Axis::Mm, "size of each clump")
                .limits(1.0, 20.0)
                .weighted()
                .default("5.5"),
            c(
                "branching",
                Axis::Word,
                "few long limbs, or many forks; the only branching control in a tile",
            )
            .words(BRANCHING)
            .default("normal"),
        ],
        "the crown",
    ),
    c(
        "leaves",
        Axis::Word,
        "separate leaves, just clumps, or a winter tree",
    )
    .words(LEAVES)
    .default("on"),
    group(
        "leaf",
        &[
            c("length", Axis::Mm, "leaf length")
                .limits(0.5, 20.0)
                .weighted()
                .default("4.5"),
            c("width", Axis::Mm, "leaf width")
                .limits(0.5, 20.0)
                .weighted()
                .default("3"),
        ],
        "one leaf",
    ),
    group(
        "branch",
        &[
            c("split", Axis::Deg, "half-angle of the cone a branch keeps").limits(1.0, 89.0),
            c(
                "eagerness",
                Axis::Number,
                "1 splits early, 0 only near the tips",
            )
            .limits(0.0, 1.0),
            c(
                "target",
                Axis::Number,
                "aim point in the attractor cloud, 0 low to 1 high",
            )
            .limits(0.0, 1.0),
            c(
                "balance",
                Axis::Number,
                "how evenly a fork shares its attractors, 0 to 1",
            )
            .limits(0.0, 1.0),
            c("exponent", Axis::Number, "pipe-model exponent").limits(1.0, 8.0),
            c("minRadius", Axis::Mm, "branch radius at a tip").limits(0.2, 5.0),
        ],
        "the generator's six branching knobs (expert: parts files only)",
    )
    .expert(),
    THING_TOP,
];

// ---- wall ---------------------------------------------------------------

static WALL_STYLE: &[Word] = &[
    w("fieldstone", "drystone fieldstone", &[]),
    w("cut-stone", "dressed ashlar", &[]),
    w("brick", "irregular brick", &[]),
    w("regular-brick", "uniform brick in running bond", &[]),
];
static WALL_HEIGHT: &[Word] = &[
    Word {
        word: "full",
        about: "up to the wall-top level",
        sets: &[("height", crate::schema::H_MM)],
        level: Some("wall-top"),
    },
    w(
        "low",
        "half the wall top (provisional)",
        &[("height", 16.55)],
    ),
];

/// The masonry surface presets: chip size,
/// roundover and relief together.  Numbers in mm.
static TEXTURE: &[Word] = &[
    w(
        "chipped",
        "chipped cut stone (Hirst Arts m50)",
        &[("chip", 0.55), ("rounding", 0.22), ("relief", 0.07)],
    ),
    w(
        "worn",
        "soft, weathered",
        &[("chip", 0.30), ("rounding", 0.60), ("relief", 0.10)],
    ),
    w(
        "hewn",
        "rough-hewn all over",
        &[("chip", 0.80), ("rounding", 0.15), ("relief", 0.16)],
    ),
    w(
        "dressed",
        "near-smooth ashlar, crisp edges",
        &[("chip", 0.15), ("rounding", 0.12), ("relief", 0.03)],
    ),
];

static WALL: &[Control] = &[
    c("style", Axis::Word, "how stones are made and laid")
        .words(WALL_STYLE)
        .default("cut-stone"),
    c("height", Axis::Mm, "height above what it stands on")
        .limits(6.0, 200.0)
        .words(WALL_HEIGHT)
        .default("full"),
    c("thickness", Axis::Mm, "wall thickness")
        .limits(2.0, 40.0)
        .default("7"),
    group(
        "stones",
        &[
            c("height", Axis::Mm, "course height")
                .limits(1.0, 40.0)
                .weighted(),
            c("length", Axis::Mm, "stone length along the wall")
                .limits(2.0, 80.0)
                .weighted(),
            c("texture", Axis::Word, "surface finish of each stone").words(TEXTURE),
        ],
        "stone size",
    ),
    group(
        "joints",
        &[
            c("width", Axis::Mm, "mortar gap").limits(0.0, 3.0),
            c("depth", Axis::Mm, "how deep the gaps between stones look").limits(0.0, 5.0),
        ],
        "the gaps between stones",
    ),
    group(
        "edges",
        &[c("rounding", Axis::Mm, "how worn the stone corners are")
            .limits(0.0, 5.0)
            .weighted()],
        "the stone corners",
    ),
    c("coping", Axis::Word, "what tops a drystone wall").words(&[
        w("loose", "loose stones perched on top", &[]),
        w("on-edge", "stones set on edge", &[]),
    ]),
    c("bond", Axis::Word, "random courses, or brick-style offsets").words(&[
        w("irregular", "random courses and bays", &[]),
        w("running", "uniform courses, half-unit offsets", &[]),
    ]),
    c("ruin", Axis::Share, "how much has fallen").limits(0.0, 1.0),
    c("top", Axis::Word, "castle crenellations").words(&[
        w("flat", "a flat top", &[]),
        w("battlements", "merlons and crenels", &[]),
    ]),
];

// ---- floors -------------------------------------------------------------

static FLAGSTONES: &[Control] = &[
    BODY_TOP,
    c("style", Axis::Word, "cut slabs, or drystone flagstones")
        .words(&[
            w("cut-stone", "dressed slabs on the square grid", &[]),
            w(
                "fieldstone",
                "drystone flags: a fieldstone wall laid flat",
                &[],
            ),
        ])
        .default("cut-stone"),
    c("texture", Axis::Word, "surface finish of each slab")
        .words(TEXTURE)
        .default("dressed"),
    c("slabs", Axis::Count, "slabs across one square")
        .limits(1.0, 6.0)
        .default("1"),
];

static PLANKS: &[Control] = &[
    BODY_TOP,
    c("style", Axis::Word, "how the boards are cut")
        .words(&[
            w("random", "boards of mixed lengths, joints staggered", &[]),
            w(
                "even",
                "boards half a square long, each course set half a board on",
                &[],
            ),
        ])
        .default("random"),
    c("texture", Axis::Word, "the wood's finish")
        .words(&[
            w("smooth", "planed boards, faint grain, few checks", &[]),
            w("worn", "rounded edges, open grain, some checks", &[]),
            w("rough", "sawn boards, coarse grain, many checks", &[]),
        ])
        .default("worn"),
    c("boards", Axis::Count, "boards across one square")
        .limits(2.0, 16.0)
        .default("8"),
];

// ---- cliff --------------------------------------------------------------

static ROCK_MIX: &[Word] = &[
    w(
        "mixed",
        "the run shares",
        &[
            ("columns", 38.0),
            ("blocks", 38.0),
            ("slabs", 12.0),
            ("odd", 6.0),
            ("monoliths", 6.0),
        ],
    ),
    w(
        "columns",
        "basalt columns (provisional)",
        &[
            ("columns", 70.0),
            ("blocks", 15.0),
            ("slabs", 5.0),
            ("odd", 4.0),
            ("monoliths", 6.0),
        ],
    ),
    w(
        "blocks",
        "blocky granite (provisional)",
        &[
            ("columns", 15.0),
            ("blocks", 70.0),
            ("slabs", 7.0),
            ("odd", 6.0),
            ("monoliths", 2.0),
        ],
    ),
    w(
        "slabs",
        "layered sandstone (provisional)",
        &[
            ("columns", 5.0),
            ("blocks", 20.0),
            ("slabs", 70.0),
            ("odd", 5.0),
            ("monoliths", 0.0),
        ],
    ),
];

static STONE_SIZE: &[Word] = &[
    w("rubble", "small broken stones", &[("stoneSize", 1.0)]),
    w(
        "broken",
        "between the two (provisional)",
        &[("stoneSize", 1.15)],
    ),
    w(
        "massive",
        "big masses: 1.35x rubble size",
        &[("stoneSize", 1.35)],
    ),
];

static CLIFF: &[Control] = &[
    BODY_TOP,
    c(
        "rock",
        Axis::Word,
        "what the cliff is stacked from, or weights",
    )
    .words(ROCK_MIX)
    .default("mixed")
    .subs(&[
        c("columns", Axis::Number, "weight of tall upright stones"),
        c("blocks", Axis::Number, "weight of chunky blocks"),
        c("slabs", Axis::Number, "weight of thin tilted slabs"),
        c("odd", Axis::Number, "weight of irregular lumps"),
        c("monoliths", Axis::Number, "weight of ground-to-top stones"),
    ]),
    c(
        "stoneSize",
        Axis::Share,
        "small broken stones, or big masses",
    )
    .words(STONE_SIZE)
    .default("rubble"),
    c(
        "ledges",
        Axis::Count,
        "steps down the free faces: shelves a miniature can stand on",
    )
    .limits(0.0, 2.0)
    .default("1")
    .subs(&[c(
        "height",
        Axis::Share,
        "how far up the shelf is, as a share of the cliff",
    )]),
    group(
        "shattered",
        &[
            c("patches", Axis::Count, "broken patches along the run"),
            c("width", Axis::Mm, "how wide each patch is").weighted(),
        ],
        "broken patches with more cracks and scree",
    ),
];

// ---- scatter ------------------------------------------------------------

static SCATTER: &[Control] = &[
    c("count", Axis::Count, "how many, in total")
        .limits(0.0, 10_000.0)
        .weighted(),
    c(
        "perSquare",
        Axis::PerSquare,
        "how many per square of the area, or of length along a line",
    )
    .weighted(),
    c(
        "spread",
        Axis::Mm,
        "a cluster within this distance of a point",
    )
    .planned(),
    c(
        "reach",
        Axis::Mm,
        "with at=foot(…): how far out from the foot; with at= a line: how far either side of it (default 5dmm)",
    ),
    c(
        "clumps",
        Axis::PerSquare,
        "gathers them into clumps; default spread evenly",
    )
    .planned(),
    c("gap", Axis::Mm, "least space between neighbours"),
];

// ---- the table ----------------------------------------------------------

const SURFACE_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::Along];
const THING_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::At];
const RUN_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::Along, PlaceWord::Side];

/// A thing placed with `at=`; `scattered` when a scatter may hold it yet.
const fn thing(
    name: &'static str,
    about: &'static str,
    controls: &'static [Control],
    scattered: bool,
) -> Element {
    Element {
        name,
        about,
        role: Role::Thing,
        place: THING_PLACE,
        honours: &[PlaceWord::At],
        scattered,
        family: Family::Thing,
        cost: Cost {
            fixed: 0.05,
            per_square: 0.0,
        },
        controls,
        ..Element::BASE
    }
}

/// A rock: a thing sized by `size`, with a cloud for a `top=`.
const fn rock(name: &'static str, about: &'static str, controls: &'static [Control]) -> Element {
    Element {
        cloud_top: true,
        footprint: Some(Footprint {
            control: "size",
            disc: true,
        }),
        ..thing(name, about, controls, true)
    }
}

/// Every built-in element.
pub static ELEMENTS: &[Element] = &[
    Element {
        name: "dirt",
        about: "a textured earth surface: clods, grit and pebbles over gentle rises",
        family: Family::Surface,
        role: Role::Surface,
        place: SURFACE_PLACE,
        holds_scatter: true,
        body: Some(Body::Land {
            slope_deg: dt_lang::LAND_SLOPE_DEG,
        }),
        cost: Cost {
            fixed: 0.0,
            per_square: 0.06,
        },
        controls: &[
            BODY_TOP,
            BUMP_SIZE,
            c("clods", Axis::Word, "lumps of earth on the surface")
                .words(CLODS)
                .default("some"),
            c(
                "maxSlope",
                Axis::Deg,
                "the steepest surface under it that dirt stays on; on anything steeper it slides off and leaves it bare",
            )
            .limits(5.0, 85.0)
            .default("35"),
        ],
        ..Element::BASE
    },
    Element {
        name: "grass",
        about: "separate sculpted blades in tufts",
        family: Family::Plant,
        role: Role::Surface,
        place: SURFACE_PLACE,
        holds_scatter: true,
        cost: Cost {
            fixed: 0.0,
            per_square: 0.35,
        },
        controls: &[
            c("length", Axis::Mm, "blade length along the ground")
                .over(0.0, f64::INFINITY)
                .weighted()
                .default("3.5..6.5"),
            c("height", Axis::Mm, "how tall a blade stands over the ground")
                .over(0.0, f64::INFINITY)
                .weighted()
                .default("1.5..2"),
            c(
                "clearings",
                Axis::PerSquare,
                "patches of bare earth the grass parts round",
            )
            .limits(0.0, 20.0)
            .default("0"),
            c("cover", Axis::Share, "share of the ground grassed over")
                .limits(0.0, 1.0)
                .default("100%"),
            c(
                "bare",
                Axis::Share,
                "the least share left bare: clearings are added until the bare earth, rocks included, reaches it",
            )
            .limits(0.0, 0.9)
            .default("0%"),
            c("flow", Axis::Word, "how tidily the blades lean together")
                .words(FLOW)
                .default("wavy"),
        ],
        ..Element::BASE
    },
    Element {
        name: "reeds",
        about: "clumps of tapered reeds at the waterline, leaning toward the water",
        family: Family::Plant,
        role: Role::Surface,
        place: &[PlaceWord::In],
        cost: Cost {
            fixed: 0.05,
            per_square: 0.0,
        },
        controls: &[
            c("clumps", Axis::Count, "how many clumps along the shore")
                .limits(0.0, 30.0)
                .weighted()
                .default("3..5"),
            c("stems", Axis::Count, "stems in a clump")
                .limits(1.0, 30.0)
                .weighted()
                .default("5..9"),
            c("height", Axis::Mm, "how tall a stem stands")
                .over(0.0, f64::INFINITY)
                .weighted()
                .default("8..13"),
        ],
        ..Element::BASE
    },
    Element {
        name: "water",
        about: "still water with swells round its shores and rocks: fills its area up to its level",
        family: Family::Water,
        role: Role::Surface,
        place: &[PlaceWord::In],
        body: Some(Body::Fluid {
            settle_mm: dt_lang::WATER_SETTLE_MM,
        }),
        cost: Cost {
            fixed: 0.0,
            per_square: 0.1,
        },
        controls: &[BODY_TOP],
        ..Element::BASE
    },
    rock("rock", "one rock", &ROCK),
    rock("stone", "a rock with stone sizes, about 1–6dmm", &STONE),
    rock(
        "boulder",
        "a rock with boulder sizes, about 4.5–8dmm",
        &BOULDER,
    ),
    rock(
        "pebble",
        "a rock with pebble sizes, about 1–2.2dmm",
        &PEBBLE,
    ),
    Element {
        family: Family::Plant,
        footprint: Some(Footprint {
            control: "size",
            disc: true,
        }),
        ..thing(
            "flower",
            "a wildflower: a thin dome on a column, ringed by petal domes on cones",
            &FLOWER,
            true,
        )
    },
    Element {
        family: Family::Plant,
        cloud_top: true,
        footprint: Some(Footprint {
            control: "crown/width",
            disc: true,
        }),
        cost: Cost {
            fixed: 6.5,
            per_square: 0.0,
        },
        ..thing(
            "tree",
            "a space-colonisation tree with foliage clumps and leaves",
            TREE,
            true,
        )
    },
    Element {
        name: "wall",
        about: "a masonry wall; on a tile edge it stands inside the tile",
        role: Role::Run,
        place: RUN_PLACE,
        needs: &[PlaceWord::Along],
        honours: &[PlaceWord::Along],
        footprint: Some(Footprint {
            control: "thickness",
            disc: false,
        }),
        masonry: true,
        cost: Cost {
            fixed: 0.7,
            per_square: 0.0,
        },
        controls: WALL,
        ..Element::BASE
    },
    Element {
        name: "flagstones",
        about: "a floor of stone slabs, a wall laid on its side; each slab's height and tilt is drawn inside its band",
        family: Family::Surface,
        role: Role::Surface,
        place: &[PlaceWord::In],
        body: Some(Body::Pieces),
        masonry: true,
        cost: Cost {
            fixed: 0.4,
            per_square: 0.0,
        },
        controls: FLAGSTONES,
        ..Element::BASE
    },
    Element {
        name: "planks",
        about: "a floor of wooden boards; each board's height and tilt is drawn inside its band",
        family: Family::Surface,
        role: Role::Surface,
        place: &[PlaceWord::In],
        body: Some(Body::Pieces),
        cost: Cost {
            fixed: 0.4,
            per_square: 0.0,
        },
        controls: PLANKS,
        ..Element::BASE
    },
    Element {
        name: "cliff",
        about: "a cliff of stacked rock: stones stacked so their tops span the band, in its region",
        family: Family::Surface,
        role: Role::Surface,
        place: SURFACE_PLACE,
        body: Some(Body::Pieces),
        cost: Cost {
            fixed: 0.2,
            per_square: 0.0,
        },
        controls: CLIFF,
        ..Element::BASE
    },
    Element {
        name: "scatter",
        about: "more than one thing: how many and where; its children say what",
        role: Role::Scatter,
        place: &[PlaceWord::In, PlaceWord::Along, PlaceWord::Against],
        honours: &[PlaceWord::Along, PlaceWord::Against],
        cost: Cost {
            fixed: 0.05,
            per_square: 0.0,
        },
        controls: SCATTER,
        ..Element::BASE
    },
];

/// Old words, with what replaces them, shown for a document that still
/// writes one (the hint's examples are format 1).  Used for hints only.
pub static RENAMED: &[(&str, &str)] = &[
    ("hills", "`hills` is now `dirt`"),
    ("meadow", "`meadow` is now `grass`"),
    (
        "scree",
        "scree is things scattered at a cliff's foot: `scatter at=\"foot(cliff)\" perSquare=55 { stone }`",
    ),
    ("blocks", "scree `blocks` are now `boulder`s in a scatter"),
    ("family", "`family` is now `style`"),
    (
        "floor",
        "`floor` is now `flagstones` (stone slabs) or `planks` (boards); its `level` is the body's `top`",
    ),
    (
        "level",
        "a floor's `level` is now its `top=`: `flagstones top=\"$floorHeight\"`",
    ),
    ("self", "`self=` is now `at=`"),
    ("near", "`near=` is now `at=\"foot(…)\"`"),
    ("heights", "`heights` is now `let`: `let $foot \"4dmm\"`"),
    ("levels", "`levels` is now `let`: `let $foot \"4dmm\"`"),
    ("cols", "a tile's size is `size=\"2x1\"`, columns first"),
    ("rows", "a tile's size is `size=\"2x1\"`, columns first"),
    ("tips", "a tree's branch ends are `crown clumps=…`"),
    ("canopy", "a tree's crown is `crown width=…`, in dmm"),
    ("foliage", "foliage is `leaves on`, `clumps-only` or `bare`"),
    ("mass", "stone size is `stoneSize rubble|broken|massive`"),
    (
        "form",
        "a cliff's form is its region: `cliff top=\"$dirtEdgeHeight + 35dmm\" at=crag`; run `dharmatiles migrate`",
    ),
    (
        "bank",
        "a bank is drawn: a wide-band `pin top=\"[$dirtEdgeHeight .. $dirtEdgeHeight + 3.5dmm]\" at=apron` on the dirt; run `dharmatiles migrate`",
    ),
    (
        "dirt-on-tops",
        "dirt on a cliff's top is a dirt layer: `dirt top=\"$bottom + [0.6dmm .. 1.2dmm]\" at=crag`",
    ),
    (
        "stones",
        "dirt beds no stones: scatter rocks on it after it, `scatter perSquare=1 { rock size=\"4dmm .. 8dmm\" }`, or delete `stones=none`; a cliff's mix is `rock mixed`, or `rock columns=… blocks=…`",
    ),
    ("shatter", "write `shattered patches=… width=…`"),
    (
        "blade",
        "grass controls go on the `grass` line: `grass length=… curl=…`",
    ),
    ("organic", "a line's wobble is `line name width=… wobble=…`"),
    (
        "mostly",
        "write the weighting on the size: `size=\"low(1dmm .. 6dmm, 11)\"` for mostly=small, `high(1dmm .. 6dmm)` for large, the plain range for even; `dharmatiles migrate` rewrites old tiles",
    ),
    ("lines", "a region is `boundedBy` its lines"),
    (
        "flowers",
        "a flower is a thing: one is `flower at=…`, many are `scatter perSquare=4 { flower }`; `dharmatiles migrate` rewrites old tiles",
    ),
    (
        "in",
        "one word names a place: write `at=` and the region; `dharmatiles migrate` rewrites old tiles",
    ),
    (
        "along",
        "one word names a place: write `at=` and the line or edge; `dharmatiles migrate` rewrites old tiles",
    ),
    (
        "against",
        "write `at=\"foot(cliff)\"`, the band at that kind's foot; `dharmatiles migrate` rewrites old tiles",
    ),
];

/// The control names written with hyphens before names became camelCase,
/// each with its new spelling. `dharmatiles migrate` rewrites them and an
/// old one is an `unknown-control` error that names the new one.
pub static OLD_CONTROL_NAMES: &[(&str, &str)] = &[
    ("per-square", "perSquare"),
    ("bump-size", "bumpSize"),
    ("clump-size", "clumpSize"),
    ("min-radius", "minRadius"),
    ("stone-size", "stoneSize"),
    ("bounded-by", "boundedBy"),
];

/// The camelCase name of an old hyphenated control name.
#[must_use]
pub fn camel_of(old: &str) -> Option<&'static str> {
    OLD_CONTROL_NAMES
        .iter()
        .find(|(o, _)| *o == old)
        .map(|(_, n)| *n)
}

/// The hint for an old word.
#[must_use]
pub fn renamed(word: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(w, _)| *w == word).map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_within_each_element() {
        for l in ELEMENTS {
            let mut names: Vec<_> = l.controls.iter().map(|k| k.name).collect();
            let n = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), n, "{}", l.name);
            for k in l.controls {
                let mut subs: Vec<_> = k.subs.iter().map(|s| s.name).collect();
                let n = subs.len();
                subs.sort_unstable();
                subs.dedup();
                assert_eq!(subs.len(), n, "{} {}", l.name, k.name);
            }
        }
    }

    #[test]
    fn word_defaults_are_words() {
        for l in ELEMENTS {
            for k in l
                .controls
                .iter()
                .chain(l.controls.iter().flat_map(|k| k.subs))
            {
                if k.value == Some(Axis::Word) && !k.default.is_empty() {
                    assert!(
                        k.words.iter().any(|w| w.word == k.default),
                        "{} {}",
                        l.name,
                        k.name
                    );
                }
            }
        }
    }

    /// Every control of every element, with the name that owns it.
    fn every_control() -> Vec<(&'static str, &'static Control)> {
        let mut out = Vec::new();
        for l in ELEMENTS {
            for k in l.controls {
                out.push((l.name, k));
                out.extend(k.subs.iter().map(|s| (k.name, s)));
            }
        }
        out
    }

    #[test]
    fn every_number_default_is_one_its_control_takes() {
        let level = |_: &str| None;
        for (owner, k) in every_control() {
            let Some(crate::model::Value::Num(n)) = k.default_value() else {
                continue;
            };
            let (lo, hi) = n
                .bounds(&level)
                .unwrap_or_else(|e| panic!("{owner} {}: {e}", k.name));
            if let Err(r) = k.check(owner, lo, hi, n.expr.is_range()) {
                panic!("{owner} {}: {}", k.name, r.message);
            }
        }
    }

    #[test]
    fn branching_words_stay_within_the_expert_knobs() {
        let tree = crate::schema::element("tree").unwrap();
        let branch = tree.control("branch").unwrap();
        for word in tree
            .control("crown")
            .unwrap()
            .sub("branching")
            .unwrap()
            .words
        {
            for (name, v) in word.sets {
                let knob = branch.sub(name).unwrap();
                assert!(
                    knob.check("branch", *v, *v, false).is_ok(),
                    "{} {name}={v}",
                    word.word
                );
            }
        }
    }

    #[test]
    fn a_number_outside_its_control_is_refused_once_in_the_schema() {
        let rock = crate::schema::element("rock").unwrap();
        let size = rock.control("size").unwrap();
        let r = size.check("rock", 250.0, 250.0, false).unwrap_err();
        assert!(r.message.starts_with("`rock size` is "), "{}", r.message);
        let ruin = crate::schema::element("wall")
            .unwrap()
            .control("ruin")
            .unwrap();
        let r = ruin.check("wall", 1.5, 1.5, false).unwrap_err();
        assert_eq!(r.message, "`wall ruin` is 0% to 100%, not 150%");
        let boards = crate::schema::element("planks")
            .unwrap()
            .control("boards")
            .unwrap();
        assert!(boards.check("planks", 1.0, 1.0, false).is_err());
        assert!(boards.check("planks", 8.0, 8.0, false).is_ok());
        let rows = crate::schema::element("planks")
            .unwrap()
            .control("boards")
            .unwrap();
        let r = rows.check("planks", 4.0, 8.0, true).unwrap_err();
        assert_eq!(r.message, "`planks boards` takes one number, not a range");
    }

    #[test]
    fn typed_text_is_checked_as_a_written_value_is() {
        let check = |k, owner: &str, typed: &str| {
            crate::decode::check_text_in(k, owner, typed, crate::units::Units::default())
        };
        let ctl = |e: &str, c: &str| crate::schema::element(e).unwrap().control(c).unwrap();
        let tree = crate::schema::element("tree")
            .unwrap()
            .control("trunk")
            .unwrap()
            .sub("height")
            .unwrap();
        assert!(check(tree, "tree", "40dmm").is_none());
        assert!(check(tree, "tree", "30dmm..50dmm").is_none());
        assert_eq!(
            check(tree, "tree", "250dmm").unwrap().message,
            "`tree height` is 0dmm to 200dmm, not 250dmm"
        );
        assert_eq!(
            check(tree, "tree", "50dmm..30dmm").unwrap().message,
            "a range is written low end first"
        );
        // a bare number is refused: a length carries its unit
        assert!(check(tree, "tree", "40").is_some());
        // the reader resolves names; a word is not a number
        assert!(check(tree, "tree", "$wallHeight * 4dmm").is_none());
        assert!(check(tree, "tree", "tall").is_none());
        // a syntax error is refused; nothing typed yet is not
        assert!(
            check(tree, "tree", "4dmm +")
                .unwrap()
                .message
                .contains("`4dmm +`")
        );
        assert!(check(tree, "tree", "  ").is_none());
        let ruin = ctl("wall", "ruin");
        assert!(check(ruin, "wall", "40%").is_none());
        assert_eq!(
            check(ruin, "wall", "150%").unwrap().message,
            "`wall ruin` is 0% to 100%, not 150%"
        );
        let boards = ctl("planks", "boards");
        assert!(
            check(boards, "planks", "4..8")
                .unwrap()
                .message
                .contains("one number")
        );
    }

    #[test]
    fn branching_words_cover_every_expert_knob() {
        let tree = crate::schema::element("tree").unwrap();
        let branch = tree.control("branch").unwrap();
        let crown = tree.control("crown").unwrap();
        for word in crown.sub("branching").unwrap().words {
            let names: Vec<_> = word.sets.iter().map(|(n, _)| *n).collect();
            let knobs: Vec<_> = branch.subs.iter().map(|k| k.name).collect();
            assert_eq!(names, knobs, "{}", word.word);
        }
    }
}
