//! The print list: a `.dmap` document that lists tiles to print and how to
//! lay them on plates.
//!
//! ```kdl
//! dharmatiles version="1.0.0"
//! map {
//!     piece "lib:tiles/ground/1x1-grass" count=8
//!     piece "lib:tiles/water/1x1-water-corridor+grass" count=4 seed=12
//!     piece "home:meadows/glade" count=1 system="ol"
//! }
//! plates printer="bambu/x1c-ams" system="db" mode=multicolour group=filament gap="3mm" margin="5mm"
//! ```
//!
//! A print list is a map whose pieces have no place yet.  A piece with `at`
//! is placed on the map: `at="(2sq, 1sq)"` is across and down from the map's
//! north-west corner (in squares of the piece's own system, or `mm`), `rot` is
//! 0, 90, 180 or 270 clockwise, `bottom` is the height above the map's ground
//! of the tile's zero (ground level on the tile; a `$` name or a length, 0 by
//! default, so the connector is under the ground), and a placed piece is one
//! piece (no `count`).  The
//! map may say `grid cols=8 rows=5` and `let $plinth "20dmm"` (a name for
//! heights).  The `plates` line names the printer profile, the base system,
//! the print mode and the spacing, every word of it optional, and may hold
//! `pin p4 plate=1 at="(0mm, 5mm)"` lines: pieces the packer keeps where they
//! were dropped.  Decoding keeps spans and goes on past a mistake, as a
//! tile's does, so a file's mistakes come together.
//!
//! This module reads the document and nothing else: resolving a piece's tile
//! path, building and packing are the evaluator's.

use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::decode::{arg_str, syntax, value_span};
use crate::error::{DocError, Src};
use crate::model::Spanned;
use crate::schema::did_you_mean;

/// The file extension of a print list.
pub const EXT: &str = "dmap";

/// The printer a print list that names none is laid out for: the X1 Carbon
/// with an AMS (the page's question 15, answered by its recommendation).
pub const DEFAULT_PRINTER: &str = "bambu/x1c-ams";

/// A length written on the map: squares or millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Len {
    /// Squares of the piece's own system (`2sq`).
    Sq(f64),
    /// Millimetres (`35mm`).
    Mm(f64),
}

impl Len {
    /// In millimetres, for a square of `square_mm`.
    #[must_use]
    pub fn mm(self, square_mm: f64) -> f64 {
        match self {
            Self::Sq(v) => v * square_mm,
            Self::Mm(v) => v,
        }
    }

    /// As written: `2sq`, `2.5sq`, `35mm`.
    #[must_use]
    pub fn word(self) -> String {
        let (v, u) = match self {
            Self::Sq(v) => (v, "sq"),
            Self::Mm(v) => (v, "mm"),
        };
        format!("{}{u}", trim_num(v))
    }
}

/// A number with no trailing zeros: `2`, `2.5`, `0.125`.
fn trim_num(v: f64) -> String {
    let t = format!("{v:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-" || t == "-0" {
        "0".to_owned()
    } else {
        t.to_owned()
    }
}

/// A place: across, then down, from a north-west corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct At {
    /// Across (east).
    pub x: Len,
    /// Down (south).
    pub y: Len,
}

impl At {
    /// As written: `(2sq, 1sq)`.
    #[must_use]
    pub fn text(self) -> String {
        format!("({}, {})", self.x.word(), self.y.word())
    }
}

/// Read `(2sq, 1sq)` or `(0mm, 35mm)`.
///
/// # Errors
/// What is wrong with it, in words.
pub fn parse_at(s: &str) -> Result<At, String> {
    let inner = s
        .trim()
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .ok_or("a place is a pair in brackets, like `\"(2sq, 1sq)\"`")?;
    let (a, b) = inner
        .split_once(',')
        .ok_or("a place is two lengths with a comma between, like `\"(2sq, 1sq)\"`")?;
    let len = |t: &str| -> Result<Len, String> {
        let t = t.trim();
        let (n, sq) = if let Some(n) = t.strip_suffix("sq") {
            (n, true)
        } else if let Some(n) = t.strip_suffix("mm") {
            (n, false)
        } else {
            return Err(format!("`{t}` needs a unit: `sq` or `mm`"));
        };
        let v: f64 = n
            .trim()
            .parse()
            .map_err(|_| format!("`{t}` is not a length like `2sq` or `35mm`"))?;
        if !v.is_finite() || v.abs() > 100_000.0 {
            return Err(format!("`{t}` is out of range"));
        }
        Ok(if sq { Len::Sq(v) } else { Len::Mm(v) })
    };
    Ok(At {
        x: len(a)?,
        y: len(b)?,
    })
}

/// A height as a sum: `$wallHeight`, `20dmm`, `$groundHeight + 2mm`.
#[derive(Debug, Clone, PartialEq)]
pub struct Height {
    /// Each term with its sign.
    pub terms: Vec<(f64, HeightTerm)>,
}

/// One term of a [`Height`].
#[derive(Debug, Clone, PartialEq)]
pub enum HeightTerm {
    /// A `$` name, without the `$`.
    Name(String),
    /// A number and its unit (`dmm`, `mm`, `sq`).
    Num(f64, String),
}

