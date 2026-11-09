//! A written value as a built element takes it.
//!
//! A control value is a number, a word, a range, or a weighted range.  A
//! tile's `let` names resolve here: a `let` that is one number is that
//! number; a `let` that holds a range (weighted or not) holds a
//! distribution, and every use of its name is its own draw, made by the
//! control that names it (each control draws from its own key, so two uses
//! never share a roll).  What a control may be is the schema's and the
//! element's; this only turns what is written into a value.

use dt_doc::consts::ConstTable;
use dt_doc::expr::Expr;
use dt_doc::model::{Num, Value};
use dt_doc::schema;
use dt_geom::{Dist, Weighting};

/// The tile's `let`s as an element control sees them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lets {
    /// Each name that is one number, in mm (a range reads as its middle
    /// where only a number is wanted, such as the end of another range).
    pub nums: Vec<(String, f64)>,
    /// Each name that holds a range, directly or through another name: its
    /// expression, to be lowered where it is used.
    pub draws: Vec<(String, Expr)>,
}

impl Lets {
    /// Every constant of the table, as a number and, when it draws, as its
    /// expression.
    pub(crate) fn from_consts(consts: &ConstTable) -> Self {
        let mut l = Self::default();
        for e in consts.entries() {
            if let Ok(mm) = consts.scalar(&e.expr, 0.5, &|_| None) {
                l.nums.push((e.name.clone(), mm));
            }
            if consts.draws(&e.expr) {
                l.draws.push((e.name.clone(), e.expr.clone()));
            }
        }
        l
    }

    fn num(&self, name: &str) -> Option<f64> {
        self.nums
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| *v)
    }

    fn draw(&self, name: &str) -> Option<&Expr> {
        self.draws
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, e)| e)
    }

    /// True when the value holds a range or names a `let` that does.
    pub(crate) fn draws_in(&self, e: &Expr) -> bool {
        e.range_count() > 0 || e.names().iter().any(|n| self.draw(n).is_some())
    }
}

/// Why a value could not be lowered, with the diagnostic code it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    pub code: &'static str,
    pub message: String,
    pub help: Option<String>,
    /// A mistake in the document, shown where it was made.  A soft one is a
    /// name with no value yet (a definition's parameter): `check` accepts it
    /// and `build` refuses it.
    pub hard: bool,
}

impl Refused {
    /// A name with no value yet.
    fn value(message: impl Into<String>) -> Self {
        Self {
            code: "dt::value",
            message: message.into(),
            help: None,
            hard: false,
        }
    }

    /// A mistake in the document.
    fn mistake(message: impl Into<String>) -> Self {
        Self {
            hard: true,
            ..Self::value(message)
        }
    }

    fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl From<String> for Refused {
    fn from(message: String) -> Self {
        Self::value(message)
    }
}

/// A value lowered, and a note when a weighting could not matter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lowered {
    pub value: dt_elements::Value,
    pub note: Option<String>,
}

impl Lowered {
    fn plain(value: dt_elements::Value) -> Self {
        Self { value, note: None }
    }
}

/// A value as a built element takes it: one number in mm, a range, a
/// weighted range, or a word.
pub(crate) fn element_value(v: &Value, lets: &Lets) -> Result<Lowered, Refused> {
    Ok(match v {
        Value::Num(n) => number(n, lets)?,
        Value::Word(w) => Lowered::plain(dt_elements::Value::Word(w.word.to_owned())),
        Value::Bool(v) => Lowered::plain(dt_elements::Value::Word(
            if *v { "#true" } else { "#false" }.to_owned(),
        )),
        Value::Tag(t) | Value::Text(t) => Lowered::plain(dt_elements::Value::Word(t.clone())),
        Value::Param(p) => {
            return Err(Refused::value(format!(
                "the parameter `{p}` has no value here"
            )));
        }
    })
}

/// What a lowered expression is: a number, or one range with its odds.
#[derive(Clone, Copy, Debug)]
enum Part {
    Num(f64),
    Dist(Dist),
}

