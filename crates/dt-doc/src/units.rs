//! Units and kinds (format 1): what a written value measures, checked before
//! anything is built, and the numbers a unit stands for.
//!
//! A value is a **length** (`8dmm`, `3sq`, `0.2layer`), an **angle**
//! (`90deg`, `2 pi rad`), a **plain** number (a count, a fraction, `50%`,
//! `$x`), a **region** (`$tile`, `$region`, `parent(…)`) or a **point**
//! (`point(…)`, `polar(…)`).  A bare `0` is the same in every unit and fits
//! any of the first three.  [`kind_of`] settles the kind of an expression
//! and refuses what measures nothing (a length plus a plain number, a
//! length times a length, a region as a number).
//!
//! [`lower`] then turns every unit into the number the rest of the program
//! reads: a length in design millimetres (1/35 of a square, the reference
//! square the elements are written at) or in squares for a point, an angle
//! in degrees.  `dmm` is the reference itself (a factor of exactly 1) and
//! `sq` is `35`, so a size written as `dmm` lowers to the same number,
//! bit for bit; `mm`, `in` and `layer` are printed
//! lengths and scale back by the system's square, the way a size scales
//! forward (`35 / square`).

use crate::expr::{BinOp, Expr, UnitWord};
use crate::schema::SQUARE_MM;

/// What a unit needs to know of the build target: the square (35 mm on
/// DungeonBlocks, 25.4 on OpenLOCK) and the printer's layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    /// The square, mm.
    pub square_mm: f64,
    /// One printed layer, mm.
    pub layer_mm: f64,
}

impl Default for Units {
    /// DungeonBlocks with the default printer's 0.2 mm layer.
    fn default() -> Self {
        Self {
            square_mm: SQUARE_MM,
            layer_mm: 0.2,
        }
    }
}

/// What a value measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A plain number: a count, a fraction, `50%`, `$x`.
    Plain,
    /// A length.
    Length,
    /// An angle.
    Angle,
    /// A region: `$tile`, `$region`, `parent(…)`.
    Region,
    /// A point: `point(…)`, `polar(…)`.
    Point,
    /// A bare `0`: fits a length, an angle or a plain number.
    Zero,
    /// Not known until the definition is called (a `$` name a caller
    /// installs, a parameter that fills no control): it fits any.
    Any,
    /// A thing: controls and a seed make one solid in its own frame, and
    /// it reads nothing (`rock`, `tree`, a definition of one).
    Thing,
    /// A layer: reads the scene so far and changes it (`dirt`, `water`, a
    /// stack, a part).
    Layer,
}

impl Kind {
    /// How the kind reads in a message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Kind::Plain => "a plain number",
            Kind::Length => "a length",
            Kind::Angle => "an angle",
            Kind::Region => "a region",
            Kind::Point => "a point",
            Kind::Zero | Kind::Any => "a number",
            Kind::Thing => "a thing",
            Kind::Layer => "a layer",
        }
    }

    /// The kind words, written as `kind=` on a `param`: the one table.
    pub const WORDS: &'static [(&'static str, Kind)] = &[
        ("length", Kind::Length),
        ("angle", Kind::Angle),
        ("plain", Kind::Plain),
        ("region", Kind::Region),
        ("point", Kind::Point),
        ("thing", Kind::Thing),
        ("layer", Kind::Layer),
    ];

    /// The kind a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Kind> {
        Self::WORDS
            .iter()
            .find(|(w, _)| *w == word)
            .map(|(_, k)| *k)
    }

    /// The word for the kind, as written in `kind=` (`any` and a bare zero
    /// have none of their own).
    #[must_use]
    pub fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, k)| *k == self)
            .map_or("any", |(w, _)| w)
    }

    /// True for a generator: a thing or a layer.
    #[must_use]
    pub fn is_generator(self) -> bool {
        matches!(self, Kind::Thing | Kind::Layer)
    }

    /// True when a value of this kind can be a parameter's: a length, an
    /// angle, a plain number, a thing or a layer.
    #[must_use]
    pub fn holdable(self) -> bool {
        matches!(
            self,
            Kind::Length | Kind::Angle | Kind::Plain | Kind::Thing | Kind::Layer
        )
    }
}