/// Read a height.
///
/// # Errors
/// What is wrong with it, in words.
pub fn parse_height(s: &str) -> Result<Height, String> {
    let mut terms = Vec::new();
    let mut rest = s.trim();
    if rest.is_empty() {
        return Err("a height is a `$` name or a length like `20dmm`".to_owned());
    }
    let mut sign = 1.0;
    let mut first = true;
    loop {
        if let Some(r) = rest.strip_prefix('-') {
            sign = -sign;
            rest = r.trim_start();
        } else if !first && let Some(r) = rest.strip_prefix('+') {
            rest = r.trim_start();
        }
        let end = rest.find([' ', '+', '-']).filter(|&i| i > 0);
        let (tok, tail) = end.map_or((rest, ""), |i| rest.split_at(i));
        if let Some(n) = tok.strip_prefix('$') {
            if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("`{tok}` is not a name"));
            }
            terms.push((sign, HeightTerm::Name(n.to_owned())));
        } else {
            let unit = ["dmm", "mm", "sq"].into_iter().find(|u| tok.ends_with(u));
            let Some(u) = unit else {
                return Err(format!(
                    "`{tok}` needs a unit (`dmm`, `mm` or `sq`), or is a `$` name"
                ));
            };
            let v: f64 = tok[..tok.len() - u.len()]
                .parse()
                .map_err(|_| format!("`{tok}` is not a length like `20dmm`"))?;
            terms.push((sign, HeightTerm::Num(v, u.to_owned())));
        }
        rest = tail.trim_start();
        if rest.is_empty() {
            break;
        }
        sign = 1.0;
        first = false;
        if !(rest.starts_with('+') || rest.starts_with('-')) {
            return Err(format!(
                "`{rest}` is not part of a height: join terms with + or -"
            ));
        }
    }
    Ok(Height { terms })
}

impl Height {
    /// The height in millimetres for a square of `square_mm`: `dmm` is the
    /// 35 mm reference scaled with the system, `mm` is as printed.  `name`
    /// gives a `$` name's value in millimetres.
    ///
    /// # Errors
    /// A name nothing defines.
    pub fn mm(
        &self,
        square_mm: f64,
        name: &dyn Fn(&str) -> Result<f64, String>,
    ) -> Result<f64, String> {
        let mut total = 0.0;
        for (sign, t) in &self.terms {
            let v = match t {
                HeightTerm::Name(n) => name(n)?,
                HeightTerm::Num(v, u) => match u.as_str() {
                    "dmm" => v * square_mm / crate::schema::SQUARE_MM,
                    "sq" => v * square_mm,
                    _ => *v,
                },
            };
            total += sign * v;
        }
        Ok(total)
    }
}

/// The map's grid, in squares (`grid cols=8 rows=5`).
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    /// Squares across.
    pub cols: u32,
    /// Squares down.
    pub rows: u32,
    /// The line.
    pub span: SourceSpan,
}

/// A name the map defines for heights: `let $plinth "20dmm"`.
#[derive(Debug, Clone)]
pub struct Let {
    /// The name, without the `$`.
    pub name: Spanned<String>,
    /// What it stands for, as written.
    pub expr: Spanned<String>,
}

/// A pin: a piece kept where it was dropped on a plate.
#[derive(Debug, Clone)]
pub struct Pin {
    /// The piece's id.
    pub id: Spanned<String>,
    /// The plate, from 1.
    pub plate: Spanned<u32>,
    /// Where its footprint's north-west corner stands on the bed, across and
    /// down from the bed's north-west corner.
    pub at: Option<(At, SourceSpan)>,
    /// Turned a quarter.
    pub turn: bool,
    /// The line.
    pub span: SourceSpan,
}

/// A tile to print.
#[derive(Debug, Clone)]
pub struct Piece {
    /// Its name, if written (`piece p1 "…"`): what a pin or the map names it by.
    pub id: Option<Spanned<String>>,
    /// The tile: `lib:tiles/ground/1x1-grass`, `home:meadows/glade` or a path
    /// beside the list; the `.dtile` may be left off.
    pub tile: Spanned<String>,
    /// How many copies (1 where none is written).
    pub count: Spanned<u32>,
    /// A seed to build it with, in place of the tile's own.
    pub seed: Option<Spanned<u64>>,
    /// A base system for this piece (`db` or `ol`), in place of the list's.
    pub system: Option<Spanned<String>>,
    /// `print=`: `missing` (the default: print what the inventory lacks),
    /// `always` (print it whatever you have) or `skip` (leave it off).
    pub print: PrintMode,
    /// Where the map places it, as written (`"(2sq, 1sq)"`).
    pub at: Option<Spanned<String>>,
    /// `at` read: across and down from the map's north-west corner.
    pub place: Option<At>,
    /// Its turn on the map, as written.
    pub rot: Option<Spanned<i64>>,
    /// The height above the map's ground of the tile's zero, as written.
    pub bottom: Option<Spanned<String>>,
    /// The whole node.
    pub span: SourceSpan,
}

/// What a piece does at print time (`print=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintMode {
    /// Print what the inventory lacks of it: the count, less what you have.
    Missing,
    /// Print the whole count, whatever you have.
    Always,
    /// Leave it off the plates.
    Skip,
}

impl PrintMode {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Always => "always",
            Self::Skip => "skip",
        }
    }
}

/// Single colour or multicolour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// One filament, no paint.
    Single,
    /// Each material in its own filament.
    Multicolour,
}

impl Mode {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multicolour => "multicolour",
        }
    }
}

/// How multicolour pieces are put on plates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Pieces that share their filaments go together, then large to small.
    Filament,
    /// Large to small, whatever the filaments.
    Size,
}

impl Group {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Filament => "filament",
            Self::Size => "size",
        }
    }
}

/// The `plates` line: what the list is laid out for.
#[derive(Debug, Clone)]
pub struct Plates {
    /// The base system pieces are built for (`db`, `ol`).
    pub system: Option<Spanned<String>>,
    /// The printer profile (`bambu/x1c-ams`).
    pub printer: Option<Spanned<String>>,
    /// Single colour or multicolour.
    pub mode: Option<Spanned<Mode>>,
    /// How multicolour pieces are grouped.
    pub group: Option<Spanned<Group>>,
    /// Between two pieces, mm.
    pub gap: Option<Spanned<f64>>,
    /// Between a piece and the edge of the bed, mm.
    pub margin: Option<Spanned<f64>>,
    /// May a piece be turned a quarter to fit.
    pub turn: Option<Spanned<bool>>,
    /// Look in the inventory (`check`, the default: pieces print only what
    /// you lack) or leave it alone (`ignore`: every count prints).
    pub inventory: Option<Spanned<bool>>,
    /// Pieces kept where they were dropped.
    pub pins: Vec<Pin>,
    /// The whole node.
    pub span: SourceSpan,
}

