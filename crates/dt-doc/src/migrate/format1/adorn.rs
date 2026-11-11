//! Format 0 values written as format 1: the names that gain their `$`,
//! every length its unit, every angle its `deg`, a point flipped to the
//! north-west corner.
//!
//! The rewrite works on the parsed [`Expr`] and prints it back, so a value's
//! tree (and so the order of every float operation) is the one it had; only
//! unit words are added and names renamed.  `dmm` is the reference length
//! (a factor of exactly 1), so no number moves.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::expr::{BinOp, Expr, UnitWord};
use crate::units::Kind;

/// What a place wants of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    Length,
    Angle,
    Plain,
    /// A `let` holds any: the kind comes from what it holds.
    Any,
}

impl Want {
    fn kind(self) -> Option<Kind> {
        match self {
            Want::Length => Some(Kind::Length),
            Want::Angle => Some(Kind::Angle),
            Want::Plain => Some(Kind::Plain),
            Want::Any => None,
        }
    }

    pub(super) fn of_kind(k: Kind) -> Want {
        match k {
            Kind::Length => Want::Length,
            Kind::Angle => Want::Angle,
            _ => Want::Plain,
        }
    }
}

/// Why a value cannot be carried over.
#[derive(Debug, Clone)]
pub(super) struct Flaw {
    pub message: String,
    pub help: Option<String>,
}

pub(super) fn flaw<'h>(message: impl Into<String>, help: impl Into<Option<&'h str>>) -> Flaw {
    Flaw {
        message: message.into(),
        help: help.into().map(str::to_owned),
    }
}

/// The names a value can read, and what each measures.
#[derive(Default)]
pub(super) struct Names {
    /// The library's heights: `$name`, lengths.
    pub library: HashSet<String>,
    /// The plain names in scope (a tile's `let`s, a definition's `param`s).
    pub plain: HashSet<String>,
    /// What each plain name measures, once known.
    pub kinds: HashMap<String, Kind>,
    /// How each plain name was used: for the ones whose kind is found there.
    pub uses: RefCell<Vec<(String, Kind)>>,
    /// The tile's rows, for a read of `y` (a definition has no size).
    pub rows: Option<f64>,
}

impl Names {
    fn kind_of(&self, n: &str) -> Option<Kind> {
        match n {
            "x" | "y" => Some(Kind::Plain),
            "bottom" => Some(Kind::Length),
            _ if self.library.contains(n) => Some(Kind::Length),
            _ => self.kinds.get(n).copied(),
        }
    }

    /// A use of `n` as `k`, for the plain names whose kind is still open.
    pub(super) fn note(&self, n: &str, k: Kind) {
        self.record(n, k);
    }

    fn record(&self, n: &str, k: Kind) {
        if self.plain.contains(n) && !self.kinds.contains_key(n) {
            self.uses.borrow_mut().push((n.to_owned(), k));
        }
    }
}

/// What `e` measures, where its names and calls say so (`None`: nothing in
/// it does, it is only numbers).
fn infer(e: &Expr, names: &Names) -> Option<Kind> {
    match e {
        Expr::Num { .. } | Expr::Unit(_) => None,
        Expr::Name(n) => names.kind_of(n),
        Expr::Neg(a) => infer(a, names),
        Expr::Bin { op, lhs, rhs } => {
            let (l, r) = (infer(lhs, names), infer(rhs, names));
            match op {
                BinOp::Add | BinOp::Sub => l.or(r),
                BinOp::Mul => match (l, r) {
                    (Some(k @ (Kind::Length | Kind::Angle)), _)
                    | (_, Some(k @ (Kind::Length | Kind::Angle))) => Some(k),
                    (Some(Kind::Plain), Some(Kind::Plain)) => Some(Kind::Plain),
                    _ => None,
                },
                BinOp::Div => l,
            }
        }
        Expr::Range { lo, hi } => infer(lo, names).or_else(|| infer(hi, names)),
        Expr::Weighted { range, .. } => infer(range, names),
        Expr::List { items } => items.iter().find_map(|(v, _)| infer(v, names)),
        Expr::Call { name, args } => match name.as_str() {
            "sin" | "cos" | "smooth" => Some(Kind::Plain),
            _ => args.iter().find_map(|a| infer(a, names)),
        },
    }
}

