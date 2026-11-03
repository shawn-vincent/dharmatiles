//! The built-in looks and their controls: the R6 vocabulary tables
//! (docs/rust/language.html, "Looks and their vocabulary"), as data.
//!
//! Numbers behind words are documented here, next to the word.  Words
//! marked *provisional* have numbers chosen for this table that the look's
//! own phase (P2–P7) tunes against its lab; the words themselves are
//! decided.

use crate::schema::{Axis, Control, Look, PlaceWord, Role, Word};

const fn c(name: &'static str, value: Axis, about: &'static str) -> Control {
    Control {
        name,
        value: Some(value),
        words: &[],
        subs: &[],
        required: false,
        expert: false,
        limits: None,
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
    const fn limits(mut self, lo: f64, hi: f64) -> Self {
        self.limits = Some((lo, hi));
        self
    }
    const fn default(mut self, d: &'static str) -> Self {
        self.default = d;
        self
    }
}

// ---- shared words -------------------------------------------------------

/// `mostly=`: skews a range draw, `lo + (hi - lo) * u^exponent`.
pub static MOSTLY: &[Word] = &[
    w(
        "small",
        "skewed toward the low end (rockpile's exponent 2)",
        &[("exponent", 2.0)],
    ),
    w("even", "a uniform draw", &[("exponent", 1.0)]),
    w("large", "leaning toward the high end", &[("exponent", 0.5)]),
];

/// The kinds a scatter or bank may gather against (`against=`).
pub static TAGS: &[&str] = &["cliff", "wall", "rock", "tree"];

// ---- ground, dirt, grass ------------------------------------------------

const BUMPS: Control = c("bumps", Axis::Mm, "how far it rises and falls")
    .limits(0.0, 15.0)
    .default("0")
    .subs(&[
        c("up", Axis::Mm, "how far it rises").limits(0.0, 15.0),
        c("down", Axis::Mm, "how far it falls").limits(0.0, 15.0),
    ]);
const BUMP_SIZE: Control = c(
    "bump-size",
    Axis::Mm,
    "how wide the hills are: broad and rolling, or choppy",
)
.limits(5.0, 70.0);
const LEVEL: Control = c("level", Axis::Level, "where the surface sits").default("ground");

static STONES: &[Word] = &[
    // dirtlab STONES_2X2 (4, 7) per 2×2 is `some`; `many` is provisional.
    w(
        "none",
        "bare dirt, no stones",
        &[("min", 0.0), ("max", 0.0)],
    ),
    w(
        "some",
        "dirtlab's few bedded stones",
        &[("min", 4.0), ("max", 7.0)],
    ),
    w(
        "many",
        "a stony patch (provisional)",
        &[("min", 8.0), ("max", 12.0)],
    ),
];

static CLODS: &[Word] = &[
    // dirtlab CLOD_DENSITY 0.7 is `some`; few and many are provisional.
    w(
        "few",
        "a scatter of clods (provisional)",
        &[("density", 0.35)],
    ),
    w("some", "dirtlab's clods", &[("density", 0.7)]),
    w(
        "many",
        "crumbly, cloddy earth (provisional)",
        &[("density", 1.0)],
    ),
];

static FLOW: &[Word] = &[
    // grasslab MEANDER_AMP_DEG (10, 30) is `wavy`; the others are provisional.
    w(
        "combed",
        "blades lean together, tidily (provisional)",
        &[("meander-min", 0.0), ("meander-max", 10.0)],
    ),
    w(
        "wavy",
        "grasslab's in-phase meander",
        &[("meander-min", 10.0), ("meander-max", 30.0)],
    ),
    w(
        "wild",
        "blades every which way (provisional)",
        &[("meander-min", 25.0), ("meander-max", 60.0)],
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
        c("size", Axis::Mm, "rock width").default(size),
        c("mostly", Axis::Word, "skews the size range").words(MOSTLY),
        c("shape", Axis::Word, "river stones, broken rock, flat slabs").words(ROCK_SHAPE),
        c("wear", Axis::Word, "sharp, or chipped and rounded edges").words(WEAR),
        c(
            "buried",
            Axis::Share,
            "how much of each rock is under the ground",
        ),
    ]
}

// rocklab WIDTH_MM (6, 15): "port the labs first" (P3 alignment; was 2..10)
static ROCK: [Control; 5] = rock_controls("6..15");
static STONE: [Control; 5] = rock_controls("1..6");
static BOULDER: [Control; 5] = rock_controls("4.5..8");
static PEBBLE: [Control; 5] = rock_controls("1..2.2");

// ---- tree ---------------------------------------------------------------

static CROWN_SHAPE: &[Word] = &[
    // `round` is the Python Tree's envelope defaults; the rest are provisional.
    w(
        "round",
        "the Python tree's crown",
        &[
            ("top-pointiness", 0.0),
            ("top-curve", 1.4),
            ("bottom-pointiness", 0.35),
            ("bottom-curve", 0.8),
        ],
    ),
    w(
        "cone",
        "pointed at the top, like a fir (provisional)",
        &[
            ("top-pointiness", 1.0),
            ("top-curve", 0.8),
            ("bottom-pointiness", 0.1),
            ("bottom-curve", 1.0),
        ],
    ),
    w(
        "wide",
        "a broad, flat-bottomed spread (provisional)",
        &[
            ("top-pointiness", 0.0),
            ("top-curve", 2.0),
            ("bottom-pointiness", 0.6),
            ("bottom-curve", 0.6),
        ],
    ),
    w(
        "tall",
        "a narrow column (provisional)",
        &[
            ("top-pointiness", 0.3),
            ("top-curve", 1.0),
            ("bottom-pointiness", 0.3),
            ("bottom-curve", 1.0),
        ],
    ),
];

static BRANCHING: &[Word] = &[
    // `normal` is the Python Tree's defaults; open and dense are provisional.
    w(
        "open",
        "few long limbs (provisional)",
        &[
            ("split", 40.0),
            ("eagerness", 0.4),
            ("target", 0.25),
            ("balance", 0.8),
            ("exponent", 3.0),
            ("min-radius", 1.0),
        ],
    ),
    w(
        "normal",
        "the Python tree's branching",
        &[
            ("split", 30.0),
            ("eagerness", 0.8),
            ("target", 0.33),
            ("balance", 1.0),
            ("exponent", 3.0),
            ("min-radius", 1.0),
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
            ("min-radius", 0.9),
        ],
    ),
];

static LEAVES: &[Word] = &[
    w(
        "on",
        "separate leaves over the clumps",
        &[("leaves", 1.0), ("clumps", 1.0)],
    ),
    w(
        "clumps-only",
        "just foliage clumps",
        &[("leaves", 0.0), ("clumps", 1.0)],
    ),
    w("bare", "a winter tree", &[("leaves", 0.0), ("clumps", 0.0)]),
];

static TREE: &[Control] = &[
    c("height", Axis::Mm, "ground to the top of the crown")
        .limits(20.0, 120.0)
        .default("40"),
    group(
        "trunk",
        &[
            c("height", Axis::Mm, "bare trunk below the crown").default("30"),
            c("width", Axis::Mm, "trunk thickness at the ground"),
        ],
        "the bare trunk",
    ),
    group(
        "crown",
        &[
            c("width", Axis::Mm, "crown width").default("30"),
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
            c("clump-size", Axis::Mm, "size of each clump").default("5.5"),
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
            c("length", Axis::Mm, "leaf length").default("4.5"),
            c("width", Axis::Mm, "leaf width").default("3"),
        ],
        "one leaf",
    ),
    group(
        "branch",
        &[
            c("split", Axis::Deg, "half-angle of the cone a branch keeps"),
            c(
                "eagerness",
                Axis::Number,
                "1 splits early, 0 only near the tips",
            ),
            c(
                "target",
                Axis::Number,
                "aim point in the attractor cloud, 0 low to 1 high",
            ),
            c(
                "balance",
                Axis::Number,
                "how evenly a fork shares its attractors, 0 to 1",
            ),
            c("exponent", Axis::Number, "pipe-model exponent"),
            c("min-radius", Axis::Mm, "branch radius at a tip"),
        ],
        "the generator's six branching knobs (expert: parts files only)",
    )
    .expert(),
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
        sets: &[],
        level: Some("wall-top"),
    },
    w(
        "low",
        "half the wall top (provisional)",
        &[("height", 16.55)],
    ),
];