impl Default for Plates {
    fn default() -> Self {
        Self {
            system: None,
            printer: None,
            mode: None,
            group: None,
            gap: None,
            margin: None,
            turn: None,
            inventory: None,
            pins: Vec::new(),
            span: SourceSpan::new(0.into(), 0),
        }
    }
}

/// A print list as written.
#[derive(Debug, Clone)]
pub struct PrintList {
    /// What it is for, in words (`about "…"`).
    pub about: Option<String>,
    /// The grid, if written.
    pub grid: Option<Grid>,
    /// The map's own names for heights.
    pub lets: Vec<Let>,
    /// The tiles to print, in the order written.
    pub pieces: Vec<Piece>,
    /// What it is laid out for; every word has a default.
    pub plates: Plates,
    /// The text, for diagnostics.
    pub src: Src,
    /// Where the `map` is written.
    pub span: SourceSpan,
}

struct Reader {
    src: Src,
    errors: Vec<DocError>,
}

const MAP_KEYS: [&str; 4] = ["about", "grid", "let", "piece"];
const PLATES_KEYS: [&str; 8] = [
    "system",
    "printer",
    "mode",
    "group",
    "gap",
    "margin",
    "turn",
    "inventory",
];
const PIECE_KEYS: [&str; 8] = [
    "count", "seed", "system", "print", "at", "rot", "bottom", "id",
];