/// What `e` measures once its own names are read: [`infer`] for callers.
pub(super) fn kind_of(e: &Expr, names: &Names) -> Option<Kind> {
    infer(e, names)
}

fn unit(n: f64, u: UnitWord) -> Expr {
    Expr::Bin {
        op: BinOp::Mul,
        lhs: Box::new(Expr::num(n)),
        rhs: Box::new(Expr::Unit(u)),
    }
}

/// Every unit a format 0 value takes: `len` for a length (`dmm`, or `sq` in
/// a point).
#[derive(Clone, Copy)]
pub(super) struct Units {
    pub len: UnitWord,
}

pub(super) const DESIGN: Units = Units { len: UnitWord::Dmm };
pub(super) const SQUARES: Units = Units { len: UnitWord::Sq };

/// `e` written as format 1 for a place that wants `want`.
///
/// # Errors
/// A read that has no format 1 form here (`y` in a part: it has no rows).
pub(super) fn adorn(e: &Expr, want: Want, names: &Names, u: Units) -> Result<Expr, Flaw> {
    let go = |x: &Expr, w: Want| adorn(x, w, names, u);
    Ok(match e {
        Expr::Num {
            value,
            percent: false,
        } => match want {
            Want::Length => unit(*value, u.len),
            Want::Angle => unit(*value, UnitWord::Deg),
            _ => e.clone(),
        },
        Expr::Num { .. } | Expr::Unit(_) => e.clone(),
        Expr::Name(n) => {
            if let Some(k) = want.kind() {
                names.record(n, k);
            }
            match n.as_str() {
                "x" => Expr::Name("$x".to_owned()),
                "bottom" => Expr::Name("$bottom".to_owned()),
                "y" => match names.rows {
                    Some(rows) => Expr::Bin {
                        op: BinOp::Sub,
                        lhs: Box::new(Expr::num(rows)),
                        rhs: Box::new(Expr::Name("$y".to_owned())),
                    },
                    None => {
                        return Err(flaw(
                            "`y` is read in a part, which has no size to flip it against",
                            "format 1 counts `$y` down from the north; in a tile this becomes `rows - $y`; rewrite by hand",
                        ));
                    }
                },
                _ if names.library.contains(n) => Expr::Name(format!("${n}")),
                _ => e.clone(),
            }
        }
        Expr::Neg(a) => Expr::Neg(Box::new(go(a, want)?)),
        Expr::Bin { op, lhs, rhs } => {
            // what a sum, product or quotient measures when the place does not
            // say (a `let`): the first of its names that does
            let want = if want == Want::Any {
                infer(e, names).map_or(Want::Any, Want::of_kind)
            } else {
                want
            };
            let (wl, wr) = match op {
                BinOp::Add | BinOp::Sub => (want, want),
                BinOp::Div => (want, Want::Plain),
                BinOp::Mul => {
                    if matches!(want, Want::Plain | Want::Any) {
                        (want, want)
                    } else {
                        let k = want.kind();
                        let (il, ir) = (infer(lhs, names), infer(rhs, names));
                        if il == k {
                            (want, Want::Plain)
                        } else if ir == k {
                            (Want::Plain, want)
                        } else if ir.is_some() && il.is_none() {
                            (want, Want::Plain)
                        } else if il.is_some() && ir.is_none() {
                            (Want::Plain, want)
                        } else {
                            (want, Want::Plain)
                        }
                    }
                }
            };
            Expr::Bin {
                op: *op,
                lhs: Box::new(go(lhs, wl)?),
                rhs: Box::new(go(rhs, wr)?),
            }
        }
        Expr::Range { lo, hi } => {
            let want = if want == Want::Any {
                infer(e, names).map_or(Want::Any, Want::of_kind)
            } else {
                want
            };
            Expr::Range {
                lo: Box::new(go(lo, want)?),
                hi: Box::new(go(hi, want)?),
            }
        }
        Expr::Weighted {
            name,
            args,
            range,
            parts,
        } => Expr::Weighted {
            name: name.clone(),
            args: args.clone(),
            range: Box::new(go(range, want)?),
            parts: parts.clone(),
        },
        Expr::List { items } => {
            let want = if want == Want::Any {
                infer(e, names).map_or(Want::Any, Want::of_kind)
            } else {
                want
            };
            Expr::List {
                items: items
                    .iter()
                    .map(|(v, w)| Ok((go(v, want)?, *w)))
                    .collect::<Result<_, Flaw>>()?,
            }
        }
        Expr::Call { name, args } => {
            if matches!(name.as_str(), "sin" | "cos")
                && names.rows.is_none()
                && let [arg] = &args[..]
                && let Some(turned) = tide(arg)
            {
                return Ok(Expr::Call {
                    name: name.clone(),
                    args: vec![turned],
                });
            }
            let arg_want = match name.as_str() {
                "sin" | "cos" => Want::Angle,
                "smooth" => Want::Plain,
                _ => want,
            };
            Expr::Call {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| go(a, arg_want))
                    .collect::<Result<_, _>>()?,
            }
        }
    })
}

