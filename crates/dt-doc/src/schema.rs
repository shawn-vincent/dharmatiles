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

/// The OpenLOCK square, mm (one inch): the same number as dt-kit's
/// `OPENLOCK_SQUARE_MM`, here for what reads a document without geometry
/// (the browser build).
pub const OPENLOCK_SQUARE_MM: f64 = 25.4;

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
    /// A length in design millimetres (`dmm`, 1/35 of a square): the size an
    /// author writes at the DungeonBlocks reference.  Scales with the system.
    Dmm,
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
    /// A height: an expression of levels, constants and the `$` names the
    /// element sets (`$dirtEdgeHeight + 35dmm`).  Every range in it is
    /// its own roll.  Written as a `top=`; the control's help says what
    /// the element does with it.
    Height,
    /// A tile's size in squares, `2x1`.
    Size,
    /// Free text: a sentence, written as a string.
    Text,
    /// Written as it is, read by the node itself (a line's points, a fill
    /// point): the control holds the text.
    Raw,
    /// Names of other things in the tile, written one after another (the
    /// lines a region is bounded by, the regions an `except` takes out).
    Names,
}

impl Axis {
    /// True when values are numbers or expressions.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Axis::Dmm
                | Axis::Deg
                | Axis::Share
                | Axis::Count
                | Axis::PerSquare
                | Axis::Number
                | Axis::Height
        )
    }

    /// How the axis reads in an error message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Axis::Dmm => "a length, such as `8dmm` or `2mm`",
            Axis::Deg => "an angle in degrees",
            Axis::Share => "a share, written as a percent such as \"30%\"",
            Axis::Count => "a count",
            Axis::PerSquare => "a number per square",
            Axis::Number => "a plain number",
            Axis::Bool => "#true or #false",
            Axis::Word => "one of a fixed set of words",
            Axis::Tag => "a kind of thing: cliff, wall, rock or tree",
            Axis::Height => "a height, such as `$dirtEdgeHeight + 35dmm`",
            Axis::Size => "a size in squares, such as `2x1`",
            Axis::Text => "text",
            Axis::Raw => "points written as they are",
            Axis::Names => "names of things in the tile",
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
    /// What leaving it out means, in words, for a control with no default
    /// value to show: "no strip", "worked out from the run".  A control
    /// shows its default, or what its element works out (`Element::shown`),
    /// or this; a control with none of the three is a mistake in the table
    /// (`crates/dt-eval/tests/shown.rs` says so).
    pub absent: &'static str,
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

    /// The default as an author writes it in a document: a length carries `dmm` and an angle `deg` (`6..10` is
    /// `6dmm..10dmm`); a word, a count and a share are written as ever.
    #[must_use]
    pub fn default_written(&self) -> String {
        let unit = match self.value {
            Some(Axis::Dmm) => "dmm",
            Some(Axis::Deg) => "deg",
            _ => return self.default.to_owned(),
        };
        if self.words.iter().any(|w| w.word == self.default) {
            return self.default.to_owned();
        }
        // a number gets the unit after it; a name or an operator is left
        let mut out = String::new();
        let chars: Vec<char> = self.default.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // `..` is a range's dots: the number after them is a number of its own
            let word_before = i > 0
                && (chars[i - 1].is_alphanumeric()
                    || "_$".contains(chars[i - 1])
                    || (chars[i - 1] == '.' && !(i > 1 && chars[i - 2] == '.')));
            if c.is_ascii_digit() && !word_before {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    // `..` is a range's dots, not a decimal point
                    if chars[i] == '.' && chars.get(i + 1) == Some(&'.') {
                        break;
                    }
                    i += 1;
                }
                out.extend(&chars[start..i]);
                let after = chars.get(i);
                if after.is_none_or(|a| !a.is_alphabetic() && *a != '%') {
                    out.push_str(unit);
                }
            } else {
                out.push(c);
                i += 1;
            }
        }
        out
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
        if matches!(axis, Axis::Dmm | Axis::Count | Axis::PerSquare) && lo < 0.0 {
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
                Axis::Dmm => format!("{}dmm", fmt(v)),
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

/// A node of the tile's own language that is not an element: the `tile`
/// itself, a `line`, a `region` and a region's `within` and `except` clauses.
/// Its controls are the same [`Control`]s an element has, so the editor's
/// sheet is drawn from one shape.
#[derive(Debug)]
pub struct Node {
    /// The node's name as written.
    pub name: &'static str,
    /// One line of help.
    pub about: &'static str,
    /// The page of the guide (`docs/guide/…`, with an anchor) that documents it.
    pub doc: &'static str,
    /// Its controls, in the order the sheet shows them.
    pub controls: &'static [Control],
}

/// A node of the tile's language by name.
#[must_use]
pub fn node(name: &str) -> Option<&'static Node> {
    crate::elements::NODES.iter().find(|n| n.name == name)
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
    /// The family it is drawn in.
    pub family: Family,
    /// What it is as a body of the stack, if it is one: it paints the
    /// surface things stand on, and takes a `top=` and pins of its own.
    pub body: Option<Body>,
    /// True when its `top=` is a cloud worked out where the thing stands (a
    /// tree's at each branch tip, a rock's at its middle), not a band over a
    /// region.
    pub cloud_top: bool,
    /// How much room one of it takes up, if it takes room others keep clear
    /// of.
    pub footprint: Option<Footprint>,
    /// True when its `style` may be brick, which makes the tile's masonry
    /// brick when every such element is.
    pub masonry: bool,
    /// What a build of it costs, for the progress bar before it has been
    /// timed here.
    pub cost: Cost,
    /// Its controls.
    pub controls: &'static [Control],
}