impl Reader {
    fn err(
        &mut self,
        code: &'static str,
        span: SourceSpan,
        msg: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut DocError {
        self.errors
            .push(DocError::new(&self.src, code, span, msg, label));
        let n = self.errors.len() - 1;
        &mut self.errors[n]
    }

    /// A property's string value, or an error at it.
    fn string(&mut self, key: &str, e: &KdlEntry) -> Option<Spanned<String>> {
        if let KdlValue::String(s) = e.value() {
            Some(Spanned {
                value: s.clone(),
                span: value_span(e),
            })
        } else {
            self.err(
                "dt::map",
                value_span(e),
                format!("`{key}` takes a quoted word"),
                "not a string",
            )
            .help = Some(format!("write `{key}=\"…\"`"));
            None
        }
    }

    /// A property that is a word out of `words`, bare or quoted.
    fn word<T: Copy>(
        &mut self,
        key: &str,
        e: &KdlEntry,
        words: &[(&str, T)],
    ) -> Option<Spanned<T>> {
        let KdlValue::String(s) = e.value() else {
            self.err(
                "dt::map",
                value_span(e),
                format!("`{key}` takes a word"),
                "not a word",
            );
            return None;
        };
        if let Some((_, v)) = words.iter().find(|(w, _)| w == s) {
            return Some(Spanned {
                value: *v,
                span: value_span(e),
            });
        }
        let names: Vec<&str> = words.iter().map(|(w, _)| *w).collect();
        let help = did_you_mean(s, names.iter().copied()).map_or_else(
            || format!("`{key}` is {}", names.join(" or ")),
            |d| format!("did you mean `{d}`?"),
        );
        self.err(
            "dt::map",
            value_span(e),
            format!("`{s}` is not a {key}"),
            "not that",
        )
        .help = Some(help);
        None
    }

    /// A length in millimetres, `"3mm"`.
    fn mm(&mut self, key: &str, e: &KdlEntry) -> Option<Spanned<f64>> {
        let span = value_span(e);
        let KdlValue::String(s) = e.value() else {
            self.err(
                "dt::unit",
                span,
                format!("`{key}` is written with its unit"),
                "no unit",
            )
            .help = Some(format!("write `{key}=\"3mm\"`"));
            return None;
        };
        let n = s
            .trim()
            .strip_suffix("mm")
            .and_then(|n| n.trim().parse::<f64>().ok());
        match n {
            Some(v) if v.is_finite() && (0.0..=100.0).contains(&v) => {
                Some(Spanned { value: v, span })
            }
            _ => {
                self.err(
                    "dt::unit",
                    span,
                    format!("`{key}` is a length from 0 to 100 mm, like `\"3mm\"`"),
                    "not that",
                );
                None
            }
        }
    }

    /// A whole number of at least `min`.
    fn whole(&mut self, key: &str, e: &KdlEntry, min: i128, max: i128) -> Option<Spanned<i128>> {
        let span = value_span(e);
        match e.value() {
            KdlValue::Integer(i) if (min..=max).contains(i) => Some(Spanned { value: *i, span }),
            _ => {
                self.err(
                    "dt::map",
                    span,
                    format!("`{key}` is a whole number from {min} to {max}"),
                    "not that",
                );
                None
            }
        }
    }

    fn piece(&mut self, n: &KdlNode) -> Option<Piece> {
        let mut pos: Vec<&KdlEntry> = Vec::new();
        let mut p = Piece {
            id: None,
            tile: Spanned {
                value: String::new(),
                span: n.name().span(),
            },
            count: Spanned {
                value: 1,
                span: n.name().span(),
            },
            seed: None,
            system: None,
            print: PrintMode::Missing,
            at: None,
            place: None,
            rot: None,
            bottom: None,
            span: n.span(),
        };
        for e in n.entries() {
            let Some(k) = e.name() else {
                pos.push(e);
                continue;
            };
            match k.value() {
                "count" => {
                    if let Some(v) = self.whole("count", e, 1, 10_000) {
                        p.count = Spanned {
                            value: u32::try_from(v.value).unwrap_or(1),
                            span: v.span,
                        };
                    }
                }
                "seed" => {
                    if let Some(v) = self.whole("seed", e, 0, i128::from(i64::MAX)) {
                        p.seed = Some(Spanned {
                            value: u64::try_from(v.value).unwrap_or(0),
                            span: v.span,
                        });
                    }
                }
                "system" => {
                    if let Some(s) = self.word("system", e, &[("db", "db"), ("ol", "ol")]) {
                        p.system = Some(Spanned {
                            value: s.value.to_owned(),
                            span: s.span,
                        });
                    }
                }
                "print" => {
                    if let Some(s) = self.word(
                        "print",
                        e,
                        &[
                            ("missing", PrintMode::Missing),
                            ("always", PrintMode::Always),
                            ("skip", PrintMode::Skip),
                        ],
                    ) {
                        p.print = s.value;
                    }
                }
                "at" => {
                    p.at = self.string("at", e);
                    if let Some(a) = &p.at {
                        match parse_at(&a.value) {
                            Ok(x) => p.place = Some(x),
                            Err(m) => {
                                self.err("dt::map", a.span, m, "not a place").help = Some(
                                    "a place is across and down from the map's north-west corner, like `at=\"(2sq, 1sq)\"`"
                                        .to_owned(),
                                );
                            }
                        }
                    }
                }
                "height" => {
                    self.err(
                        "dt::map",
                        k.span(),
                        "a piece has no `height`",
                        "renamed",
                    )
                    .help = Some(
                        "`height` is now `bottom`: the height of the tile's zero (ground level) above the map's ground"
                            .to_owned(),
                    );
                }
                "bottom" => {
                    p.bottom = self.string("bottom", e);
                    if let Some(h) = &p.bottom
                        && let Err(m) = parse_height(&h.value)
                    {
                        self.err("dt::map", h.span, m, "not a height").help = Some(
                            "write a `$` name from the standard heights (`$wallHeight`) or a length (`20dmm`)"
                                .to_owned(),
                        );
                    }
                }
                "rot" => {
                    if let Some(v) = self.whole("rot", e, 0, 270) {
                        if v.value % 90 == 0 {
                            p.rot = Some(Spanned {
                                value: i64::try_from(v.value).unwrap_or(0),
                                span: v.span,
                            });
                        } else {
                            self.err("dt::map", v.span, "`rot` is 0, 90, 180 or 270", "not that");
                        }
                    }
                }
                other => {
                    self.err(
                        "dt::unknown-control",
                        k.span(),
                        format!("a piece has no `{other}`"),
                        "unknown",
                    )
                    .help = Some(did_you_mean(other, PIECE_KEYS).map_or_else(
                        || format!("a piece takes: {}", PIECE_KEYS.join(", ")),
                        |d| format!("did you mean `{d}`?"),
                    ));
                }
            }
        }
        match pos.as_slice() {
            [path] => p.tile = self.tile_path(path)?,
            [id, path] => {
                p.id = arg_str(id).map(|s| Spanned {
                    value: s.to_owned(),
                    span: value_span(id),
                });
                p.tile = self.tile_path(path)?;
            }
            _ => {
                self.err(
                    "dt::map",
                    n.span(),
                    "a piece names its tile",
                    "needs `piece \"lib:tiles/ground/1x1-grass\"`",
                )
                .help =
                    Some("write the tile's path in quotes after `piece`, then `count=`".to_owned());
                return None;
            }
        }
        if let Some(c) = n.children() {
            self.err(
                "dt::map",
                c.span(),
                "a piece has no children",
                "nothing goes inside it",
            );
        }
        if p.place.is_some() && p.count.value != 1 {
            self.err(
                "dt::map",
                p.count.span,
                "a placed piece is one piece",
                "more than one",
            )
            .help = Some(
                "give each copy its own `piece` line and place, or take `at` off to make a print list"
                    .to_owned(),
            );
        }
        Some(p)
    }

    /// `grid cols=8 rows=5`.
    fn grid(&mut self, n: &KdlNode, list: &mut PrintList) {
        if list.grid.is_some() {
            self.err("dt::map", n.span(), "a map has one grid", "a second one");
            return;
        }
        let (mut cols, mut rows) = (None, None);
        for e in n.entries() {
            match e.name().map(|k| k.value()) {
                Some("cols") => cols = self.whole("cols", e, 1, 200),
                Some("rows") => rows = self.whole("rows", e, 1, 200),
                _ => {
                    self.err(
                        "dt::map",
                        e.span(),
                        "the grid takes `cols=` and `rows=`",
                        "not that",
                    )
                    .help = Some("grid cols=8 rows=5".to_owned());
                }
            }
        }
        if let (Some(c), Some(r)) = (cols, rows) {
            list.grid = Some(Grid {
                cols: u32::try_from(c.value).unwrap_or(1),
                rows: u32::try_from(r.value).unwrap_or(1),
                span: n.span(),
            });
        } else {
            self.err(
                "dt::map",
                n.span(),
                "the grid needs both `cols=` and `rows=`",
                "incomplete",
            )
            .help = Some("grid cols=8 rows=5".to_owned());
        }
    }

    /// `let $plinth "20dmm"`.
    fn let_(&mut self, n: &KdlNode, list: &mut PrintList) {
        let args: Vec<&KdlEntry> = n.entries().iter().filter(|e| e.name().is_none()).collect();
        let [name, expr] = args.as_slice() else {
            self.err(
                "dt::map",
                n.span(),
                "`let` takes a name and a height",
                "needs `let $plinth \"20dmm\"`",
            );
            return;
        };
        let (Some(nm), Some(ex)) = (arg_str(name), arg_str(expr)) else {
            self.err(
                "dt::map",
                n.span(),
                "`let` takes a name and a quoted height",
                "here",
            );
            return;
        };
        let Some(bare) = nm
            .strip_prefix('$')
            .filter(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        else {
            self.err(
                "dt::map",
                value_span(name),
                format!("`{nm}` is not a name"),
                "a name starts with `$`",
            )
            .help = Some("let $plinth \"20dmm\"".to_owned());
            return;
        };
        if let Err(m) = parse_height(ex) {
            self.err("dt::map", value_span(expr), m, "not a height");
            return;
        }
        list.lets.push(Let {
            name: Spanned {
                value: bare.to_owned(),
                span: value_span(name),
            },
            expr: Spanned {
                value: ex.to_owned(),
                span: value_span(expr),
            },
        });
    }

    /// `pin p4 plate=1 at="(0mm, 5mm)" turn=90`.
    fn pin(&mut self, n: &KdlNode) -> Option<Pin> {
        let pos: Vec<&KdlEntry> = n.entries().iter().filter(|e| e.name().is_none()).collect();
        let [id] = pos.as_slice() else {
            self.err(
                "dt::map",
                n.span(),
                "a pin names its piece",
                "needs `pin p4 plate=1 at=…`",
            )
            .help = Some("give the piece an id (`piece p4 \"…\"`) and pin it by that".to_owned());
            return None;
        };
        let id = arg_str(id).map(|s| Spanned {
            value: s.to_owned(),
            span: value_span(id),
        })?;
        let mut plate = None;
        let mut at = None;
        let mut turn = false;
        for e in n.entries() {
            let Some(k) = e.name() else { continue };
            match k.value() {
                "plate" => {
                    plate = self.whole("plate", e, 1, 999).map(|v| Spanned {
                        value: u32::try_from(v.value).unwrap_or(1),
                        span: v.span,
                    });
                }
                "at" => {
                    if let Some(s) = self.string("at", e) {
                        match parse_at(&s.value) {
                            Ok(a) => at = Some((a, s.span)),
                            Err(m) => {
                                self.err("dt::map", s.span, m, "not a place");
                            }
                        }
                    }
                }
                "turn" => {
                    if let Some(v) = self.whole("turn", e, 0, 270) {
                        if v.value % 90 == 0 {
                            turn = v.value % 180 != 0;
                        } else {
                            self.err("dt::map", v.span, "`turn` is 0 or 90", "not that");
                        }
                    }
                }
                other => {
                    self.err(
                        "dt::unknown-control",
                        k.span(),
                        format!("a pin has no `{other}`"),
                        "unknown",
                    )
                    .help = Some("a pin takes: plate, at, turn".to_owned());
                }
            }
        }
        let Some(plate) = plate else {
            self.err(
                "dt::map",
                n.span(),
                "a pin names its plate",
                "needs `plate=1`",
            );
            return None;
        };
        Some(Pin {
            id,
            plate,
            at,
            turn,
            span: n.span(),
        })
    }

    fn tile_path(&mut self, e: &KdlEntry) -> Option<Spanned<String>> {
        let span = value_span(e);
        let Some(s) = arg_str(e) else {
            self.err(
                "dt::map",
                span,
                "a tile is named by its path in quotes",
                "not a path",
            );
            return None;
        };
        if s.trim().is_empty() {
            self.err("dt::map", span, "this tile path is empty", "no path");
            return None;
        }
        Some(Spanned {
            value: s.to_owned(),
            span,
        })
    }

    fn plates(&mut self, n: &KdlNode, into: &mut Plates) {
        into.span = n.span();
        for e in n.entries() {
            let Some(k) = e.name() else {
                self.err(
                    "dt::map",
                    e.span(),
                    "the plates line takes `word=value` settings",
                    "write the setting's name first",
                )
                .help = Some(format!("the settings are: {}", PLATES_KEYS.join(", ")));
                continue;
            };
            match k.value() {
                "system" => {
                    if let Some(s) = self.word("system", e, &[("db", "db"), ("ol", "ol")]) {
                        into.system = Some(Spanned {
                            value: s.value.to_owned(),
                            span: s.span,
                        });
                    }
                }
                "printer" => into.printer = self.string("printer", e),
                "mode" => {
                    into.mode = self.word(
                        "mode",
                        e,
                        &[("single", Mode::Single), ("multicolour", Mode::Multicolour)],
                    );
                }
                "group" => {
                    into.group = self.word(
                        "group",
                        e,
                        &[("filament", Group::Filament), ("size", Group::Size)],
                    );
                }
                "gap" => into.gap = self.mm("gap", e),
                "margin" => into.margin = self.mm("margin", e),
                "turn" => {
                    into.turn = self.word("turn", e, &[("allowed", true), ("off", false)]);
                }
                "inventory" => {
                    into.inventory =
                        self.word("inventory", e, &[("check", true), ("ignore", false)]);
                }
                other => {
                    self.err(
                        "dt::unknown-control",
                        k.span(),
                        format!("the plates line has no `{other}`"),
                        "unknown",
                    )
                    .help = Some(did_you_mean(other, PLATES_KEYS).map_or_else(
                        || format!("the settings are: {}", PLATES_KEYS.join(", ")),
                        |d| format!("did you mean `{d}`?"),
                    ));
                }
            }
        }
        for c in n.iter_children() {
            if c.name().value() == "pin" {
                if let Some(p) = self.pin(c) {
                    into.pins.push(p);
                }
            } else {
                self.err(
                    "dt::unknown-node",
                    c.name().span(),
                    format!(
                        "the plates line holds `pin` lines, not `{}`",
                        c.name().value()
                    ),
                    "unknown",
                )
                .help = Some("pin p4 plate=1 at=\"(0mm, 5mm)\"".to_owned());
            }
        }
    }
}

/// Read a print list.
///
/// # Errors
/// Every problem found, each pointing at its text with a hint: not KDL v2,
/// a bad header, an unknown node or setting, a piece with no tile.
pub fn decode(name: &str, text: &str) -> Result<PrintList, Vec<DocError>> {
    let src: Src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (header, header_errors) = crate::header::read(&src, &kdl);
    if header_errors.iter().any(|e| e.code == "dt::format-newer") {
        return Err(header_errors);
    }
    if header
        .as_ref()
        .is_none_or(|h| h.format < crate::header::FORMAT)
    {
        let mut errors = header_errors;
        errors.push(crate::header::format_old(&src, &kdl, header.as_ref()));
        return Err(errors);
    }
    let mut r = Reader {
        src: Arc::clone(&src),
        errors: header_errors,
    };
    let mut list = PrintList {
        about: None,
        grid: None,
        lets: Vec::new(),
        pieces: Vec::new(),
        plates: Plates::default(),
        src,
        span: SourceSpan::new(0.into(), 0),
    };
    let (mut maps, mut plates) = (0, 0);
    for n in kdl.nodes() {
        match n.name().value() {
            crate::header::NODE => {}
            "map" => {
                maps += 1;
                if maps > 1 {
                    r.err(
                        "dt::map",
                        n.name().span(),
                        "a file holds one map",
                        "a second one",
                    )
                    .help = Some("put each list in its own file".to_owned());
                    continue;
                }
                list.span = n.span();
                read_map(&mut r, n, &mut list);
            }
            "plates" => {
                plates += 1;
                if plates > 1 {
                    r.err(
                        "dt::map",
                        n.name().span(),
                        "a list has one plates line",
                        "a second one",
                    )
                    .help = Some("a pin means something only for one printer and one base; keep one `plates` line".to_owned());
                    continue;
                }
                r.plates(n, &mut list.plates);
            }
            other => {
                r.err(
                    "dt::unknown-node",
                    n.name().span(),
                    format!("unknown top-level node `{other}`"),
                    "not map or plates",
                )
                .help = Some(did_you_mean(other, ["map", "plates"]).map_or_else(
                    || {
                        "a print list is `map { piece … }` and an optional `plates …` line"
                            .to_owned()
                    },
                    |d| format!("did you mean `{d}`?"),
                ));
            }
        }
    }
    if maps == 0 && r.errors.is_empty() {
        let span = kdl
            .nodes()
            .first()
            .map_or_else(|| SourceSpan::new(0.into(), 0), KdlNode::span);
        r.err("dt::map", span, "this document has no `map`", "no map here")
            .help = Some(
            "a print list holds `map { piece \"lib:tiles/ground/1x1-grass\" count=4 }`".to_owned(),
        );
    }
    // ids are each piece's own; a pin names one of them
    let mut seen: Vec<&str> = Vec::new();
    for p in &list.pieces {
        if let Some(id) = &p.id {
            if seen.contains(&id.value.as_str()) {
                r.err(
                    "dt::map",
                    id.span,
                    format!("two pieces are called `{}`", id.value),
                    "the same id",
                )
                .help = Some("each piece has its own id".to_owned());
            }
            seen.push(&id.value);
        }
    }
    let mut pinned: Vec<&str> = Vec::new();
    for pin in &list.plates.pins {
        if !seen.contains(&pin.id.value.as_str()) {
            r.err(
                "dt::map",
                pin.id.span,
                format!("no piece is called `{}`", pin.id.value),
                "no such piece",
            )
            .help = Some(
                did_you_mean(&pin.id.value, seen.iter().copied()).map_or_else(
                    || "a pin names a piece by its id (`piece p4 \"…\"`)".to_owned(),
                    |d| format!("did you mean `{d}`?"),
                ),
            );
        } else if pinned.contains(&pin.id.value.as_str()) {
            r.err(
                "dt::map",
                pin.id.span,
                format!("`{}` is pinned twice", pin.id.value),
                "a second pin",
            );
        }
        pinned.push(&pin.id.value);
        if pin.at.is_none() {
            r.err(
                "dt::map",
                pin.span,
                "a pin says where: `at=`",
                "needs `at=\"(0mm, 5mm)\"`",
            );
        }
    }
    if r.errors.is_empty() {
        Ok(list)
    } else {
        Err(r.errors)
    }
}

fn read_map(r: &mut Reader, n: &KdlNode, list: &mut PrintList) {
    if let Some(e) = n.entries().first() {
        r.err(
            "dt::map",
            e.span(),
            "`map` takes no arguments",
            "put the pieces inside",
        )
        .help = Some("map { piece \"lib:tiles/ground/1x1-grass\" count=4 }".to_owned());
    }
    for c in n.iter_children() {
        match c.name().value() {
            "about" => {
                if let [e] = c.entries()
                    && let Some(s) = arg_str(e)
                {
                    list.about = Some(s.to_owned());
                } else {
                    r.err(
                        "dt::map",
                        c.span(),
                        "`about` takes one quoted sentence",
                        "here",
                    );
                }
            }
            "piece" => {
                if let Some(p) = r.piece(c) {
                    list.pieces.push(p);
                }
            }
            "grid" => r.grid(c, list),
            "let" => r.let_(c, list),
            other => {
                let help = did_you_mean(other, MAP_KEYS).map(|d| format!("did you mean `{d}`?"));
                r.err(
                    "dt::unknown-node",
                    c.name().span(),
                    format!("a map holds `piece` lines, not `{other}`"),
                    "unknown",
                )
                .help = help.or_else(|| Some(format!("a map takes: {}", MAP_KEYS.join(", "))));
            }
        }
    }
}

/// A `piece` line for a print list.
#[must_use]
pub fn piece_line(
    id: Option<&str>,
    tile: &str,
    count: u32,
    seed: Option<u64>,
    system: Option<&str>,
    always: bool,
) -> String {
    let mut l = format!(
        "piece {}{} count={count}",
        id.map_or_else(String::new, |i| format!("{i} ")),
        crate::inventory::quote(tile)
    );
    if let Some(s) = seed {
        l.push_str(&format!(" seed={s}"));
    }
    if let Some(s) = system {
        l.push_str(&format!(" system={s}"));
    }
    if always {
        l.push_str(" print=always");
    }
    l
}

/// A piece to put in a list: the same tile, seed, system and `print` word
/// make the same piece.
#[derive(Debug, Clone, Copy)]
pub struct Want<'a> {
    /// The tile's path, as a piece writes it.
    pub tile: &'a str,
    /// A seed, if it is not the tile's own.
    pub seed: Option<u64>,
    /// A system, if it is not the list's.
    pub system: Option<&'a str>,
    /// `print=always`.
    pub always: bool,
}

fn same_tile(a: &str, b: &str) -> bool {
    a.strip_suffix(".dtile").unwrap_or(a) == b.strip_suffix(".dtile").unwrap_or(b)
}

/// Put one more copy of a piece in a list: a piece already there (same tile,
/// seed, system and `print`) has its `count` raised, else a new `piece` line
/// is added.  Everything else in the file is kept.  Returns the new text and
/// the piece's count now.
///
/// # Errors
/// The text does not read as a print list: its mistakes.
pub fn add_or_bump(text: &str, want: &Want) -> Result<(String, u32), Vec<DocError>> {
    let list = decode("list", text)?;
    let found = list.pieces.iter().position(|p| {
        same_tile(&p.tile.value, want.tile)
            && p.seed.as_ref().map(|s| s.value) == want.seed
            && p.system.as_ref().map(|s| s.value.as_str()) == want.system
            && (p.print == PrintMode::Always) == want.always
    });
    let wrap = |e: String| {
        vec![DocError::new(
            &list.src,
            "dt::map",
            list.span,
            e,
            "this list",
        )]
    };
    // an edit of the editor's names every piece (the first one writes the ids of all)
    let (text, _) = give_ids(text).map_err(wrap)?;
    if let Some(i) = found {
        let n = list.pieces[i].count.value + 1;
        let id = piece_ids(&list)[i].clone();
        let out = set_count(&text, &id, n).map_err(wrap)?;
        return Ok((out, n));
    }
    let list = decode("list", &text)?;
    let line = piece_line(
        Some(&next_id(&list)),
        want.tile,
        1,
        want.seed,
        want.system,
        want.always,
    );
    Ok((
        crate::inventory::insert_in_block(&text, list.span, &[line]),
        1,
    ))
}

/// The nodes of a list's map, in order.
fn map_nodes(doc: &mut KdlDocument) -> Result<&mut Vec<KdlNode>, String> {
    let map = doc
        .nodes_mut()
        .iter_mut()
        .find(|n| n.name().value() == "map")
        .ok_or("the list has no map")?;
    Ok(map
        .children_mut()
        .as_mut()
        .ok_or("the map has no pieces")?
        .nodes_mut())
}

/// The id a piece node is written with (`piece p1 "tile"`), if any.
fn node_id(n: &KdlNode) -> Option<String> {
    let args: Vec<&KdlEntry> = n.entries().iter().filter(|e| e.name().is_none()).collect();
    if args.len() == 2 {
        args[0].value().as_string().map(str::to_owned)
    } else {
        None
    }
}

/// Where piece `id` is among the map's nodes.
fn piece_at(nodes: &[KdlNode], id: &str) -> Result<usize, String> {
    nodes
        .iter()
        .position(|n| n.name().value() == "piece" && node_id(n).as_deref() == Some(id))
        .ok_or_else(|| format!("the list has no piece {id}"))
}

/// The id of each piece of the list, in order: the one written, or the one
/// [`give_ids`] writes (the first `pN` not taken, in order), so a page can
/// name a piece before it has an id in its file.
#[must_use]
pub fn piece_ids(list: &PrintList) -> Vec<String> {
    let mut taken: Vec<String> = list
        .pieces
        .iter()
        .filter_map(|p| p.id.as_ref().map(|i| i.value.clone()))
        .collect();
    let mut n = 0;
    list.pieces
        .iter()
        .map(|p| {
            p.id.as_ref().map_or_else(
                || loop {
                    n += 1;
                    let c = format!("p{n}");
                    if !taken.contains(&c) {
                        taken.push(c.clone());
                        break c;
                    }
                },
                |i| i.value.clone(),
            )
        })
        .collect()
}

/// `text` with an id written on every piece that has none (`pN`, as
/// [`piece_ids`] says), every other byte kept; and whether any was written.
/// The editor's first change to a file does this in the same write.
///
/// # Errors
/// The text does not read as a list, or is not KDL.
pub fn give_ids(text: &str) -> Result<(String, bool), String> {
    let list = decode("list", text).map_err(|e| {
        e.first().map_or_else(
            || "the list does not read".to_owned(),
            |d| d.message.clone(),
        )
    })?;
    if list.pieces.iter().all(|p| p.id.is_some()) {
        return Ok((text.to_owned(), false));
    }
    let ids = piece_ids(&list);
    let mut doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    let nodes = map_nodes(&mut doc)?;
    for (k, n) in nodes
        .iter_mut()
        .filter(|n| n.name().value() == "piece")
        .enumerate()
    {
        if node_id(n).is_none() {
            n.entries_mut()
                .insert(0, KdlEntry::new(KdlValue::String(ids[k].clone())));
        }
    }
    Ok((doc.to_string(), true))
}

/// Set piece `id` to `count` copies, keeping every other byte.
///
/// # Errors
/// There is no such piece, the count is not 1 to 10 000, or the text is not KDL.
pub fn set_count(text: &str, id: &str, count: u32) -> Result<String, String> {
    if !(1..=10_000).contains(&count) {
        return Err("a count is from 1 to 10 000: take the piece out to have none".to_owned());
    }
    let mut doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    let nodes = map_nodes(&mut doc)?;
    let at = piece_at(nodes, id)?;
    nodes[at].insert("count", KdlValue::Integer(i128::from(count)));
    Ok(doc.to_string())
}

/// Take piece `id` out of a list, keeping every other byte.
///
/// # Errors
/// There is no such piece, or the text is not KDL.
pub fn remove_piece(text: &str, id: &str) -> Result<String, String> {
    let mut doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    let nodes = map_nodes(&mut doc)?;
    let at = piece_at(nodes, id)?;
    nodes.remove(at);
    Ok(doc.to_string())
}

/// `text` (a print list that reads) with `lines` added as the last pieces of
/// its map, every other byte as it was.
///
/// # Errors
/// The text does not read as a print list: its mistakes, so a piece is never
/// added to a file that is not understood.
pub fn add_pieces(text: &str, lines: &[String]) -> Result<String, Vec<DocError>> {
    let list = decode("list", text)?;
    Ok(crate::inventory::insert_in_block(text, list.span, lines))
}

/// A value a piece's property is set to.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    /// A quoted string (`at`, `bottom`, `system`, `print`).
    Str(String),
    /// A whole number (`rot`, `seed`).
    Int(i64),
}