/// The masonry surface presets (`walls/masonry.py:_TEXTURES`): chip size,
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
        .words(WALL_HEIGHT)
        .default("full"),
    c("thickness", Axis::Mm, "wall thickness").default("7"),
    group(
        "stones",
        &[
            c("height", Axis::Mm, "course height"),
            c("length", Axis::Mm, "stone length along the wall"),
            c("texture", Axis::Word, "surface finish of each stone").words(TEXTURE),
        ],
        "stone size",
    ),
    group(
        "joints",
        &[
            c("width", Axis::Mm, "mortar gap"),
            c("depth", Axis::Mm, "how deep the gaps between stones look"),
        ],
        "the gaps between stones",
    ),
    group(
        "edges",
        &[c("rounding", Axis::Mm, "how worn the stone corners are")],
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
    c("ruin", Axis::Share, "how much has fallen"),
    c("top", Axis::Word, "castle crenellations").words(&[
        w("flat", "a flat top", &[]),
        w("battlements", "merlons and crenels", &[]),
    ]),
];

// ---- floor --------------------------------------------------------------

static FLOOR: &[Control] = &[
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
    c(
        "level",
        Axis::Level,
        "the pavement top: 7.4 is the official interior floor",
    )
    .default("7.4"),
];

// ---- cliff --------------------------------------------------------------