/// Whether a value of kind `have` fits where kind `want` is wanted.  The one
/// answer, asked at every place a name, a parameter, an entry or a child is
/// checked: a kind fits itself; anything fits `Any`, and `Any` (not known
/// until the call) fits anything; a bare zero fits a length, an angle or a
/// plain number; a thing fits where a layer is wanted, because placing a thing
/// makes it a layer; a layer never fits where a thing is wanted.
#[must_use]
pub fn fits(have: Kind, want: Kind) -> bool {
    have == want
        || have == Kind::Any
        || want == Kind::Any
        || (have == Kind::Zero && matches!(want, Kind::Length | Kind::Angle | Kind::Plain))
        || (have == Kind::Thing && want == Kind::Layer)
}

/// A value of the wrong kind where `who` wants `want`: the message and the
/// help every such mistake gives, one wording whatever the place.  `name` is
/// what was written (`dirt`), `have` what it is, `who` what wants it
/// (`` `stand piece` ``, `a scatter`).
#[must_use]
pub fn misfit(name: &str, have: Kind, who: &str, want: Kind) -> KindError {
    let help = match (have, want) {
        (Kind::Layer, Kind::Thing) => format!(
            "a layer reads the scene so far (it paints the ground, or places things); a thing is one solid on its own: a rock, a flower or a tree, or a part whose stack is exactly one of them with no `at=` (a placed thing is a layer). Put `{name}` in the stack instead, and give {who} a thing"
        ),
        (Kind::Thing | Kind::Layer, Kind::Layer | Kind::Thing) => format!(
            "`{name}` is {}; {who} wants {}",
            have.describe(),
            want.describe()
        ),
        (Kind::Thing | Kind::Layer, _) => format!(
            "a {} goes in the stack, or where a parameter holds one (`param leaf kind={}`, then `leaf` in the stack); {who} takes {}",
            have.word(),
            have.word(),
            want.describe()
        ),
        _ => wanted_help(want),
    };
    misfit_text(name, have.describe(), who, want, help)
}

/// [`misfit`] for something written that has no kind of its own (a yes or
/// no, nothing): `have` is how it reads in the message.
#[must_use]
pub fn misfit_text(name: &str, have: &str, who: &str, want: Kind, help: String) -> KindError {
    KindError {
        code: dt_lang::codes::TYPE,
        message: format!("`{name}` is {have}, and {who} wants {}", want.describe()),
        help: Some(help),
    }
}

/// What to write where a kind is wanted.
#[must_use]
pub fn wanted_help(want: Kind) -> String {
    match want {
        Kind::Thing => "pass the name of a thing: the elements `rock`, `stone`, `boulder`, `pebble`, `flower` and `tree`, and a part whose stack is exactly one of them".to_owned(),
        Kind::Layer => "pass the name of a layer or a thing: an element such as `dirt` or `floor`, or a part".to_owned(),
        k => format!("write {}", k.describe()),
    }
}

/// Why a value does not measure what it should.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindError {
    /// The diagnostic code: `dt::unit` for a number missing its unit,
    /// `dt::type` for a mismatch.
    pub code: &'static str,
    /// What is wrong.
    pub message: String,
    /// What to write instead.
    pub help: Option<String>,
}

fn err<T>(message: impl Into<String>, help: impl Into<Option<String>>) -> Result<T, KindError> {
    Err(KindError {
        code: dt_lang::codes::TYPE,
        message: message.into(),
        help: help.into(),
    })
}