fn number(n: &Num, lets: &Lets) -> Result<Lowered, Refused> {
    let env = |name: &str| lets.num(name);
    let named_draw = n.expr.names().iter().any(|x| lets.draw(x).is_some());
    let weighted = {
        let mut any = false;
        visit(&n.expr, &mut |e| any |= matches!(e, Expr::Weighted { .. }));
        any
    };
    // The plain cases are as they were: one range is its two ends, one number
    // is itself.
    if !named_draw && !weighted {
        return Ok(Lowered::plain(if n.expr.is_range() {
            let (a, b) = n.bounds(&env)?;
            dt_elements::Value::Range(a, b)
        } else {
            dt_elements::Value::Number(n.mm(&lets.nums)?)
        }));
    }
    let k = if n.unit == Some(dt_doc::expr::Unit::H) {
        schema::H_MM
    } else {
        1.0
    };
    match lower(&n.expr, lets, 0)? {
        Part::Num(v) => Ok(Lowered::plain(dt_elements::Value::Number(v * k))),
        Part::Dist(d) => {
            let d = Dist::new(d.lo * k, d.hi * k, d.weighting);
            let note = (d.is_point() && !matches!(d.weighting, Weighting::Flat))
                .then(|| format!("`{}` has no width, so its weighting has no effect", n.expr));
            Ok(Lowered {
                value: if d.weighting.is_flat() || d.is_point() {
                    dt_elements::Value::Range(d.lo, d.hi)
                } else {
                    dt_elements::Value::Dist(d)
                },
                note,
            })
        }
    }
}

fn visit<'a>(e: &'a Expr, f: &mut impl FnMut(&'a Expr)) {
    f(e);
    match e {
        Expr::Num { .. } | Expr::Name(_) => {}
        Expr::Neg(a) => visit(a, f),
        Expr::Bin { lhs, rhs, .. } | Expr::Range { lo: lhs, hi: rhs } => {
            visit(lhs, f);
            visit(rhs, f);
        }
        Expr::Weighted { range, .. } => visit(range, f),
        Expr::Call { args, .. } => args.iter().for_each(|a| visit(a, f)),
    }
}

const DEPTH: usize = 32;

fn lower(e: &Expr, lets: &Lets, depth: usize) -> Result<Part, Refused> {
    if depth > DEPTH {
        return Err(Refused::mistake("`let`s refer to each other in a loop"));
    }
    let env = |name: &str| lets.num(name);
    if !lets.draws_in(e) {
        return Ok(Part::Num(e.sample(&env, 0.0)?));
    }
    Ok(match e {
        Expr::Name(n) => match lets.draw(n) {
            Some(x) => lower(x, lets, depth + 1)?,
            None => Part::Num(e.sample(&env, 0.0)?),
        },
        Expr::Neg(a) => scale(lower(a, lets, depth)?, -1.0),
        Expr::Bin { op, lhs, rhs } => {
            use dt_doc::expr::BinOp::{Add, Div, Mul, Sub};
            let (a, b) = (lower(lhs, lets, depth)?, lower(rhs, lets, depth)?);
            match (op, a, b) {
                (Add, Part::Num(x), Part::Num(y)) => Part::Num(x + y),
                (Sub, Part::Num(x), Part::Num(y)) => Part::Num(x - y),
                (Mul, Part::Num(x), Part::Num(y)) => Part::Num(x * y),
                (Div, Part::Num(x), Part::Num(y)) => Part::Num(x / y),
                (Add, Part::Dist(d), Part::Num(x)) | (Add, Part::Num(x), Part::Dist(d)) => {
                    Part::Dist(Dist::new(d.lo + x, d.hi + x, d.weighting))
                }
                (Sub, Part::Dist(d), Part::Num(x)) => {
                    Part::Dist(Dist::new(d.lo - x, d.hi - x, d.weighting))
                }
                // x - (lo + (hi - lo) t) runs from x - lo to x - hi
                (Sub, Part::Num(x), Part::Dist(d)) => ordered(x - d.lo, x - d.hi, d.weighting),
                (Mul, Part::Dist(d), Part::Num(x)) | (Mul, Part::Num(x), Part::Dist(d)) => {
                    scale(Part::Dist(d), x)
                }
                (Div, Part::Dist(d), Part::Num(x)) => ordered(d.lo / x, d.hi / x, d.weighting),
                (Div, Part::Num(_), Part::Dist(_)) => {
                    return Err(Refused::mistake(format!(
                        "`{e}` divides by a range: the odds of its result are not a weighting"
                    ))
                    .with_help("divide a range by a number, not a number by a range"));
                }
                _ => {
                    return Err(Refused::mistake(format!(
                        "`{e}` combines two ranges in one control"
                    ))
                    .with_help(
                        "a control draws one range: write the range once, or draw the other in its own control",
                    ));
                }
            }
        }
        Expr::Range { .. } => {
            let (lo, hi) = ends(e, lets)?;
            Part::Dist(Dist::flat(lo, hi))
        }
        Expr::Weighted { name, args, range } => {
            let (lo, hi) = ends(range, lets)?;
            let w = Weighting::from_call(name, args).map_err(|w| Refused {
                code: "dt::range",
                message: w.message,
                help: Some(w.help),
                hard: true,
            })?;
            Part::Dist(Dist::new(lo, hi, w))
        }
        Expr::Call { name, .. } => {
            return Err(Refused::mistake(format!(
                "`{name}` is given a range: a function takes numbers"
            )));
        }
        Expr::Num { .. } => Part::Num(e.sample(&env, 0.0)?),
    })
}

