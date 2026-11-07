//! What a document may say: the controls each built-in element accepts, the
//! words that stand for numbers, and how each kind of value scales.
//!
//! The tables live in [`crate::elements`]; this module holds
//! their types.
//!
//! The tables are the single source for `dharmatiles elements <name>`, the
//! MCP `describe_element` tool and, later, the JSON Schema: dt-doc owns them
//! so that validation never needs geometry.

/// The DungeonBlocks reference square, mm.  Authors write every size at
/// this scale; other systems scale the written sizes like a photocopy.
pub const SQUARE_MM: f64 = 35.0;

/// h: the official 33.1 mm wall top at the DungeonBlocks reference.  It is
/// the engine's vertical scale (levels are kept as fractions of it), and
/// an author may opt in to it with `(h)1.2`.
pub const H_MM: f64 = 33.1;

/// The most squares a tile has across or up.  Bigger boards are several
/// tiles; the cap also keeps `check` from planning a grid that cannot fit
/// in memory (a 64×64 tile is 8193 × 8193 height nodes).
pub const MAX_SQUARES: u32 = 64;

/// What a value measures, which fixes what may be written for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// A size in mm at the DungeonBlocks reference.  `(mm)` checks it and
    /// `(h)` writes it in wall tops.  Scales with the system.
    Mm,
    /// Where a surface sits: a named level, mm above the tile base, or an
    /// expression of levels (`"ground - 1"`).  Scales with the system.
    Level,
    /// An angle in degrees.  Never scales.
    Deg,
    /// A share, written as a percent (`"85%"`).  Never scales.
    Share,
    /// A count of things.  Never scales.
    Count,
    /// A density per square (or per square of length along a line).  It
    /// follows the square, so the number never changes.
    PerSquare,
    /// A plain number: a weight, a fraction, an exponent.
    Number,
    /// `#true` or `#false`.
    Bool,
    /// Only the control's words.
    Word,
    /// A kind of thing solids carry: `cliff`, `wall`, `rock`, `tree`.
    Tag,
}

impl Axis {
    /// True when values are numbers or expressions.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Axis::Mm
                | Axis::Level
                | Axis::Deg
                | Axis::Share
                | Axis::Count
                | Axis::PerSquare
                | Axis::Number
        )
    }

    /// How the axis reads in an error message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Axis::Mm => "a size in mm",
            Axis::Level => "a level: a named level, mm, or an expression like \"ground - 1\"",
            Axis::Deg => "an angle in degrees",
            Axis::Share => "a share, written as a percent such as \"30%\"",
            Axis::Count => "a count",
            Axis::PerSquare => "a number per square",
            Axis::Number => "a plain number",
            Axis::Bool => "#true or #false",
            Axis::Word => "one of a fixed set of words",
            Axis::Tag => "a kind of thing: cliff, wall, rock or tree",
        }
    }
}

/// A word that stands for documented numbers: `rock mixed`,
/// `crown branching=normal`, `mostly=small`.  The decoded model keeps
/// the word and these numbers, so a GUI can show both.
#[derive(Debug, PartialEq)]
pub struct Word {
    /// The word as written.
    pub word: &'static str,
    /// What it gives you, in one line.
    pub about: &'static str,
    /// The generator numbers it stands for, by name.  Empty for a plain
    /// choice whose meaning is the element's own code path.
    pub sets: &'static [(&'static str, f64)],
    /// A named level it stands for (`height full` is the `wall-top`
    /// level), resolved through the tile's levels.
    pub level: Option<&'static str>,
}

/// One control: a value, sub-controls written as properties on its own
/// line, or both (`ledges 1 height="40%"`).
#[derive(Debug, PartialEq)]
pub struct Control {
    /// Name as written.
    pub name: &'static str,
    /// What its value measures; `None` for a group, whose controls are
    /// all properties on its own line (`leaf length=4.5 width=3`).
    pub value: Option<Axis>,
    /// Words its value may also be (or, with [`Axis::Word`], must be).
    pub words: &'static [Word],
    /// Sub-controls, written as properties on the control's own line.
    pub subs: &'static [Control],
    /// Whether it must be given.
    pub required: bool,
    /// Expert controls are accepted in `define`s (parts files) and
    /// refused in tiles.
    pub expert: bool,
    /// The sensible range for a number, used to catch slips.
    pub limits: Option<(f64, f64)>,
    /// The default, as an author would write it; empty if none.
    pub default: &'static str,
    /// One line of help.
    pub about: &'static str,
}