fn unit_err<T>(
    message: impl Into<String>,
    help: impl Into<Option<String>>,
) -> Result<T, KindError> {
    err(message, help).map_err(|e: KindError| KindError {
        code: dt_lang::codes::UNIT,
        ..e
    })
}

/// How a name is read: its kind, or `None` when it is not known here.
pub type Names<'a> = &'a dyn Fn(&str) -> Option<Kind>;

/// The kind of `e`.  A `%` written as a number is plain, except where
/// `pct_length` says it is a length (a point's half: a share of the frame).
///
/// # Errors
/// The first thing that measures nothing.
pub fn kind_of(e: &Expr, names: Names<'_>, pct_length: bool) -> Result<Kind, KindError> {
    let kind = |x: &Expr| kind_of(x, names, pct_length);
    Ok(match e {
        Expr::Num { percent: true, .. } => {
            if pct_length {
                Kind::Length
            } else {
                Kind::Plain
            }
        }
        Expr::Num { value, .. } => {
            if *value == 0.0 {
                Kind::Zero
            } else {
                Kind::Plain
            }
        }
        Expr::Unit(u) => match u {
            UnitWord::Pi => Kind::Plain,
            UnitWord::Deg | UnitWord::Rad => Kind::Angle,
            UnitWord::Pct => {
                return err(
                    "`%` as a unit goes second in a coordinate",
                    "write `x($tile, %)`, or `50%` for a fraction".to_owned(),
                );
            }
            _ => Kind::Length,
        },
        // a name no scope knows yet is the caller's: checked where it is called
        Expr::Name(n) => names(n).unwrap_or(Kind::Any),
        Expr::Neg(a) => number(kind(a)?, "a negation")?,
        Expr::Range { lo, hi } => unify(kind(lo)?, kind(hi)?, e)?,
        Expr::Weighted { range, .. } => kind(range)?,
        Expr::List { items } => {
            let mut k = Kind::Zero;
            for (v, _) in items {
                k = unify(k, kind(v)?, e)?;
            }
            k
        }
        Expr::Bin { op, lhs, rhs } => {
            let (a, b) = (
                number(kind(lhs)?, "arithmetic")?,
                number(kind(rhs)?, "arithmetic")?,
            );
            match op {
                BinOp::Add | BinOp::Sub => unify(a, b, e)?,
                BinOp::Mul => mul(a, b, e)?,
                BinOp::Div => div(a, b, e, rhs)?,
            }
        }
        Expr::Call { name, args } => call_kind(name, args, names, pct_length, e)?,
    })
}

/// A region or a point is not a number.
fn number(k: Kind, what: &str) -> Result<Kind, KindError> {
    match k {
        Kind::Region => err(
            format!("a region is not a number, and {what} needs one"),
            "a region goes to `parent(…)` and the coordinates `x($tile, sq)`, `d($region, %)`"
                .to_owned(),
        ),
        Kind::Point => err(
            format!("a point is not a number, and {what} needs one"),
            "a point is written alone: \"(0.5sq, 1sq)\", \"point($tile, 50%, 0)\"".to_owned(),
        ),
        Kind::Thing => err(
            format!("a thing is not a number, and {what} needs one"),
            "a thing is placed in the stack, not worked out".to_owned(),
        ),
        Kind::Layer => err(
            format!("a layer is not a number, and {what} needs one"),
            "a layer goes in the stack, not in a formula".to_owned(),
        ),
        k => Ok(k),
    }
}

fn unify(a: Kind, b: Kind, e: &Expr) -> Result<Kind, KindError> {
    Ok(match (a, b) {
        (Kind::Any, k) | (k, Kind::Any) | (Kind::Zero, k) | (k, Kind::Zero) => k,
        (x, y) if x == y => x,
        (x, y) => {
            let help = match (x, y) {
                (Kind::Length, Kind::Plain) | (Kind::Plain, Kind::Length) => {
                    "give the number its unit: `4dmm` or `4mm`; a plain number is a count or a fraction, never a length".to_owned()
                }
                (Kind::Angle, Kind::Plain) | (Kind::Plain, Kind::Angle) => {
                    "give the number its unit: `90deg`, or `2 pi rad` for a turn".to_owned()
                }
                _ => "both sides of a sum, a range or a list measure the same thing".to_owned(),
            };
            return err(
                format!("`{e}` mixes {} and {}", x.describe(), y.describe()),
                help,
            );
        }
    })
}