impl Prop {
    fn value(&self) -> KdlValue {
        match self {
            Self::Str(s) => KdlValue::String(s.clone()),
            Self::Int(i) => KdlValue::Integer(i128::from(*i)),
        }
    }
}

/// Set (`Some`) or take off (`None`) properties of piece `id`, keeping every
/// other byte.
///
/// # Errors
/// There is no such piece, or the text is not KDL.
pub fn set_piece(text: &str, id: &str, edits: &[(&str, Option<Prop>)]) -> Result<String, String> {
    let mut doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    let nodes = map_nodes(&mut doc)?;
    let at = piece_at(nodes, id)?;
    let node = &mut nodes[at];
    for (k, v) in edits {
        match v {
            Some(v) => {
                node.insert(*k, v.value());
            }
            None => {
                // by name: `KdlNode::remove` compares the written form too, and misses
                node.entries_mut()
                    .retain(|e| e.name().is_none_or(|n| n.value() != *k));
            }
        }
    }
    Ok(doc.to_string())
}

/// The first `pN` no piece of the list is called.
#[must_use]
pub fn next_id(list: &PrintList) -> String {
    let used: Vec<&str> = list
        .pieces
        .iter()
        .filter_map(|p| p.id.as_ref().map(|i| i.value.as_str()))
        .collect();
    (1..)
        .map(|n| format!("p{n}"))
        .find(|c| !used.contains(&c.as_str()))
        .unwrap_or_default()
}