static ROCK_MIX: &[Word] = &[
    w(
        "mixed",
        "rockpile's run shares",
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
    w("rubble", "small broken stones", &[("stone-size", 1.0)]),
    w(
        "broken",
        "between the two (provisional)",
        &[("stone-size", 1.15)],
    ),
    w(
        "massive",
        "big masses: 1.35x rubble size",
        &[("stone-size", 1.35)],
    ),
];

static CLIFF: &[Control] = &[
    c("height", Axis::Mm, "height above the foot").default("35"),
    c("depth", Axis::Mm, "how far the rock reaches into the tile"),
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
        "stone-size",
        Axis::Share,
        "small broken stones, or big masses",
    )
    .words(STONE_SIZE)
    .default("rubble"),
    c(
        "ledges",
        Axis::Count,
        "shelves a miniature can stand on (P7)",
    )
    .limits(0.0, 2.0)
    .subs(&[c(
        "height",
        Axis::Share,
        "how far up, as a share of the cliff",
    )]),
    group(
        "shattered",
        &[
            c("patches", Axis::Count, "broken patches along the run"),
            c("width", Axis::Mm, "how wide each patch is"),
        ],
        "broken patches with more cracks and scree",
    ),
    c("top", Axis::Word, "earth lodged on the flat top")
        .words(&[
            w("bare", "bare rock", &[]),
            w("dirt", "dirt lodged in the top's joints", &[]),
        ])
        .default("bare"),
];

// ---- scatter ------------------------------------------------------------

static SCATTER: &[Control] = &[
    c("count", Axis::Count, "how many, in total"),
    c(
        "per-square",
        Axis::PerSquare,
        "how many per square of the area, or of length along a line",
    ),
    c(
        "spread",
        Axis::Mm,
        "with at=: a cluster within this distance of it",
    ),
    c(
        "reach",
        Axis::Mm,
        "with against=: how far out from the foot",
    ),
    c(
        "clumps",
        Axis::PerSquare,
        "gathers them into clumps; default spread evenly",
    ),
    c("gap", Axis::Mm, "least space between neighbours"),
    c("mostly", Axis::Word, "skews every child's size range").words(MOSTLY),
];

// ---- the table ----------------------------------------------------------

const SURFACE_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::Along];
const THING_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::At];
const RUN_PLACE: &[PlaceWord] = &[PlaceWord::In, PlaceWord::Along, PlaceWord::Side];

const fn thing(name: &'static str, about: &'static str, controls: &'static [Control]) -> Look {
    Look {
        name,
        about,
        role: Role::Thing,
        place: THING_PLACE,
        needs: &[],
        holds_scatter: false,
        controls,
    }
}