/// The water's tide, `K * (x + y)` with `K` a whole number of turns (720
/// degrees: two waves a square), in the frame that counts `y` from the
/// north: `K deg * ($x - $y)`.  A whole number of turns a square makes the
/// same wave whatever the rows, so a part, which has no size to flip `y`
/// against, can say it; any other read of `y` there is for a hand.
fn tide(arg: &Expr) -> Option<Expr> {
    let Expr::Bin {
        op: BinOp::Mul,
        lhs,
        rhs,
    } = arg
    else {
        return None;
    };
    let (
        Expr::Num {
            value: k,
            percent: false,
        },
        Expr::Bin { op, lhs: a, rhs: b },
    ) = (&**lhs, &**rhs)
    else {
        return None;
    };
    #[allow(clippy::float_cmp)]
    if k % 360.0 != 0.0 {
        return None;
    }
    let is = |e: &Expr, n: &str| matches!(e, Expr::Name(x) if x == n);
    let dollar = |n: &str| Box::new(Expr::Name(format!("${n}")));
    let (xy, yx) = (is(a, "x") && is(b, "y"), is(a, "y") && is(b, "x"));
    // the first operand keeps its place, so the sum's float order stays
    let inner = match (op, xy, yx) {
        (BinOp::Add, true, _) => Expr::Bin {
            op: BinOp::Sub,
            lhs: dollar("x"),
            rhs: dollar("y"),
        },
        (BinOp::Add, _, true) => Expr::Bin {
            op: BinOp::Sub,
            lhs: dollar("x"),
            rhs: dollar("y"),
        },
        (BinOp::Sub, true, _) => Expr::Bin {
            op: BinOp::Add,
            lhs: dollar("x"),
            rhs: dollar("y"),
        },
        _ => return None,
    };
    Some(Expr::Bin {
        op: BinOp::Mul,
        lhs: Box::new(unit(*k, UnitWord::Deg)),
        rhs: Box::new(inner),
    })
}

/// A point's y in the north-west frame: `rows - y`, worked out in decimals
/// (`1 - 0.1` is `0.9`, not `0.9000000000000000222`), and checked to read
/// back, through the reader's own flip, as the very number it was.
pub(super) fn flipped(rows: f64, y: f64) -> Option<f64> {
    let s = format!("{y}");
    let (int, frac) = s.split_once('.').unwrap_or((s.as_str(), ""));
    if frac.len() > 12 || int.starts_with('-') {
        return None;
    }
    let scale = 10_i128.pow(u32::try_from(frac.len()).ok()?);
    let y_scaled = int.parse::<i128>().ok()? * scale
        + if frac.is_empty() {
            0
        } else {
            frac.parse::<i128>().ok()?
        };
    #[allow(clippy::cast_possible_truncation)]
    let z = rows as i128 * scale - y_scaled;
    if z < 0 {
        return None;
    }
    let text = if frac.is_empty() {
        format!("{z}")
    } else {
        format!("{}.{:0w$}", z / scale, z % scale, w = frac.len())
    };
    let c: f64 = text.parse().ok()?;
    #[allow(clippy::float_cmp)]
    (crate::framed::flip(rows, c) == y).then_some(c)
}