fn mul(a: Kind, b: Kind, e: &Expr) -> Result<Kind, KindError> {
    Ok(match (a, b) {
        (Kind::Zero, Kind::Zero) => Kind::Zero,
        (Kind::Any, k)
        | (k, Kind::Any)
        | (Kind::Plain | Kind::Zero, k)
        | (k, Kind::Plain | Kind::Zero) => k,
        (x, y) => {
            let what = if x == Kind::Length && y == Kind::Length {
                "no area exists in this language".to_owned()
            } else {
                "multiply by a plain number".to_owned()
            };
            return err(
                format!(
                    "`{e}` multiplies {} by {}, which measures nothing",
                    x.describe(),
                    y.describe()
                ),
                what,
            );
        }
    })
}

fn div(a: Kind, b: Kind, e: &Expr, rhs: &Expr) -> Result<Kind, KindError> {
    Ok(match (a, b) {
        (Kind::Any, _) => Kind::Any,
        (x, Kind::Any) => x,
        (x, Kind::Plain | Kind::Zero) => x,
        (Kind::Zero, _) => Kind::Plain,
        (x, y) if x == y => Kind::Plain,
        (Kind::Plain, y @ (Kind::Length | Kind::Angle)) => {
            // `1/2rad` is 1/(2rad): one over an angle is nothing
            let help = match rhs {
                Expr::Bin {
                    op: BinOp::Mul,
                    rhs: u,
                    ..
                } if matches!(**u, Expr::Unit(_)) => {
                    let Expr::Unit(unit) = **u else {
                        unreachable!()
                    };
                    format!(
                        "a unit binds to its number, so `1/2rad` is one over two radians: write `(1/2) {w}` or `0.5{w}`",
                        w = unit.word()
                    )
                }
                _ => "divide by a plain number, or by something of the same kind to get a plain number".to_owned(),
            };
            return err(
                format!(
                    "`{e}` divides a plain number by {}, which measures nothing",
                    y.describe()
                ),
                help,
            );
        }
        (x, y) => {
            return err(
                format!(
                    "`{e}` divides {} by {}, which measures nothing",
                    x.describe(),
                    y.describe()
                ),
                "divide by a plain number, or by something of the same kind to get a plain number"
                    .to_owned(),
            );
        }
    })
}