/// A `piece` line for a piece placed on a map.
#[must_use]
pub fn placed_line(
    id: &str,
    tile: &str,
    at: At,
    rot: i64,
    seed: Option<u64>,
    system: Option<&str>,
) -> String {
    let mut l = format!(
        "piece {id} {} at={} rot={rot}",
        crate::inventory::quote(tile),
        crate::inventory::quote(&at.text())
    );
    if let Some(s) = seed {
        l.push_str(&format!(" seed={s}"));
    }
    if let Some(s) = system {
        l.push_str(&format!(" system={s}"));
    }
    l
}

/// A copy of piece `id` under a new id at `at`, put just after it.
///
/// # Errors
/// There is no such piece, or the text is not KDL.
pub fn duplicate_piece(text: &str, id: &str, new_id: &str, at: At) -> Result<String, String> {
    let mut doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    let nodes = map_nodes(&mut doc)?;
    let at_i = piece_at(nodes, id)?;
    let mut copy = nodes[at_i].clone();
    // the copy is its own piece: it has an id of its own
    if let Some(e) = copy.entries_mut().iter_mut().find(|e| e.name().is_none()) {
        e.set_value(KdlValue::String(new_id.to_owned()));
        e.clear_format();
    }
    copy.insert("at", KdlValue::String(at.text()));
    nodes.insert(at_i + 1, copy);
    Ok(doc.to_string())
}