/// A point's y written for format 1, in squares: a number, or a range with
/// its odds turned round (low and high swap: the odds follow the number,
/// which now runs the other way).
pub(super) fn flip_y(y: &Expr, rows: f64, names: &Names) -> Result<Expr, Flaw> {
    let num = |v: f64| {
        flipped(rows, v).ok_or_else(|| {
            flaw(
                format!("`{v}` has no exact north-west form on a {rows}-row tile"),
                "write the point by hand",
            )
        })
    };
    let sq = |v: f64| unit(v, UnitWord::Sq);
    let _ = names;
    match y {
        Expr::Num {
            value,
            percent: false,
        } => Ok(sq(num(*value)?)),
        Expr::Neg(inner) => match &**inner {
            Expr::Num { value, .. } => Ok(sq(num(-value)?)),
            _ => Err(flaw("a point's y is a number or a range", None)),
        },
        Expr::Range { lo, hi } => {
            let (a, b) = (const_of(lo)?, const_of(hi)?);
            Ok(Expr::Range {
                lo: Box::new(sq(num(b)?)),
                hi: Box::new(sq(num(a)?)),
            })
        }
        Expr::Weighted {
            name,
            args,
            range,
            parts,
        } if parts.is_empty() && matches!(name.as_str(), "flat" | "low" | "high" | "bell") => {
            let Expr::Range { lo, hi } = &**range else {
                return Err(flaw("a weighted y is over a range", None));
            };
            let (a, b) = (const_of(lo)?, const_of(hi)?);
            let swapped = match name.as_str() {
                "low" => "high",
                "high" => "low",
                other => other,
            };
            Ok(Expr::Weighted {
                name: swapped.to_owned(),
                args: args.clone(),
                range: Box::new(Expr::Range {
                    lo: Box::new(sq(num(b)?)),
                    hi: Box::new(sq(num(a)?)),
                }),
                parts: Vec::new(),
            })
        }
        _ => Err(flaw(
            "this y is more than a number or a range",
            "write the point by hand: `(x, rows - y)` in the north-west frame",
        )),
    }
}

fn const_of(e: &Expr) -> Result<f64, Flaw> {
    match e {
        Expr::Num {
            value,
            percent: false,
        } => Ok(*value),
        Expr::Neg(a) => const_of(a).map(|v| -v),
        _ => Err(flaw("a point's y is a number or a range of numbers", None)),
    }
}

// ---- printing ----------------------------------------------------------

fn unit_literal(e: &Expr) -> Option<(&Expr, UnitWord)> {
    match e {
        Expr::Bin {
            op: BinOp::Mul,
            lhs,
            rhs,
        } => match (&**lhs, &**rhs) {
            (Expr::Num { percent: false, .. }, Expr::Unit(u)) => Some((lhs, *u)),
            _ => None,
        },
        _ => None,
    }
}

fn prec(e: &Expr) -> u8 {
    if unit_literal(e).is_some() {
        return 4;
    }
    match e {
        Expr::Bin {
            op: BinOp::Add | BinOp::Sub,
            ..
        } => 1,
        Expr::Bin { .. } => 2,
        Expr::Neg(_) => 3,
        Expr::Num { value, .. } if *value < 0.0 => 3,
        _ => 4,
    }
}

fn operand(e: &Expr, min: u8, v2: bool) -> String {
    let s = write(e, v2, false);
    if prec(e) < min { format!("({s})") } else { s }
}

fn number(value: f64) -> String {
    if value < 0.0 {
        format!("-{}", -value)
    } else {
        format!("{value}")
    }
}

