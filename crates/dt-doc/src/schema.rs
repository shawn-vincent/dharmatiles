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
            Axis::Mm | Axis::Deg | Axis::Share | Axis::Count | Axis::PerSquare | Axis::Number
        )
    }

    /// How the axis reads in an error message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Axis::Mm => "a size in mm",
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
/// `crown branching=normal`, `rock shape=angular`.  The decoded model keeps
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
    /// The numbers it takes, as the element reads them: mm at the
    /// reference square, a share as a fraction.  `None` for any number its
    /// axis allows.
    pub limits: Option<Limits>,
    /// True when it takes a range (`size="6..15"`), drawn from per thing.
    pub ranges: bool,
    /// True when it also takes a weighted range (`size="low(1..6, 4)"`):
    /// the odds along the range.  Implies `ranges`.
    pub weighted: bool,
    /// Part of the language, not built yet: `check` accepts it, a build
    /// refuses it.  A sub-control of a planned control is planned too.
    pub planned: bool,
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

    /// The numbers it takes, for a table or a hint: "20 to 120",
    /// "0% to 100%", "over 0", with ", or a range" when it takes one;
    /// `None` when its axis says it all.
    #[must_use]
    pub fn takes(&self) -> Option<String> {
        let share = self.value == Some(Axis::Share);
        let shown = |v: f64| {
            if share {
                format!("{}%", fmt(v * 100.0))
            } else {
                fmt(v)
            }
        };
        let limits = self.limits.map(|l| l.describe(shown));
        match (limits, self.ranges, self.weighted) {
            (Some(l), true, true) => Some(format!("{l}, or a range, or a weighted range")),
            (None, true, true) => Some("a number, a range or a weighted range".to_owned()),
            (Some(l), true, false) => Some(format!("{l}, or a range")),
            (Some(l), false, _) => Some(l),
            (None, true, false) => Some("a number or a range".to_owned()),
            (None, false, _) => None,
        }
    }

    /// True for a group: sub-controls only, no value of its own.
    #[must_use]
    pub fn is_group(&self) -> bool {
        self.value.is_none()
    }
}

/// The numbers a control takes: from `min` (or over it, when `above`) up
/// to `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The low end.
    pub min: f64,
    /// The high end; infinite for no high end.
    pub max: f64,
    /// True when `min` itself is refused: "over 0".
    pub above: bool,
}

impl Limits {
    /// True when `v` is one of the numbers.
    #[must_use]
    pub fn holds(&self, v: f64) -> bool {
        v.is_finite()
            && (if self.above {
                v > self.min
            } else {
                v >= self.min
            })
            && v <= self.max
    }

    /// The numbers in words, each shown by `show`: "1 to 6", "over 0",
    /// "over 0, up to 200".
    #[must_use]
    pub fn describe(&self, show: impl Fn(f64) -> String) -> String {
        let (lo, hi) = (show(self.min), show(self.max));
        match (self.above, self.max.is_finite()) {
            (false, true) => format!("{lo} to {hi}"),
            (false, false) => format!("{lo} or more"),
            (true, true) => format!("over {lo}, up to {hi}"),
            (true, false) => format!("over {lo}"),
        }
    }
}

/// A number as an author writes it: no trailing zeros.
fn fmt(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "∞" } else { "-∞" }.to_owned();
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Why a value is refused: the message, the label at the value, and what
/// to write instead.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// What is wrong.
    pub message: String,
    /// A word or two at the value.
    pub label: &'static str,
    /// What to write instead.
    pub help: Option<String>,
}

impl Control {
    /// The one check of a number written for this control, `lo..hi` (one
    /// number has `lo == hi`), with names already resolved, in the units
    /// the element reads.  `range` is true when it was written as a range.
    /// `owner` and the control's name word the message: `tree height`.
    ///
    /// # Errors
    /// The [`Refusal`] for a value the control does not take.
    pub fn check(&self, owner: &str, lo: f64, hi: f64, range: bool) -> Result<(), Refusal> {
        check(
            self.value.unwrap_or(Axis::Number),
            self.limits,
            self.ranges,
            (owner, self.name),
            (lo, hi, range),
        )
    }
}

/// [`Control::check`], for a value whose control is known only by its
/// axis (then it takes any range of numbers the axis allows).
///
/// # Errors
/// The [`Refusal`] for a value the axis does not take.
pub fn check(
    axis: Axis,
    limits: Option<Limits>,
    ranges: bool,
    (owner, name): (&str, &str),
    (lo, hi, range): (f64, f64, bool),
) -> Result<(), Refusal> {
    {
        let refuse = |message: String, label, help: Option<String>| {
            Err(Refusal {
                message,
                label,
                help,
            })
        };
        if range && !ranges {
            return refuse(
                format!("`{owner} {name}` takes one number, not a range"),
                "a range",
                Some(format!("write one number, such as {name}={}", fmt(lo))),
            );
        }
        if lo > hi {
            return refuse(
                "a range is written low end first".to_owned(),
                "high..low",
                Some(format!("write \"{}..{}\"", fmt(hi), fmt(lo))),
            );
        }
        if matches!(axis, Axis::Mm | Axis::Count | Axis::PerSquare) && lo < 0.0 {
            return refuse(
                format!("`{owner} {name}` can't be negative"),
                "below zero",
                None,
            );
        }
        if axis == Axis::Count && (lo.fract() != 0.0 || hi.fract() != 0.0) {
            return refuse(
                format!("`{owner} {name}` is a count of whole things"),
                "not whole",
                Some("perSquare densities may be fractions; counts can't".to_owned()),
            );
        }
        if let Some(l) = limits
            && !(l.holds(lo) && l.holds(hi))
        {
            let shown = |v: f64| match axis {
                Axis::Share => format!("{}%", fmt(v * 100.0)),
                Axis::Mm => format!("{} mm", fmt(v)),
                _ => fmt(v),
            };
            let limits = l.describe(shown);
            let written = if range {
                format!("{}..{}", shown(lo), shown(hi))
            } else {
                shown(lo)
            };
            return refuse(
                format!("`{owner} {name}` is {limits}, not {written}"),
                "out of range",
                Some(format!("`{name}` takes {limits}")),
            );
        }
        Ok(())
    }
}