fn call_kind(
    name: &str,
    args: &[Expr],
    names: Names<'_>,
    pct_length: bool,
    e: &Expr,
) -> Result<Kind, KindError> {
    let kind = |x: &Expr| kind_of(x, names, pct_length);
    let takes = |n: usize, help: &str| {
        if args.len() == n {
            Ok(())
        } else {
            err(
                format!("`{name}` takes {n} values, not {}", args.len()),
                help.to_owned(),
            )
        }
    };
    let region = |x: &Expr| -> Result<(), KindError> {
        match kind(x)? {
            Kind::Region | Kind::Any => Ok(()),
            k => err(
                format!("`{name}` takes a region first, not {}", k.describe()),
                "write `$tile`, `$region` or `parent($region)`".to_owned(),
            ),
        }
    };
    match name {
        "parent" => {
            takes(1, "`parent($region)` is the region containing this one")?;
            if matches!(&args[0], Expr::Name(n) if n == "$tile") {
                return err(
                    "the tile has no parent",
                    "`$tile` is the root; `parent($region)` is the region containing a thing's own"
                        .to_owned(),
                );
            }
            match kind(&args[0])? {
                Kind::Region | Kind::Any => Ok(Kind::Region),
                k => err(
                    format!("`parent` takes a region, not {}", k.describe()),
                    "write `parent($region)`: the region containing this thing's own".to_owned(),
                ),
            }
        }
        "x" | "y" | "d" | "a" => {
            takes(
                2,
                &format!(
                    "`{name}($tile, {})`: a region, then the unit it counts in",
                    if name == "a" { "deg" } else { "sq" }
                ),
            )?;
            region(&args[0])?;
            let ok: &[UnitWord] = match name {
                "a" => &[UnitWord::Pct, UnitWord::Deg, UnitWord::Rad],
                "d" => &[
                    UnitWord::Pct,
                    UnitWord::Mm,
                    UnitWord::In,
                    UnitWord::Dmm,
                    UnitWord::Sq,
                    UnitWord::Layer,
                ],
                _ => &[
                    UnitWord::Sq,
                    UnitWord::Mm,
                    UnitWord::In,
                    UnitWord::Dmm,
                    UnitWord::Layer,
                    UnitWord::Pct,
                ],
            };
            match &args[1] {
                Expr::Unit(u) if ok.contains(u) => Ok(Kind::Plain),
                _ => {
                    let list: Vec<&str> = ok.iter().map(|u| u.word()).collect();
                    err(
                        format!(
                            "`{name}` counts in {}; the unit is written, as the second value",
                            list.join(", ")
                        ),
                        format!("write `{name}($tile, {})`", list[0]),
                    )
                }
            }
        }
        "point" => {
            takes(
                3,
                "`point($tile, 50%, 2sq)`: the frame, then across and down",
            )?;
            region(&args[0])?;
            for half in &args[1..] {
                expect_kind(kind(half)?, Kind::Length, half)?;
            }
            Ok(Kind::Point)
        }
        "polar" => {
            takes(
                3,
                "`polar($tile, 40%, 90deg)`: the frame, how far out, the angle round",
            )?;
            region(&args[0])?;
            expect_kind(kind(&args[1])?, Kind::Plain, &args[1])?;
            expect_kind(kind(&args[2])?, Kind::Angle, &args[2])?;
            Ok(Kind::Point)
        }
        _ => {
            let Some((_, n)) = crate::expr::FUNCTIONS.iter().find(|(k, _)| *k == name) else {
                let all: Vec<&str> = crate::expr::FUNCTIONS.iter().map(|(k, _)| *k).collect();
                return err(
                    format!("no function is called `{name}`"),
                    format!("the functions are {}", all.join(", ")),
                );
            };
            takes(*n, "")?;
            let ks = args.iter().map(&kind).collect::<Result<Vec<_>, _>>()?;
            let ks = ks
                .into_iter()
                .map(|k| number(k, &format!("`{name}`")))
                .collect::<Result<Vec<_>, _>>()?;
            match name {
                "sin" | "cos" => match ks[0] {
                    Kind::Angle | Kind::Zero | Kind::Any => Ok(Kind::Plain),
                    k => err(
                        format!("`{name}` takes an angle, not {}", k.describe()),
                        format!("write `{name}(90deg)`; a turn is `360deg` or `2 pi rad`"),
                    ),
                },
                "smooth" => match ks[0] {
                    Kind::Plain | Kind::Zero | Kind::Any => Ok(Kind::Plain),
                    k => err(
                        format!("`smooth` takes a plain number, not {}", k.describe()),
                        None,
                    ),
                },
                _ => {
                    let mut k = Kind::Zero;
                    for x in ks {
                        k = unify(k, x, e)?;
                    }
                    Ok(k)
                }
            }
        }
    }
}