/// The text of a `pin` line.
fn pin_line(id: &str, plate: u32, at: At, turn: bool) -> String {
    let mut l = format!(
        "pin {} plate={plate} at={}",
        crate::inventory::quote(id),
        crate::inventory::quote(&at.text())
    );
    if turn {
        l.push_str(" turn=90");
    }
    l
}

/// Pin piece `id` to plate `plate` at `at`: an existing pin of it is replaced,
/// else a `pin` line goes inside the `plates` line (made if there is none).
/// Every other byte is kept.
///
/// # Errors
/// The text does not read as a map: its mistakes.
pub fn set_pin(
    text: &str,
    id: &str,
    plate: u32,
    at: At,
    turn: bool,
) -> Result<String, Vec<DocError>> {
    let list = decode("map", text)?;
    let line = pin_line(id, plate, at, turn);
    if let Some(p) = list.plates.pins.iter().find(|p| p.id.value == id) {
        let (s, e) = (p.span.offset(), p.span.offset() + p.span.len());
        return Ok(format!("{}{line}{}", &text[..s], &text[e..]));
    }
    if list.plates.span.is_empty() {
        let sep = if text.ends_with('\n') { "" } else { "\n" };
        return Ok(format!("{text}{sep}\nplates {{\n    {line}\n}}\n"));
    }
    Ok(crate::inventory::insert_in_block(
        text,
        list.plates.span,
        &[line],
    ))
}