/// What kinds of place an element takes, and the one other place word,
/// `side=`.  A place is written `at=`, whatever its kind: the kinds are
/// what [`Element::place`] lists, and the word the user writes for any of
/// them is `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceWord {
    /// `at=` a region; default the whole tile.
    In,
    /// `at=` a line, an edge or an edge path.
    Along,
    /// `side=left|right|centre` of a drawn line.
    Side,
    /// `at=` a point; ranges allowed.
    At,
    /// `at=foot(cliff)`: the band at a kind of thing's foot.
    Against,
}

impl PlaceWord {
    /// The word as written: every kind of place is written `at`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PlaceWord::Side => "side",
            _ => "at",
        }
    }

    /// What this kind of place is called: the kinds `at=` takes are a
    /// region, a line, a point and the foot of a thing.
    #[must_use]
    pub fn kind(self) -> &'static str {
        match self {
            PlaceWord::In => "region",
            PlaceWord::Along => "line",
            PlaceWord::At => "point",
            PlaceWord::Against => "foot(…)",
            PlaceWord::Side => "side",
        }
    }

    /// Parse a place word as written: `at` and `side` only; `in`, `along`
    /// and `against` are gone.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "side" => PlaceWord::Side,
            "at" => PlaceWord::At,
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
    /// The kinds of place its build honours so far, a region aside (every
    /// element acts in its region); the rest of [`Element::place`] is
    /// planned.
    pub honours: &'static [PlaceWord],
    /// True when a `scatter` may scatter it (built so far).
    pub scattered: bool,
    /// True for a body of separate pieces made to fit its band (stones,
    /// slabs, boards): it steps between its pins, and takes its own defaults
    /// wherever it is laid.
    pub pieces: bool,
    /// Its controls.
    pub controls: &'static [Control],
}

impl Element {
    /// What a table entry starts from: no required place words, nothing held,
    /// scattered or honoured, not a pieces body. An entry writes what differs
    /// and takes `..Element::BASE`.
    pub const BASE: Self = Self {
        name: "",
        about: "",
        role: Role::Thing,
        place: &[],
        needs: &[],
        holds_scatter: false,
        honours: &[],
        scattered: false,
        pieces: false,
        controls: &[],
    };

    /// A control by name.
    #[must_use]
    pub fn control(&'static self, name: &str) -> Option<&'static Control> {
        self.controls.iter().find(|c| c.name == name)
    }

    /// Whether a build takes the control at `path` (`height`, or
    /// `crown/width` for a sub-control): one the schema has, not planned.
    #[must_use]
    pub fn takes(&'static self, path: &str) -> bool {
        let mut parts = path.splitn(2, '/');
        let Some(c) = parts.next().and_then(|n| self.control(n)) else {
            return false;
        };
        match parts.next() {
            None => !c.planned,
            Some(sub) => !c.planned && c.sub(sub).is_some_and(|s| !s.planned),
        }
    }

    /// The place words it is written with, each once (`at`, `side`).
    #[must_use]
    pub fn place_words(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for w in self.place {
            if !out.contains(&w.as_str()) {
                out.push(w.as_str());
            }
        }
        out
    }

    /// The kinds of place `at=` takes here: `region`, `line`, `point`,
    /// `foot(…)`.
    #[must_use]
    pub fn place_kinds(&self) -> Vec<&'static str> {
        [
            PlaceWord::In,
            PlaceWord::Along,
            PlaceWord::At,
            PlaceWord::Against,
        ]
        .into_iter()
        .filter(|w| self.place.contains(w))
        .map(PlaceWord::kind)
        .collect()
    }

    /// Whether its build honours the place word `w` (a region always).
    #[must_use]
    pub fn honours(&self, w: PlaceWord) -> bool {
        w == PlaceWord::In || self.honours.contains(&w)
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
        let names = ["perSquare", "clumps", "height", "branching"];
        assert_eq!(did_you_mean("per-sqare", names), Some("perSquare"));
        assert_eq!(did_you_mean("hieght", names), Some("height"));
        assert_eq!(did_you_mean("wobble", names), None);
    }

    #[test]
    fn defaults_decode_as_values() {
        use crate::model::Value;
        let tree = element("tree").unwrap();
        let shape = tree.control("crown").unwrap().sub("shape").unwrap();
        assert!(matches!(shape.default_value(), Some(Value::Word(w)) if w.word == "round"));
        let height = tree.control("size").unwrap();
        assert!(matches!(height.default_value(), Some(Value::Num(_))));
        let boards = element("planks").unwrap().control("boards").unwrap();
        assert!(matches!(boards.default_value(), Some(Value::Num(_))));
    }

    #[test]
    fn kinds_combine() {
        assert_eq!(Kind::of([Kind::Solid, Kind::Solid]), Kind::Solid);
        assert_eq!(Kind::of([Kind::Solid, Kind::Surface]), Kind::Mixed);
        assert_eq!(Kind::of([]), Kind::Surface);
    }
}