/// Whether kind `k` is what a place wants, with the help a bare number
/// gets (`write 8dmm or 8mm`).
fn expect_kind(k: Kind, want: Kind, e: &Expr) -> Result<(), KindError> {
    let k = number(k, "this")?;
    if fits(k, want) {
        return Ok(());
    }
    let bare = matches!(e, Expr::Num { percent: false, .. });
    let wanted = want.describe();
    // a range of bare numbers, `6..10`
    if let (Kind::Length | Kind::Angle, Kind::Plain, Expr::Range { lo, hi }) = (want, k, e)
        && matches!(**lo, Expr::Num { percent: false, .. })
        && matches!(**hi, Expr::Num { percent: false, .. })
    {
        let unit = if want == Kind::Length { "dmm" } else { "deg" };
        return unit_err(
            format!(
                "`{e}` has no units, and a {} is written with one",
                if want == Kind::Length {
                    "length"
                } else {
                    "angle"
                }
            ),
            format!("write each end with its unit: {lo}{unit}..{hi}{unit}"),
        );
    }
    match (want, k) {
        (Kind::Length, Kind::Plain) if bare => unit_err(
            format!("`{e}` has no unit, and a length is written with one"),
            format!("write {e}dmm or {e}mm (`dmm` is 1/35 of a square: 1 mm on DungeonBlocks; `mm` is a printed millimetre)"),
        ),
        (Kind::Length, Kind::Plain) => err(
            format!("`{e}` is a plain number, and a length is wanted"),
            "a length has a unit: `8dmm`, `8mm`, `0.5sq`; multiply a length by a plain number to scale it".to_owned(),
        ),
        (Kind::Angle, Kind::Plain) if bare => unit_err(
            format!("`{e}` has no unit, and an angle is written with one"),
            format!("write {e}deg, or `2 pi rad` for a turn"),
        ),
        (Kind::Plain, Kind::Length) => err(
            format!("`{e}` is a length, and a plain number is wanted"),
            "divide a length by a length for a plain number, or write the number alone".to_owned(),
        ),
        _ => err(
            format!("`{e}` is {}, and {wanted} is wanted", k.describe()),
            None,
        ),
    }
}

/// Check that `e` measures `want`.
///
/// # Errors
/// What measures nothing, or the wrong thing.
pub fn expect(e: &Expr, want: Kind, names: Names<'_>, pct_length: bool) -> Result<Kind, KindError> {
    let k = kind_of(e, names, pct_length)?;
    // a `let` holds anything: a region too
    if want != Kind::Any {
        expect_kind(k, want, e)?;
    }
    Ok(k)
}

/// What a lowered length is counted in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Base {
    /// Design millimetres: the reference the elements are written at, 1/35
    /// of a square.  Every control, top and distance.
    Dmm,
    /// Squares, from a corner: a point's halves.
    Squares,
    /// Printed millimetres: a printer's numbers (`sq`, `dmm` and `layer`
    /// are not units there).
    Mm,
}