fn write(e: &Expr, v2: bool, top: bool) -> String {
    if let Some((n, u)) = unit_literal(e) {
        let Expr::Num { value, .. } = n else {
            unreachable!()
        };
        return format!("{}{}", number(*value), u.word());
    }
    match e {
        Expr::Num { value, percent } => {
            format!("{}{}", number(*value), if *percent { "%" } else { "" })
        }
        Expr::Name(n) => n.clone(),
        Expr::Unit(u) => u.word().to_owned(),
        Expr::Neg(a) => format!("-{}", operand(a, 4, v2)),
        Expr::Bin { op, lhs, rhs } => {
            let (c, p) = match op {
                BinOp::Add => ('+', 1),
                BinOp::Sub => ('-', 1),
                BinOp::Mul => ('*', 2),
                BinOp::Div => ('/', 2),
            };
            format!("{} {c} {}", operand(lhs, p, v2), operand(rhs, p + 1, v2))
        }
        Expr::Range { lo, hi } => {
            if v2 {
                format!("[{} .. {}]", operand(lo, 1, v2), operand(hi, 1, v2))
            } else if top {
                format!("{} .. {}", operand(lo, 1, v2), operand(hi, 1, v2))
            } else {
                format!("{}..{}", operand(lo, 1, v2), operand(hi, 1, v2))
            }
        }
        Expr::Weighted {
            name,
            args,
            range,
            parts,
        } => {
            let inner = write(range, v2, true);
            if name == "mix" {
                let mut out = String::from("mix(");
                for (i, ((n, a), w)) in parts.iter().zip(args).enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&format!("{n}({inner}"));
                    for x in a {
                        out.push_str(&format!(", {x}"));
                    }
                    out.push_str(&format!("), {w}"));
                }
                out.push(')');
                out
            } else {
                let mut out = format!("{name}({inner}");
                for a in args {
                    out.push_str(&format!(", {a}"));
                }
                out.push(')');
                out
            }
        }
        Expr::List { items } => {
            let parts: Vec<String> = items
                .iter()
                .map(|(v, w)| match w {
                    Some(w) => format!("{}:{w}", write(v, v2, false)),
                    None => write(v, v2, false),
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::Call { name, args } => {
            let parts: Vec<String> = args.iter().map(|a| write(a, v2, false)).collect();
            format!("{name}({})", parts.join(", "))
        }
    }
}

/// `e` as the text of a value: `v2` for the heights language (a range is
/// `[low .. high]`), else a control's (`low .. high`).
pub(super) fn show(e: &Expr, v2: bool) -> String {
    write(e, v2, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{parse, parse_units, parse_v2, parse_v2_units};

    fn names() -> Names {
        let mut n = Names::default();
        n.library.insert("groundHeight".to_owned());
        n.library.insert("waterSurfaceHeight".to_owned());
        n.rows = Some(2.0);
        n
    }

    fn height(src: &str, want: Want) -> String {
        let e = parse_v2(src).unwrap();
        let out = adorn(&e, want, &names(), DESIGN).unwrap();
        let text = show(&out, true);
        // what is written reads back as the same tree
        assert_eq!(parse_v2_units(&text).unwrap(), out, "{text}");
        text
    }

    fn control(src: &str, want: Want) -> String {
        let e = parse(src).unwrap();
        let out = adorn(&e, want, &names(), DESIGN).unwrap();
        let text = show(&out, false);
        assert_eq!(parse_units(&text).unwrap(), out, "{text}");
        text
    }

    #[test]
    fn heights_gain_dollars_and_units() {
        assert_eq!(
            height("groundHeight + 1", Want::Length),
            "$groundHeight + 1dmm"
        );
        assert_eq!(
            height("[groundHeight .. groundHeight + 1]", Want::Length),
            "[$groundHeight .. $groundHeight + 1dmm]"
        );
        assert_eq!(height("bottom + 40", Want::Length), "$bottom + 40dmm");
        assert_eq!(height("8", Want::Length), "8dmm");
        assert_eq!(
            height("bottom + [0.6 .. 1.2]", Want::Length),
            "$bottom + [0.6dmm .. 1.2dmm]"
        );
    }

    #[test]
    fn the_tide_reads_the_other_way_down() {
        assert_eq!(
            height(
                "waterSurfaceHeight + 0.6 * cos(720 * (x + y))",
                Want::Length
            ),
            "$waterSurfaceHeight + 0.6dmm * cos(720deg * ($x + (2 - $y)))"
        );
    }

    #[test]
    fn controls_take_units_at_both_ends() {
        assert_eq!(control("6..10", Want::Length), "6dmm .. 10dmm");
        assert_eq!(
            control("low(1..6, 11)", Want::Length),
            "low(1dmm .. 6dmm, 11)"
        );
        assert_eq!(control("34", Want::Angle), "34deg");
        assert_eq!(control("0.85", Want::Plain), "0.85");
    }

    #[test]
    fn a_product_puts_the_unit_on_the_length() {
        assert_eq!(
            height("bottom + 30 * (1 - x)", Want::Length),
            "$bottom + 30dmm * (1 - $x)"
        );
    }

    #[test]
    fn y_flips_to_exact_values() {
        for (rows, y) in [
            (1.0, 0.67),
            (2.0, 0.3),
            (3.0, 1.1),
            (2.0, 0.55),
            (1.0, 0.38),
        ] {
            let f = flipped(rows, y).unwrap();
            #[allow(clippy::float_cmp)]
            {
                assert_eq!(crate::framed::flip(rows, f), y, "{rows} {y} {f}");
            }
        }
    }
}