/// The family an element is drawn in, for its colour in the editor and the
/// docs: what it reads as on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Ground a body lays down: dirt, floors, a cliff.
    Surface,
    /// Water.
    Water,
    /// A solid thing: rocks and walls.
    Thing,
    /// Something that grows: grass, reeds, flowers, trees.
    Plant,
}

impl Family {
    /// The family as the editor and the docs name it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Surface => "surface",
            Family::Water => "water",
            Family::Thing => "thing",
            Family::Plant => "plant",
        }
    }
}

/// What a body does with its band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Body {
    /// Land that eases between its pins at `slope_deg` unless the entry says
    /// `slope=` (dirt).
    Land {
        /// The steepest ease, degrees.
        slope_deg: f64,
    },
    /// Fluid that fills its region up to its band and settles `settle_mm` (at
    /// the reference square) over what is under it (water).
    Fluid {
        /// How far it settles, mm at the reference square.
        settle_mm: f64,
    },
    /// Separate pieces made to fit the band (stones, slabs, boards): they
    /// step between the pins, and take their own defaults wherever they are
    /// laid.
    Pieces,
}

/// The room one of a thing takes, so others keep clear: a width read from one
/// of its controls (the middle of a range), round or along a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    /// The control that says it, a `control` or `control/sub` path.
    pub control: &'static str,
    /// True for a disc (the width is its diameter), false for a strip along a
    /// line (the width is its thickness).
    pub disc: bool,
}

/// A rough cost of one build, in seconds on a tile of `s` squares:
/// `fixed + per_square * s`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cost {
    /// Whatever the tile.
    pub fixed: f64,
    /// For each square of the tile.
    pub per_square: f64,
}

impl Cost {
    /// Seconds for a tile of `squares` squares.
    #[must_use]
    pub fn secs(self, squares: f64) -> f64 {
        self.fixed + self.per_square * squares
    }
}