/// Lower `e` (already checked) to numbers: each unit becomes its factor, a
/// `%` written as a number in a point's half becomes that share of
/// `pct_extent` (the frame's size across or down, squares).  The units a
/// coordinate counts in (`x($tile, sq)`) stay, with the region they take.
///
/// # Errors
/// A unit a printer does not have.
pub fn lower(
    e: &Expr,
    base: Base,
    units: Units,
    pct_extent: Option<f64>,
) -> Result<Expr, KindError> {
    let go = |x: &Expr| lower(x, base, units, pct_extent);
    Ok(match e {
        Expr::Num {
            value,
            percent: true,
        } => match pct_extent {
            Some(extent) => Expr::num(value / 100.0 * extent),
            None => e.clone(),
        },
        Expr::Num { .. } | Expr::Name(_) => e.clone(),
        Expr::Unit(u) => Expr::num(factor(*u, base, units)?),
        Expr::Neg(a) => Expr::Neg(Box::new(go(a)?)),
        Expr::Bin {
            op: BinOp::Mul,
            lhs,
            rhs,
        } if matches!(**rhs, Expr::Unit(_)) => {
            let Expr::Unit(u) = **rhs else { unreachable!() };
            let f = factor(u, base, units)?;
            let n = go(lhs)?;
            // the reference itself: no factor, so no float differs
            #[expect(
                clippy::float_cmp,
                reason = "an exact comparison of a stored or sentinel value, not a computed one"
            )]
            if f == 1.0 {
                n
            } else {
                Expr::Bin {
                    op: BinOp::Mul,
                    lhs: Box::new(n),
                    rhs: Box::new(Expr::num(f)),
                }
            }
        }
        Expr::Bin { op, lhs, rhs } => Expr::Bin {
            op: *op,
            lhs: Box::new(go(lhs)?),
            rhs: Box::new(go(rhs)?),
        },
        Expr::Range { lo, hi } => Expr::Range {
            lo: Box::new(go(lo)?),
            hi: Box::new(go(hi)?),
        },
        Expr::Weighted {
            name,
            args,
            range,
            parts,
        } => Expr::Weighted {
            name: name.clone(),
            args: args.clone(),
            range: Box::new(go(range)?),
            parts: parts.clone(),
        },
        Expr::List { items } => Expr::List {
            items: items
                .iter()
                .map(|(v, w)| Ok((go(v)?, *w)))
                .collect::<Result<_, KindError>>()?,
        },
        // the coordinates keep their region and their unit: the solver
        // reads them where it knows the region
        Expr::Call { name, args } if matches!(name.as_str(), "x" | "y" | "d" | "a") => Expr::Call {
            name: name.clone(),
            args: args.clone(),
        },
        Expr::Call { name, args } => Expr::Call {
            name: name.clone(),
            args: args.iter().map(&go).collect::<Result<_, _>>()?,
        },
    })
}