impl Control {
    /// A sub-control by name.
    #[must_use]
    pub fn sub(&'static self, name: &str) -> Option<&'static Control> {
        self.subs.iter().find(|c| c.name == name)
    }

    /// A word by name.
    #[must_use]
    pub fn word(&'static self, word: &str) -> Option<&'static Word> {
        self.words.iter().find(|w| w.word == word)
    }

    /// The default as a decoded value: one of its words, or a number (a
    /// range for some); `None` without a default.
    #[must_use]
    pub fn default_value(&'static self) -> Option<crate::model::Value> {
        use crate::model::{Num, Value};
        if self.default.is_empty() {
            return None;
        }
        if let Some(w) = self.word(self.default) {
            return Some(Value::Word(w));
        }
        let expr = crate::expr::parse(self.default).ok()?;
        Some(Value::Num(Num { expr, unit: None }))
    }

    /// True for a group: sub-controls only, no value of its own.
    #[must_use]
    pub fn is_group(&self) -> bool {
        self.value.is_none()
    }
}

/// The place words: one meaning each, shared by every
/// element.  Which an element takes is in [`Element::place`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceWord {
    /// `in=` a region; default the whole tile.
    In,
    /// `along=` a line, an edge or an edge path.
    Along,
    /// `side=left|right|centre` of a drawn line.
    Side,
    /// `at=` a point; ranges allowed.
    At,
    /// `against=` a kind of thing: gathers at its foot.
    Against,
}

impl PlaceWord {
    /// The word as written.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PlaceWord::In => "in",
            PlaceWord::Along => "along",
            PlaceWord::Side => "side",
            PlaceWord::At => "at",
            PlaceWord::Against => "against",
        }
    }

    /// Parse a place word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "in" => PlaceWord::In,
            "along" => PlaceWord::Along,
            "side" => PlaceWord::Side,
            "at" => PlaceWord::At,
            "against" => PlaceWord::Against,
            _ => return None,
        })
    }
}

/// What kind of stack entry an element is, which decides how the evaluator
/// plans it: stack order is free for solids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Paints or shapes a surface (`ground`, `dirt`, `grass`).  Order
    /// among surfaces decides which paints over which.
    Surface,
    /// One solid thing placed with `at=` (`tree`, `rock`, …).  It can be
    /// a child of a `scatter`.
    Thing,
    /// A solid built along an edge or line (`wall`, `cliff`).
    Run,
    /// More than one thing (`scatter`): its kind is its children's.
    Scatter,
}

/// Solid or surface, for planning: every solid is planned before
/// anything grows, and surfaces grow round every solid wherever they are
/// listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Paints or shapes a surface; grows round solids.
    Surface,
    /// Builds solids, planned first.
    Solid,
    /// A definition or scatter holding both.
    Mixed,
}

impl Kind {
    /// The kind of a list of entries' kinds (`Surface` when empty).
    #[must_use]
    pub fn of(kinds: impl IntoIterator<Item = Kind>) -> Kind {
        let mut out: Option<Kind> = None;
        for k in kinds {
            out = Some(match out {
                None => k,
                Some(o) if o == k => o,
                Some(_) => Kind::Mixed,
            });
        }
        out.unwrap_or(Kind::Surface)
    }
}

/// A built-in element.
#[derive(Debug)]
pub struct Element {
    /// Name as written in a stack.
    pub name: &'static str,
    /// One line of help.
    pub about: &'static str,
    /// How the evaluator plans it.
    pub role: Role,
    /// The place words it takes, besides `seed` and `id` (which every
    /// entry takes) and `weight` (which every scatter child takes).
    pub place: &'static [PlaceWord],
    /// Place words it must have.
    pub needs: &'static [PlaceWord],
    /// True when a `scatter` may sit inside it (`dirt { scatter … }`).
    pub holds_scatter: bool,
    /// Its controls.
    pub controls: &'static [Control],
}

impl Element {
    /// A control by name.
    #[must_use]
    pub fn control(&'static self, name: &str) -> Option<&'static Control> {
        self.controls.iter().find(|c| c.name == name)
    }

    /// Solid or surface; `Mixed` for a scatter, whose kind is its
    /// children's.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self.role {
            Role::Surface => Kind::Surface,
            Role::Thing | Role::Run => Kind::Solid,
            Role::Scatter => Kind::Mixed,
        }
    }
}

/// Element up a built-in element by name.
#[must_use]
pub fn element(name: &str) -> Option<&'static Element> {
    crate::elements::ELEMENTS.iter().find(|l| l.name == name)
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
        let names = ["per-square", "clumps", "height", "branching"];
        assert_eq!(did_you_mean("per-sqare", names), Some("per-square"));
        assert_eq!(did_you_mean("hieght", names), Some("height"));
        assert_eq!(did_you_mean("wobble", names), None);
    }

    #[test]
    fn defaults_decode_as_values() {
        use crate::model::Value;
        let tree = element("tree").unwrap();
        let shape = tree.control("crown").unwrap().sub("shape").unwrap();
        assert!(matches!(shape.default_value(), Some(Value::Word(w)) if w.word == "round"));
        let height = tree.control("height").unwrap();
        assert!(matches!(height.default_value(), Some(Value::Num(_))));
        let level = element("ground").unwrap().control("level").unwrap();
        assert!(matches!(level.default_value(), Some(Value::Num(_))));
    }

    #[test]
    fn kinds_combine() {
        assert_eq!(Kind::of([Kind::Solid, Kind::Solid]), Kind::Solid);
        assert_eq!(Kind::of([Kind::Solid, Kind::Surface]), Kind::Mixed);
        assert_eq!(Kind::of([]), Kind::Surface);
    }
}