impl Element {
    /// What a table entry starts from: no required place words, nothing held,
    /// scattered or honoured, not a body. An entry writes what differs
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
        family: Family::Thing,
        body: None,
        cloud_top: false,
        footprint: None,
        masonry: false,
        cost: Cost {
            fixed: 0.1,
            per_square: 0.0,
        },
        controls: &[],
    };

    /// The width its footprint takes when its control is not written: the
    /// middle of the control's default, mm at the reference square.
    #[must_use]
    pub fn footprint_default_mm(&'static self) -> Option<f64> {
        let f = self.footprint?;
        let mut parts = f.control.splitn(2, '/');
        let control = self.control(parts.next()?)?;
        let control = match parts.next() {
            Some(sub) => control.sub(sub)?,
            None => control,
        };
        let (lo, hi) = crate::expr::parse(control.default)
            .ok()?
            .bounds(&|_| None)
            .ok()?;
        Some(0.5 * (lo + hi))
    }

    /// Whether it is a body of the stack (dirt, water, a floor, a cliff).
    #[must_use]
    pub fn is_body(&self) -> bool {
        self.body.is_some()
    }

    /// Whether it takes a `top=`: a band over a region (a body) or a cloud
    /// where it stands.
    #[must_use]
    pub fn has_top(&self) -> bool {
        self.body.is_some() || self.cloud_top
    }

    /// A control by name.
    #[must_use]
    pub fn control(&'static self, name: &str) -> Option<&'static Control> {
        self.controls.iter().find(|c| c.name == name)
    }

    /// The control at `path`: `height`, or `crown/width` for a sub-control.
    #[must_use]
    pub fn control_at(&'static self, path: &str) -> Option<&'static Control> {
        match path.split_once('/') {
            None => self.control(path),
            Some((group, sub)) => self.control(group)?.sub(sub),
        }
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

/// Whether a tile's masonry is brick: there is some, and every wall or floor
/// in it (an element whose schema is `masonry`) has a `style` word that says
/// brick.  `entries` is each stack entry's element name and its `style` word,
/// if it wrote one.  This is the one place the word is read: a build only
/// carries the answer (`dt_kit::Look`).
pub fn masonry_is_brick<'a>(entries: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> bool {
    let mut any = false;
    for (name, style) in entries {
        if element(name).is_some_and(|l| l.masonry) {
            if !style.is_some_and(|w| w.contains("brick")) {
                return false;
            }
            any = true;
        }
    }
    any
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
        let height = tree.control("trunk").unwrap().sub("height").unwrap();
        assert!(matches!(height.default_value(), Some(Value::Num(_))));
        let boards = element("planks").unwrap().control("boards").unwrap();
        assert!(matches!(boards.default_value(), Some(Value::Num(_))));
    }

    #[test]
    fn a_default_written_carries_its_unit_at_both_ends() {
        let rock = element("rock").unwrap();
        let size = rock.control("size").unwrap();
        assert_eq!(size.default_written(), "6dmm..15dmm");
        let crown = element("tree").unwrap().control("crown").unwrap();
        assert_eq!(crown.sub("shape").unwrap().default_written(), "round");
    }

    #[test]
    fn an_element_takes_a_top_control_exactly_when_the_schema_says_it_has_a_top() {
        use crate::elements::ELEMENTS;
        for e in ELEMENTS {
            let top = e.control("top").filter(|c| c.value == Some(Axis::Height));
            assert_eq!(top.is_some(), e.has_top(), "{}", e.name);
        }
        let bodies: Vec<&str> = ELEMENTS
            .iter()
            .filter(|e| e.is_body())
            .map(|e| e.name)
            .collect();
        assert_eq!(bodies, ["dirt", "water", "flagstones", "planks", "cliff"]);
        let clouds: Vec<&str> = ELEMENTS
            .iter()
            .filter(|e| e.cloud_top)
            .map(|e| e.name)
            .collect();
        assert_eq!(clouds, ["rock", "stone", "boulder", "pebble", "tree"]);
    }

    #[test]
    fn a_footprint_names_a_control_with_a_default_to_fall_back_on() {
        use crate::elements::ELEMENTS;
        for e in ELEMENTS.iter().filter(|e| e.footprint.is_some()) {
            assert!(
                e.footprint_default_mm().is_some_and(|w| w > 0.0),
                "{}",
                e.name
            );
        }
        assert_eq!(element("wall").unwrap().footprint_default_mm(), Some(7.0));
        assert_eq!(element("tree").unwrap().footprint_default_mm(), Some(30.0));
    }

    #[test]
    fn kinds_combine() {
        assert_eq!(Kind::of([Kind::Solid, Kind::Solid]), Kind::Solid);
        assert_eq!(Kind::of([Kind::Solid, Kind::Surface]), Kind::Mixed);
        assert_eq!(Kind::of([]), Kind::Surface);
    }
}