/// Every built-in look the R6 vocabulary names.  Reeds join in their
/// phase (P4); water joined in P3, floors in P5.
pub static LOOKS: &[Look] = &[
    Look {
        name: "ground",
        about: "the bare ground surface; flat at the ground level unless told",
        role: Role::Surface,
        place: SURFACE_PLACE,
        needs: &[],
        holds_scatter: true,
        controls: &[LEVEL, BUMPS, BUMP_SIZE],
    },
    Look {
        name: "dirt",
        about: "a textured earth surface: dirtlab's clods and relief",
        role: Role::Surface,
        place: SURFACE_PLACE,
        needs: &[],
        holds_scatter: true,
        controls: &[
            LEVEL,
            BUMPS,
            BUMP_SIZE,
            c("clods", Axis::Word, "lumps of earth on the surface")
                .words(CLODS)
                .default("some"),
            c(
                "stones",
                Axis::Word,
                "a few rocklab stones bedded in the dirt",
            )
            .words(STONES)
            .default("some"),
            group(
                "bank",
                &[
                    c("against", Axis::Tag, "what the bank leans on").required(),
                    c("height", Axis::Mm, "height at the face"),
                    c("reach", Axis::Mm, "how far out it runs"),
                ],
                "earth heaped against a cliff or wall",
            ),
        ],
    },
    Look {
        name: "grass",
        about: "stroke-blade grass in tufts (grasslab v3)",
        role: Role::Surface,
        place: SURFACE_PLACE,
        needs: &[],
        holds_scatter: true,
        controls: &[
            c("length", Axis::Mm, "blade length").default("10"),
            c("curl", Axis::Deg, "how far a blade bends over").default("36..81"),
            c("tufts", Axis::PerSquare, "clumps the blades gather into").default("3"),
            c("cover", Axis::Share, "share of the ground grassed over").default("100%"),
            c("flow", Axis::Word, "how tidily the blades lean together")
                .words(FLOW)
                .default("wavy"),
        ],
    },
    Look {
        name: "water",
        about: "still water with waterlab's swells: fills its area up to its level",
        role: Role::Surface,
        place: &[PlaceWord::In],
        needs: &[],
        holds_scatter: false,
        controls: &[c("level", Axis::Level, "the calm water level").default("water")],
    },
    thing("rock", "one rock", &ROCK),
    thing("stone", "a rock with stone sizes, about 1–6 mm", &STONE),
    thing(
        "boulder",
        "a rock with boulder sizes, about 4.5–8 mm",
        &BOULDER,
    ),
    thing(
        "pebble",
        "a rock with pebble sizes, about 1–2.2 mm",
        &PEBBLE,
    ),
    thing(
        "tree",
        "a space-colonisation tree with foliage clumps and leaves",
        TREE,
    ),
    Look {
        name: "wall",
        about: "a masonry wall; on a tile edge it stands inside the tile",
        role: Role::Run,
        place: RUN_PLACE,
        needs: &[PlaceWord::Along],
        holds_scatter: false,
        controls: WALL,
    },
    Look {
        name: "floor",
        about: "a stone pavement that replaces the ground: a wall laid on its side",
        role: Role::Surface,
        place: &[PlaceWord::In],
        needs: &[],
        holds_scatter: false,
        controls: FLOOR,
    },
    Look {
        name: "cliff",
        about: "a cliff of stacked rock (clifflab rockpile); one edge is a run, two a corner",
        role: Role::Run,
        place: RUN_PLACE,
        needs: &[PlaceWord::Along],
        holds_scatter: false,
        controls: CLIFF,
    },
    Look {
        name: "scatter",
        about: "more than one thing: how many and where; its children say what",
        role: Role::Scatter,
        place: &[
            PlaceWord::In,
            PlaceWord::Along,
            PlaceWord::Against,
            PlaceWord::At,
        ],
        needs: &[],
        holds_scatter: false,
        controls: SCATTER,
    },
];

/// Words from the spike vocabulary, with what replaces them (R6 V6 and
/// the rest of the 1 Nov 2026 rework).  Used for hints only.
pub static RENAMED: &[(&str, &str)] = &[
    ("hills", "`hills` is now `ground`"),
    ("meadow", "`meadow` is now `grass`"),
    (
        "scree",
        "scree is things scattered against the cliff: `scatter against=cliff per-square=55 { stone }`",
    ),
    ("blocks", "scree `blocks` are now `boulder`s in a scatter"),
    ("family", "`family` is now `style`"),
    ("self", "`self=` is now `in=`"),
    ("near", "`near=` is now `against=`"),
    ("heights", "`heights` is now `levels`, in mm"),
    ("cols", "a tile's size is `size=\"2x1\"`, columns first"),
    ("rows", "a tile's size is `size=\"2x1\"`, columns first"),
    ("tips", "a tree's branch ends are `crown clumps=…`"),
    ("canopy", "a tree's crown is `crown width=…`, in mm"),
    ("foliage", "foliage is `leaves on`, `clumps-only` or `bare`"),
    ("mass", "stone size is `stone-size rubble|broken|massive`"),
    ("dirt-on-tops", "write `top dirt`"),
    (
        "stones",
        "a cliff's mix is `rock mixed`, or `rock columns=… blocks=…`",
    ),
    ("shatter", "write `shattered patches=… width=…`"),
    (
        "form",
        "the form follows from `along=`: one edge is a run, two a corner",
    ),
    (
        "blade",
        "grass controls go on the `grass` line: `grass length=… curl=…`",
    ),
    ("organic", "a line's wobble is `line name width=… wobble=…`"),
    ("lines", "a region is `bounded-by` its lines"),
];

/// The hint for a word from the spike vocabulary.
#[must_use]
pub fn renamed(word: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(w, _)| *w == word).map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_within_each_look() {
        for l in LOOKS {
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
        for l in LOOKS {
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

    #[test]
    fn branching_words_cover_every_expert_knob() {
        let tree = crate::schema::look("tree").unwrap();
        let branch = tree.control("branch").unwrap();
        let crown = tree.control("crown").unwrap();
        for word in crown.sub("branching").unwrap().words {
            let names: Vec<_> = word.sets.iter().map(|(n, _)| *n).collect();
            let knobs: Vec<_> = branch.subs.iter().map(|k| k.name).collect();
            assert_eq!(names, knobs, "{}", word.word);
        }
    }
}