/// A range's two ends, evaluated as a plain range's always were (the
/// expression at 0 and at 1); a range inside an end is refused.
fn ends(range: &Expr, lets: &Lets) -> Result<(f64, f64), Refused> {
    let Expr::Range { lo, hi } = range else {
        return Err(Refused::mistake("a weighting wraps a range"));
    };
    for end in [lo, hi] {
        if lets.draws_in(end) {
            return Err(Refused::mistake(format!(
                "`{end}` is a range inside the end of another range: the ends of a range are numbers"
            )));
        }
    }
    let env = |name: &str| lets.num(name);
    Ok(range.bounds(&env)?)
}

/// A range from `lo` to `hi` with odds that run from `lo`: swapped, with the
/// odds read from the other end, when `lo` is the larger.
fn ordered(lo: f64, hi: f64, w: Weighting) -> Part {
    if lo <= hi {
        Part::Dist(Dist::new(lo, hi, w))
    } else {
        Part::Dist(Dist::new(hi, lo, w.mirrored()))
    }
}

/// A part times a number; a negative number mirrors a range.
fn scale(p: Part, k: f64) -> Part {
    match p {
        Part::Num(x) => Part::Num(x * k),
        Part::Dist(d) => ordered(d.lo * k, d.hi * k, d.weighting),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_doc::expr::parse_v2;

    fn lets(defs: &[(&str, &str)]) -> Lets {
        let mut l = Lets::default();
        for (n, src) in defs {
            let e = parse_v2(src).unwrap();
            l.nums
                .push(((*n).to_owned(), e.sample(&|_| None, 0.5).unwrap()));
            if e.range_count() > 0 {
                l.draws.push(((*n).to_owned(), e));
            }
        }
        l
    }

    fn dist(src: &str, l: &Lets) -> Dist {
        match lower(&parse_v2(src).unwrap(), l, 0).unwrap() {
            Part::Dist(d) => d,
            Part::Num(x) => panic!("{src} is the number {x}"),
        }
    }

    #[test]
    fn a_name_holds_a_distribution_and_keeps_its_odds() {
        let l = lets(&[("small", "low(1..6, 11)"), ("pair", "[1 .. 2]")]);
        assert_eq!(dist("small", &l), Dist::new(1.0, 6.0, Weighting::Low(11.0)));
        // scaled and moved, the odds go with it
        assert_eq!(
            dist("2 * small", &l),
            Dist::new(2.0, 12.0, Weighting::Low(11.0))
        );
        assert_eq!(
            dist("small + 1", &l),
            Dist::new(2.0, 7.0, Weighting::Low(11.0))
        );
        // a negative factor mirrors it, and the odds are read from the other end
        let d = dist("-small", &l);
        assert_eq!(
            (d.lo, d.hi, d.weighting),
            (-6.0, -1.0, Weighting::High(11.0))
        );
        let d = dist("10 - small", &l);
        assert_eq!((d.lo, d.hi, d.weighting), (4.0, 9.0, Weighting::High(11.0)));
        assert_eq!(dist("pair", &l), Dist::flat(1.0, 2.0));
    }

    #[test]
    fn two_ranges_in_one_control_are_refused() {
        let l = lets(&[("s", "[1 .. 2]")]);
        let e = lower(&parse_v2("s - s").unwrap(), &l, 0).unwrap_err();
        assert_eq!(e.code, "dt::value");
        assert!(e.message.contains("two ranges"), "{e:?}");
        let e = lower(&parse_v2("3 / s").unwrap(), &l, 0).unwrap_err();
        assert!(e.message.contains("divides by a range"), "{e:?}");
    }

    #[test]
    fn a_strength_out_of_range_is_dt_range() {
        let l = lets(&[]);
        let e = lower(&parse_v2("low(1..6, -2)").unwrap(), &l, 0).unwrap_err();
        assert_eq!(e.code, "dt::range");
        assert_eq!(e.message, "low's ratio must be above 0, not -2");
    }

    #[test]
    fn a_name_that_refers_to_itself_is_refused() {
        // not reachable from a document (a name is declared first), but the
        // walk is bounded all the same
        let mut l = lets(&[]);
        l.draws.push(("a".into(), parse_v2("[1 .. a]").unwrap()));
        assert!(lower(&Expr::Name("a".into()), &l, 0).is_err());
    }
}
