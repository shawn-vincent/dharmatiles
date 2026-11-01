//! The controls each built-in look accepts.
//!
//! This is the draft for the looks the three R6 tiles use.  In P1 each
//! look's table moves next to its Rust type in dt-looks' public model and
//! also feeds the JSON Schema, `dharmatiles looks <name>` and the MCP
//! `describe_look` tool.

use crate::expr::Unit;

/// What a control measures, which fixes the unit a document may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Layout across, in squares.
    Across,
    /// Layout up, in h (fractions of the wall), or a named height.
    Up,
    /// A print-scale detail, in true millimetres.
    Mm,
    /// An angle, in degrees.
    Deg,
    /// A share: a plain fraction or a percent.
    Share,
    /// A plain number: a count or a multiplier.
    Number,
    /// A density, per square.
    PerSquare,
    /// `#true` or `#false`.
    Bool,
    /// One of a fixed set of words.
    Choice(&'static [&'static str]),
    /// The name of a line in this definition.
    Line,
    /// The name of a region in this definition.
    Region,
    /// A kind tag that solids carry (`cliff`, `rock`, `wall`, …).
    Tag,
    /// Free text.
    Text,
}

impl Axis {
    /// Units a document may write for this axis.  The first is the default.
    #[must_use]
    pub fn units(self) -> &'static [Unit] {
        match self {
            Axis::Across => &[Unit::Sq],
            Axis::Up => &[Unit::H],
            Axis::Mm => &[Unit::Mm],
            Axis::Deg => &[Unit::Deg],
            Axis::Share => &[Unit::Percent],
            _ => &[],
        }
    }

    /// How the axis reads in an error message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Axis::Across => "a length across, in squares",
            Axis::Up => "a height, in h (fractions of the wall) or a named height",
            Axis::Mm => "a print-scale size, in true millimetres",
            Axis::Deg => "an angle, in degrees",
            Axis::Share => "a share, as a fraction or a percent",
            Axis::Number => "a plain number",
            Axis::PerSquare => "a count per square",
            Axis::Bool => "#true or #false",
            Axis::Choice(_) => "one of a fixed set of words",
            Axis::Line => "the name of a line",
            Axis::Region => "the name of a region",
            Axis::Tag => "a kind tag",
            Axis::Text => "text",
        }
    }

    /// True when values are expressions (numbers, ranges, arithmetic).
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Axis::Across
                | Axis::Up
                | Axis::Mm
                | Axis::Deg
                | Axis::Share
                | Axis::Number
                | Axis::PerSquare
        )
    }
}

/// One control.
#[derive(Debug)]
pub struct Control {
    /// Name as written in documents.
    pub name: &'static str,
    /// What it measures.
    pub axis: Axis,
    /// Whether a call must give it.
    pub required: bool,
    /// One line of help.
    pub about: &'static str,
}

/// A named group of controls written as a child node (`leaf length=4.5`).
#[derive(Debug)]
pub struct Group {
    /// Name as written.
    pub name: &'static str,
    /// One line of help.
    pub about: &'static str,
    /// Its controls.
    pub controls: &'static [Control],
}

/// A built-in look.
#[derive(Debug)]
pub struct Look {
    /// Name as written in a stack.
    pub name: &'static str,
    /// One line of help.
    pub about: &'static str,
    /// Its top-level controls.
    pub controls: &'static [Control],
    /// Its groups.
    pub groups: &'static [Group],
}

const fn c(name: &'static str, axis: Axis, about: &'static str) -> Control {
    Control {
        name,
        axis,
        required: false,
        about,
    }
}

const fn req(name: &'static str, axis: Axis, about: &'static str) -> Control {
    Control {
        name,
        axis,
        required: true,
        about,
    }
}

/// Controls every stack entry accepts, look or definition.
pub static ENTRY: &[Control] = &[
    c(
        "self",
        Axis::Region,
        "the region this entry acts on; default: the caller's",
    ),
    c(
        "seed",
        Axis::Number,
        "pin this entry's seed instead of deriving it from its path",
    ),
    c(
        "id",
        Axis::Text,
        "a stable name among same-named siblings, for seeds",
    ),
];

/// Kind tags built-ins put on their solids.
pub static TAGS: &[&str] = &[
    "ground", "dirt", "rock", "scree", "cliff", "grass", "reed", "tree", "wall", "floor", "water",
];

const WALL_FAMILIES: &[&str] = &["cut-stone", "fieldstone", "brick", "regular-brick"];
const CLIFF_FORMS: &[&str] = &[
    "run",
    "corner",
    "canyon",
    "escarpment",
    "plateau",
    "massif",
    "mound",
    "spire",
    "needles",
];