/// A unit's factor, in `base`.
fn factor(u: UnitWord, base: Base, units: Units) -> Result<f64, KindError> {
    use std::f64::consts::PI;
    // a printed length as the reference counts it: the way a written size
    // scales forward is `square / 35`, so a printed length scales back
    let printed = |mm: f64| match base {
        Base::Dmm => mm * (SQUARE_MM / units.square_mm),
        Base::Squares => mm / units.square_mm,
        Base::Mm => mm,
    };
    let refused = |what: &str| {
        unit_err(
            format!("`{what}` is not a unit of a printer"),
            "a printer's numbers are printed lengths: `0.4mm`, `55deg`".to_owned(),
        )
    };
    Ok(match u {
        UnitWord::Pi => PI,
        UnitWord::Deg => 1.0,
        UnitWord::Rad => 180.0 / PI,
        UnitWord::Pct => 0.01,
        UnitWord::Mm => printed(1.0),
        UnitWord::In => printed(25.4),
        UnitWord::Layer => match base {
            Base::Mm => return refused("layer"),
            _ => printed(units.layer_mm),
        },
        UnitWord::Sq => match base {
            Base::Dmm => SQUARE_MM,
            Base::Squares => 1.0,
            Base::Mm => return refused("sq"),
        },
        UnitWord::Dmm => match base {
            Base::Dmm => 1.0,
            Base::Squares => 1.0 / SQUARE_MM,
            Base::Mm => return refused("dmm"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{parse, parse_height};

    fn names(n: &str) -> Option<Kind> {
        match n {
            "$x" | "$y" | "$d" | "$a" => Some(Kind::Plain),
            "$bottom" | "$groundHeight" => Some(Kind::Length),
            "$region" | "$tile" => Some(Kind::Region),
            _ => None,
        }
    }

    fn k(s: &str) -> Result<Kind, KindError> {
        kind_of(&parse_height(s).unwrap(), &names, false)
    }

    #[test]
    fn kinds() {
        assert_eq!(k("8dmm"), Ok(Kind::Length));
        assert_eq!(k("$bottom + 30dmm * (1 - $d * $d)"), Ok(Kind::Length));
        assert_eq!(k("2 pi rad"), Ok(Kind::Angle));
        assert_eq!(k("(2 * pi) rad"), Ok(Kind::Angle));
        assert_eq!(k("4.5 mm"), Ok(Kind::Length));
        assert_eq!(k("sin(360deg * $a)"), Ok(Kind::Plain));
        assert_eq!(k("cos(720deg * ($x - $y))"), Ok(Kind::Plain));
        assert_eq!(k("1 - $d * $d"), Ok(Kind::Plain));
        assert_eq!(k("0"), Ok(Kind::Zero));
        assert_eq!(k("$bottom + 0"), Ok(Kind::Length));
        assert_eq!(k("3sq / 1sq"), Ok(Kind::Plain));
        assert_eq!(k("x($tile, sq)"), Ok(Kind::Plain));
        assert_eq!(k("d(parent($region), %)"), Ok(Kind::Plain));
        assert_eq!(k("a($region, deg)"), Ok(Kind::Plain));
        assert_eq!(k("parent($region)"), Ok(Kind::Region));
        assert_eq!(k("pi"), Ok(Kind::Plain));
    }

    #[test]
    fn mistakes() {
        for bad in [
            "$bottom + 4",
            "$bottom + 20%",
            "$groundHeight + $x",
            "2mm * 3mm",
            "cos(720)",
            "sin($bottom)",
            "$region + 1",
            "1/2rad",
            "x($tile)",
            "x($tile, deg)",
            "a($tile, sq)",
            "x(1, sq)",
            "point($tile, 1, 2)",
        ] {
            assert!(k(bad).is_err(), "{bad}");
        }
        let e = k("1/2rad").unwrap_err();
        assert!(e.help.as_ref().unwrap().contains("(1/2) rad"));
    }

    #[test]
    fn a_bare_number_says_what_to_write() {
        let e = expect(&parse("8").unwrap(), Kind::Length, &names, false).unwrap_err();
        assert!(e.help.as_ref().unwrap().contains("8dmm or 8mm"), "{e:?}");
        assert!(expect(&parse("0").unwrap(), Kind::Length, &names, false).is_ok());
        assert!(expect(&parse("0").unwrap(), Kind::Angle, &names, false).is_ok());
        let e = expect(&parse("90").unwrap(), Kind::Angle, &names, false).unwrap_err();
        assert!(e.help.as_ref().unwrap().contains("90deg"), "{e:?}");
    }

    #[test]
    fn lowered_to_the_reference() {
        let u = Units::default();
        let low = |s: &str, base| {
            let e = parse_height(s).unwrap();
            lower(&e, base, u, None)
                .unwrap()
                .sample(&|_| Some(1.0), 0.0)
                .unwrap()
        };
        // dmm is the reference: no factor, bit for bit
        assert_eq!(low("8dmm", Base::Dmm).to_bits(), 8.0_f64.to_bits());
        assert_eq!(low("0.65sq", Base::Squares).to_bits(), 0.65_f64.to_bits());
        assert_eq!(low("1sq", Base::Dmm), 35.0);
        assert_eq!(low("2mm", Base::Dmm), 2.0);
        assert_eq!(low("1in", Base::Dmm), 25.4);
        assert!((low("2layer", Base::Dmm) - 0.4).abs() < 1e-12);
        assert!((low("2 pi rad", Base::Dmm) - 360.0).abs() < 1e-9);
        assert_eq!(low("90deg", Base::Dmm), 90.0);
        assert!((low("35dmm", Base::Squares) - 1.0).abs() < 1e-12);
        // OpenLOCK: a printed millimetre is more reference millimetres
        let ol = Units {
            square_mm: 25.4,
            layer_mm: 0.2,
        };
        let e = parse_height("1mm").unwrap();
        let v = lower(&e, Base::Dmm, ol, None)
            .unwrap()
            .sample(&|_| None, 0.0)
            .unwrap();
        assert!((v - 35.0 / 25.4).abs() < 1e-12);
    }
}