/// Take the pin of piece `id` away (no pin: the text as it was).
///
/// # Errors
/// The text does not read as a map: its mistakes.
pub fn clear_pin(text: &str, id: &str) -> Result<String, Vec<DocError>> {
    let list = decode("map", text)?;
    let Some(p) = list.plates.pins.iter().find(|p| p.id.value == id) else {
        return Ok(text.to_owned());
    };
    let (mut s, mut e) = (p.span.offset(), p.span.offset() + p.span.len());
    // the whole line: its indent and its newline go with it
    while s > 0 && matches!(text.as_bytes()[s - 1], b' ' | b'\t') {
        s -= 1;
    }
    if text[e..].starts_with('\n') {
        e += 1;
    }
    Ok(format!("{}{}", &text[..s], &text[e..]))
}

/// Every pin taken away.
///
/// # Errors
/// The text does not read as a map: its mistakes.
pub fn clear_pins(text: &str) -> Result<String, Vec<DocError>> {
    let list = decode("map", text)?;
    let mut out = text.to_owned();
    for p in list.plates.pins.iter().rev() {
        out = clear_pin(&out, &p.id.value)?;
    }
    Ok(out)
}

/// A new print list's text: the pieces, laid out on `printer` (the default
/// where none) for `system`, in `mode` where one is given.
#[must_use]
pub fn new_list(
    version: &str,
    about: &str,
    lines: &[String],
    printer: Option<&str>,
    system: &str,
    mode: Option<Mode>,
) -> String {
    let mut t = format!(
        "dharmatiles version={}\n\nmap {{\n    about {}\n",
        crate::inventory::quote(version),
        crate::inventory::quote(about)
    );
    for l in lines {
        t.push_str("    ");
        t.push_str(l);
        t.push('\n');
    }
    t.push_str("}\n\nplates");
    if let Some(p) = printer {
        t.push_str(&format!(" printer={}", crate::inventory::quote(p)));
    }
    t.push_str(&format!(" system={system}"));
    if let Some(m) = mode {
        t.push_str(&format!(" mode={}", m.word()));
    }
    t.push('\n');
    t
}