/// The built-in looks used by the R6 tiles.
pub static LOOKS: &[Look] = &[
    Look {
        name: "hills",
        about: "the ground itself: a height, with variability up and down",
        controls: &[
            c("height", Axis::Up, "the ground's mean height"),
            c("up", Axis::Up, "how far it may rise above that"),
            c("down", Axis::Up, "how far it may fall below that"),
        ],
        groups: &[],
    },
    Look {
        name: "dirt",
        about: "bare earth: dirtlab's clods, pebbles and relief",
        controls: &[c("height", Axis::Up, "where the earth's surface sits")],
        groups: &[Group {
            name: "bank",
            about: "earth banked up against something tall",
            controls: &[
                req("near", Axis::Tag, "what the bank leans on"),
                c("height", Axis::Mm, "height at the face"),
                c("reach", Axis::Mm, "how far out it runs"),
            ],
        }],
    },
    Look {
        name: "meadow",
        about: "grass: stroke blades in flowing clumps (grasslab v3)",
        controls: &[c("bare", Axis::Share, "share of ground left bare")],
        groups: &[
            Group {
                name: "blade",
                about: "one blade",
                controls: &[
                    c("length", Axis::Mm, "blade length"),
                    c("curl", Axis::Deg, "how far a blade turns along its length"),
                    c(
                        "clearance",
                        Axis::Mm,
                        "gap between a blade and what it lies on",
                    ),
                ],
            },
            Group {
                name: "clumps",
                about: "how blades gather",
                controls: &[c("per-square", Axis::PerSquare, "clumps per square")],
            },
            Group {
                name: "flow",
                about: "how the blades lean across the tile",
                controls: &[
                    c("swing", Axis::Deg, "how far the flow direction swings"),
                    c("meander", Axis::Deg, "in-phase meander"),
                    c("downhill", Axis::Share, "pull toward downhill"),
                ],
            },
        ],
    },
    Look {
        name: "tree",
        about: "a space-colonisation tree with foliage clusters and organic leaves",
        controls: &[
            c(
                "per-square",
                Axis::PerSquare,
                "trees per square of the region",
            ),
            c("count", Axis::Number, "trees in the region"),
            c(
                "tips",
                Axis::Number,
                "branch tips (attractors); each branch ends on one",
            ),
            c("height", Axis::Up, "overall height"),
            c("trunk", Axis::Up, "height of the bare trunk"),
            c("canopy", Axis::Across, "canopy radius"),
            c("foliage", Axis::Bool, "a leafy cluster on every tip"),
            c("leaves", Axis::Bool, "separate leaves over the foliage"),
        ],
        groups: &[
            Group {
                name: "leaf",
                about: "one leaf",
                controls: &[
                    c("length", Axis::Mm, "leaf length"),
                    c("width", Axis::Mm, "leaf width"),
                    c("thickness", Axis::Mm, "leaf thickness"),
                    c("fold", Axis::Deg, "fold along the midrib"),
                    c("curl", Axis::Deg, "curl toward the tip; capped at 32"),
                ],
            },
            Group {
                name: "branch",
                about: "how the skeleton branches",
                controls: &[
                    c("split", Axis::Deg, "half-angle of the cone a branch keeps"),
                    c(
                        "eagerness",
                        Axis::Share,
                        "1 splits early, 0 only near the tips",
                    ),
                    c(
                        "target",
                        Axis::Share,
                        "aim point in the attractor cloud, low to high",
                    ),
                    c(
                        "balance",
                        Axis::Share,
                        "how evenly a fork shares attractors",
                    ),
                    c("exponent", Axis::Number, "pipe-model exponent"),
                    c("min-radius", Axis::Mm, "radius at a tip"),
                ],
            },
            Group {
                name: "debug",
                about: "diagnostic output",
                controls: &[c("attractors", Axis::Bool, "show attractors as markers")],
            },
        ],
    },
    Look {
        name: "wall",
        about: "a masonry wall standing on the left of a line",
        controls: &[
            req("along", Axis::Line, "the line the wall stands on"),
            c(
                "family",
                Axis::Choice(WALL_FAMILIES),
                "how the stones are made and laid",
            ),
            c("top", Axis::Up, "height of the wall top above the datum"),
            c(
                "height",
                Axis::Up,
                "height above what it stands on; overrides top",
            ),
            c("thickness", Axis::Across, "wall thickness"),
            c(
                "coping",
                Axis::Choice(&["perched", "vertical"]),
                "fieldstone: what tops it",
            ),
            c(
                "bond",
                Axis::Choice(&["irregular", "running"]),
                "how courses and bays line up",
            ),
            c("ruin", Axis::Share, "how much has fallen"),
            c(
                "crenellated",
                Axis::Bool,
                "merlons and crenels along the top",
            ),
        ],
        groups: &[Group {
            name: "stones",
            about: "the size and finish of each stone",
            controls: &[
                c("course", Axis::Mm, "course height"),
                c("bay", Axis::Mm, "stone length along the wall"),
                c("joint", Axis::Mm, "gap between stones"),
                c("reveal", Axis::Mm, "how far stones stand out of the core"),
                c("roundover", Axis::Mm, "edge rounding radius"),
                c("relief", Axis::Mm, "face relief depth"),
            ],
        }],
    },
    Look {
        name: "cliff",
        about: "a cliff stacked from rock masses under gravity (clifflab rockpile)",
        controls: &[
            req("along", Axis::Line, "the line the cliff stands against"),
            c("form", Axis::Choice(CLIFF_FORMS), "the preset plan form"),
            c("height", Axis::Up, "top of the highest tier above the foot"),
            c(
                "mass",
                Axis::Number,
                "stone size multiplier: 1 is rubble, 1.4 massive",
            ),
            c("dirt-on-tops", Axis::Bool, "dirt lodged on the flat top"),
        ],
        groups: &[
            Group {
                name: "stones",
                about: "the mix of stone kinds",
                controls: &[
                    c("column", Axis::Share, "tall upright stones"),
                    c("block", Axis::Share, "chunky blocks"),
                    c("slab", Axis::Share, "thin slabs lying tilted"),
                    c("odd", Axis::Share, "irregular lumps"),
                    c("monolith", Axis::Share, "ground-to-top stones"),
                ],
            },
            Group {
                name: "shatter",
                about: "zones where detail gathers",
                controls: &[
                    c(
                        "zones",
                        Axis::PerSquare,
                        "shattered zones per square of run",
                    ),
                    c("width", Axis::Mm, "how wide each zone is along the run"),
                ],
            },
        ],
    },
    Look {
        name: "scree",
        about: "fallen stones gathered at the foot of something",
        controls: &[req("near", Axis::Tag, "what the scree falls from")],
        groups: &[
            Group {
                name: "stones",
                about: "loose stones",
                controls: &[
                    c("per-square", Axis::PerSquare, "stones per square of foot"),
                    c("size", Axis::Mm, "stone size"),
                    c("reach", Axis::Mm, "mean distance out from the face"),
                ],
            },
            Group {
                name: "blocks",
                about: "big fallen blocks against the face",
                controls: &[
                    c("per-square", Axis::PerSquare, "blocks per square of foot"),
                    c("size", Axis::Mm, "block size"),
                ],
            },
            Group {
                name: "grit",
                about: "small stones half-buried in the bank",
                controls: &[
                    c("per-square", Axis::PerSquare, "grit per square"),
                    c("size", Axis::Mm, "grit size"),
                    c(
                        "sink",
                        Axis::Share,
                        "how deep each is buried, as a share of its size",
                    ),
                ],
            },
            Group {
                name: "kinds",
                about: "the mix of fragment shapes",
                controls: &[
                    c("block", Axis::Share, "blocky fragments"),
                    c("wedge", Axis::Share, "wedges"),
                    c("slab", Axis::Share, "flat slabs"),
                ],
            },
        ],
    },
];

/// Look up a built-in look by name.
#[must_use]
pub fn look(name: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|l| l.name == name)
}

/// The closest candidate to `word`, if one is close enough to suggest.
pub fn did_you_mean<'a>(
    word: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = (word.chars().count() / 3).max(1) + 1;
    candidates
        .into_iter()
        .map(|cand| (edit_distance(word, cand), cand))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, cand)| cand)
}

/// Levenshtein distance, counting a swap of two neighbours as one edit.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions() {
        let names = ["per-square", "tips", "height", "foliage"];
        assert_eq!(did_you_mean("per-sqare", names), Some("per-square"));
        assert_eq!(did_you_mean("hieght", names), Some("height"));
        assert_eq!(did_you_mean("wobble", names), None);
    }

    #[test]
    fn every_look_has_unique_control_names() {
        for l in LOOKS {
            let mut names: Vec<_> = l.controls.iter().map(|c| c.name).collect();
            names.extend(l.groups.iter().map(|g| g.name));
            let n = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), n, "{}", l.name);
        }
    }
}
