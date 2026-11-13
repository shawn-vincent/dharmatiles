//! A written value as a rolled number: `let` names resolve here, and a
//! number, a range or a weighted range is lowered to what an element draws
//! from ([`Part`]).
//!
//! A control value is a number, a word, a range, or a weighted range.  A
//! tile's `let` names resolve here: a `let` that is one number is that
//! number; a `let` that holds a range (weighted or not) holds a
//! distribution, and every use of its name is its own draw, made by the
//! control that names it (each control draws from its own key, so two uses
//! never share a roll).  What a control may be is the schema's and the
//! element's; this only turns what is written into a value.  It has no
//! geometry, so the browser and the program lower a range the same way.

use crate::consts::ConstTable;
use crate::expr::{Algebra, BinOp, Expr, Odds, UnitWord};
use dt_value::{Dist, Draw, Op, Weighting};

/// The tile's `let`s as an element control sees them.
#[derive(Clone, Debug, Default)]
pub struct Lets {
    /// Each name that is one number, in mm (a range reads as its middle
    /// where only a number is wanted, such as the end of another range).
    pub nums: Vec<(String, f64)>,
    /// Each name that holds a range, directly or through another name: its
    /// expression, to be lowered where it is used.
    pub draws: Vec<(String, Expr)>,
    /// The context these were made from (format 1: the `$` names of this
    /// scope), which also says what a formula may read here.
    pub table: Option<std::sync::Arc<ConstTable>>,
}

impl Lets {
    /// Every constant of the table, as a number and, when it draws, as its
    /// expression.
    pub fn from_consts(consts: &ConstTable) -> Self {
        let mut l = Self {
            table: Some(std::sync::Arc::new(consts.clone())),
            ..Self::default()
        };
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

    pub fn num(&self, name: &str) -> Option<f64> {
        self.nums
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| *v)
    }

    pub fn draw(&self, name: &str) -> Option<&Expr> {
        self.draws
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, e)| e)
    }

    /// True when the value holds a range or names a `let` that does.
    pub fn draws_in(&self, e: &Expr) -> bool {
        e.range_count() > 0 || e.names().iter().any(|n| self.draw(n).is_some())
    }
}

/// Why a value could not be lowered, with the diagnostic code it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
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
    pub fn value(message: impl Into<String>) -> Self {
        Self {
            code: "dt::value",
            message: message.into(),
            help: None,
            hard: false,
        }
    }

    /// A mistake in the document.
    pub fn mistake(message: impl Into<String>) -> Self {
        Self {
            hard: true,
            ..Self::value(message)
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl From<String> for Refused {
    fn from(message: String) -> Self {
        Self::value(message)
    }
}

/// What a lowered expression is: a number, one range with its odds (which
/// shifts and scales without losing them), or several rolled and summed.
#[derive(Clone, Debug)]
pub enum Part {
    Num(f64),
    Dist(Dist),
    Draw(Draw),
}

impl Part {
    pub fn into_draw(self) -> Draw {
        match self {
            Self::Num(x) => Draw::Num(x),
            Self::Dist(d) => Draw::Range(d),
            Self::Draw(d) => d,
        }
    }
}

/// One coordinate of an `at=` point, in squares: a number, or a range with
/// its odds, drawn once from the entry's seed.  A number is a range of no
/// width, as a control's number is, so it takes its draw as before.
pub fn coordinate(e: &Expr) -> Result<Draw, Refused> {
    let lets = Lets::default();
    if !lets.draws_in(e) {
        let v = e.sample(&|_| None, 0.0)?;
        return Ok(Draw::flat(v, v));
    }
    Ok(lower(e, &lets, 0)?.into_draw())
}

const DEPTH: usize = 32;

pub fn lower(e: &Expr, lets: &Lets, depth: usize) -> Result<Part, Refused> {
    e.eval(&mut Lowerer { lets, depth })
}

/// The rolled values a control takes: a number, one range with its odds, or
/// several rolled and combined ([`Part`]).  A value with no range in it is
/// one number; every range in a value that has some is its own roll.
struct Lowerer<'a> {
    lets: &'a Lets,
    /// How many names deep: a `let` that names itself never ends.
    depth: usize,
}

impl Lowerer<'_> {
    fn env(&self) -> impl Fn(&str) -> Option<f64> + '_ {
        |name: &str| self.lets.num(name)
    }
}

/// The two ends of a range expression (the range itself, or the weighting
/// call round it).
fn ends_of(outer: &Expr) -> Option<(&Expr, &Expr)> {
    match outer {
        Expr::Range { lo, hi } => Some((lo, hi)),
        Expr::Weighted { range, .. } => ends_of(range),
        _ => None,
    }
}

impl Algebra for Lowerer<'_> {
    type Val = Part;
    type Err = Refused;

    fn shortcut(&mut self, e: &Expr) -> Option<Result<Part, Refused>> {
        if self.lets.draws_in(e) {
            return None;
        }
        Some(
            e.sample(&self.env(), 0.0)
                .map(Part::Num)
                .map_err(Refused::from),
        )
    }

    fn number(&mut self, value: f64) -> Result<Part, Refused> {
        Ok(Part::Num(value))
    }

    fn name(&mut self, n: &str) -> Result<Part, Refused> {
        match self.lets.draw(n) {
            Some(x) => {
                if self.depth + 1 > DEPTH {
                    return Err(Refused::mistake("`let`s refer to each other in a loop"));
                }
                self.depth += 1;
                let r = x.eval(self);
                self.depth -= 1;
                r
            }
            None => self
                .lets
                .num(n)
                .map(Part::Num)
                .ok_or_else(|| Refused::from(format!("unknown name `{n}`"))),
        }
    }

    fn unit(&mut self, unit: UnitWord) -> Result<Part, Refused> {
        Err(Refused::from(format!(
            "`{}` is a unit, not a number here",
            unit.word()
        )))
    }

    fn neg(&mut self, a: Part) -> Result<Part, Refused> {
        Ok(scale(a, -1.0))
    }

    fn binary(&mut self, e: &Expr, op: BinOp, a: Part, b: Part) -> Result<Part, Refused> {
        use crate::expr::BinOp::{Add, Div, Mul, Sub};
        Ok(match (op, a, b) {
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
            (Div, _, Part::Dist(_) | Part::Draw(_)) => {
                return Err(Refused::mistake(format!(
                    "`{e}` divides by a range: the odds of its result are not a weighting"
                ))
                .with_help("divide a range by a number, not by a range"));
            }
            // anything else holds two ranges or more: each rolls on its
            // own, then the arithmetic runs on the numbers
            (op, a, b) => {
                let op = match op {
                    Add => Op::Add,
                    Sub => Op::Sub,
                    Mul => Op::Mul,
                    Div => Op::Div,
                };
                Part::Draw(Draw::Bin(
                    op,
                    Box::new(a.into_draw()),
                    Box::new(b.into_draw()),
                ))
            }
        })
    }

    fn range(
        &mut self,
        outer: &Expr,
        lo: Part,
        hi: Part,
        odds: Option<Odds<'_>>,
    ) -> Result<Part, Refused> {
        // a range's two ends, as a plain range's always were (the range at 0
        // and at 1); a range inside an end is refused
        let (Part::Num(a), Part::Num(b)) = (lo, hi) else {
            let end = ends_of(outer).map(|(l, h)| if self.lets.draws_in(l) { l } else { h });
            return Err(Refused::mistake(format!(
                "`{}` is a range inside the end of another range: the ends of a range are numbers",
                end.map_or_else(String::new, ToString::to_string)
            )));
        };
        let (lo, hi) = (a + (b - a) * 0.0, a + (b - a) * 1.0);
        let Some(o) = odds else {
            return Ok(Part::Dist(Dist::flat(lo, hi)));
        };
        let w = if o.name == "mix" {
            let mut made = Vec::with_capacity(o.parts.len());
            for ((n, a), weight) in o.parts.iter().zip(o.args) {
                made.push((*weight, Weighting::from_call(n, a).map_err(refused)?));
            }
            Weighting::mix(made)
        } else {
            Weighting::from_call(o.name, o.args)
        }
        .map_err(refused)?;
        Ok(Part::Dist(Dist::new(lo, hi, w)))
    }

    fn list(&mut self, e: &Expr, items: Vec<(Part, Option<f64>)>) -> Result<Part, Refused> {
        let mut values = Vec::with_capacity(items.len());
        for (k, (v, w)) in items.into_iter().enumerate() {
            let Part::Num(x) = v else {
                let item = match e {
                    Expr::List { items } => {
                        items.get(k).map_or_else(String::new, |i| i.0.to_string())
                    }
                    _ => String::new(),
                };
                return Err(Refused::mistake(format!(
                    "`{item}` is a range inside a list: the values of a list are numbers"
                ))
                .with_help("write numbers: [1:4, 2:5, 3:1]"));
            };
            values.push((x, w));
        }
        Ok(Part::Dist(Dist::list(&values).map_err(refused)?))
    }

    fn call(&mut self, _e: &Expr, name: &str, _args: Vec<Part>) -> Result<Part, Refused> {
        Err(Refused::mistake(format!(
            "`{name}` is given a range: a function takes numbers"
        )))
    }
}

/// A weighting that cannot be made, as the diagnostic it is.
fn refused(w: dt_value::WeightingError) -> Refused {
    Refused {
        code: w.code,
        message: w.message,
        help: Some(w.help),
        hard: true,
    }
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
        Part::Draw(d) => Part::Draw(Draw::Bin(Op::Mul, Box::new(d), Box::new(Draw::Num(k)))),
    }
}

/// One range as the editor's gallery reads it: its ends and odds, and how
/// it is written (the function and its numbers after the range).
#[derive(Clone, Debug)]
pub struct Ranged {
    /// The range with its odds.
    pub dist: Dist,
    /// The function: `flat` for a plain range, `list`, `mix` or one of the calls.
    pub function: String,
    /// The numbers after the range (the weights, for a list or a mix).
    pub args: Vec<f64>,
}

/// Why a written value is not one range with its odds: the diagnostic the
/// program gives it.
#[derive(Clone, Debug)]
pub struct RangeRefusal {
    /// The diagnostic code, such as `dt::range`.
    pub code: String,
    /// What is wrong.
    pub message: String,
    /// What to write.
    pub help: Option<String>,
}

fn not_a_range(message: impl Into<String>, help: &str) -> RangeRefusal {
    RangeRefusal {
        code: "dt::value".to_owned(),
        message: message.into(),
        help: Some(help.to_owned()),
    }
}

/// Read `src`, a control's value, as one range with its odds.  The gallery
/// asks the program this for every curve it draws, so the odds are never
/// worked out in the page.
///
/// # Errors
/// The value does not parse, is not one range (a number, a sum, a name), or
/// its weighting is refused (`dt::range`, `dt::weighting`) as a document's
/// would be.
pub fn ranged(src: &str) -> Result<Ranged, RangeRefusal> {
    let e = crate::expr::parse(src.trim()).map_err(|e| read_refusal(src, &e.message))?;
    ranged_expr(src, &e)
}

/// [`ranged`] for a control value of format 1, which has units after its
/// numbers (`low(1dmm .. 6dmm, 11)`): they are lowered as the reader lowers
/// them, so the range is in design millimetres at the 35 mm reference.
///
/// # Errors
/// As [`ranged`].
pub fn ranged_in(src: &str, units: crate::units::Units) -> Result<Ranged, RangeRefusal> {
    let e = crate::expr::parse_units(src.trim()).map_err(|e| read_refusal(src, &e.message))?;
    let e = crate::units::lower(&e, crate::units::Base::Dmm, units, None).map_err(|e| {
        RangeRefusal {
            code: e.code.to_owned(),
            message: e.message,
            help: e.help,
        }
    })?;
    ranged_expr(src, &e)
}

fn read_refusal(src: &str, why: &str) -> RangeRefusal {
    not_a_range(
        format!("`{}` does not read as a value: {why}", src.trim()),
        "write a range such as 1..6, or a weighted one such as low(1..6, 4)",
    )
}

fn ranged_expr(src: &str, e: &Expr) -> Result<Ranged, RangeRefusal> {
    let lets = Lets::default();
    let part = lower(e, &lets, 0).map_err(|r| RangeRefusal {
        code: r.code.to_owned(),
        message: r.message,
        help: r.help,
    })?;
    let Part::Dist(dist) = part else {
        return Err(not_a_range(
            format!("`{}` is not one range", src.trim()),
            "the gallery weights one range such as 1..6",
        ));
    };
    let (function, args) = match e {
        Expr::Weighted { name, args, .. } => (name.clone(), args.clone()),
        Expr::List { items } => (
            "list".to_owned(),
            items.iter().map(|i| i.1.unwrap_or(1.0)).collect(),
        ),
        _ => ("flat".to_owned(), Vec::new()),
    };
    Ok(Ranged {
        dist,
        function,
        args,
    })
}

/// What is wrong with the weighting in a typed value, if anything: a
/// number past its limits (`dt::range`), a list that repeats a value or any
/// other `dt::weighting` refusal, as a build would say.  The value's units
/// are lowered first, as the reader lowers them (`low(1dmm .. 6dmm, 11)`).
/// `None` for a value that does not parse (the reader's check says so) or
/// that names things only the document knows.
#[must_use]
pub fn weighting_flaw_in(src: &str, units: crate::units::Units) -> Option<RangeRefusal> {
    let e = crate::expr::parse_units(src.trim()).ok()?;
    let e = crate::units::lower(&e, crate::units::Base::Dmm, units, None).ok()?;
    if e.range_count() == 0 {
        return None;
    }
    let r = lower(&e, &Lets::default(), 0).err()?;
    (r.hard && matches!(r.code, "dt::range" | "dt::weighting")).then(|| RangeRefusal {
        code: r.code.to_owned(),
        message: r.message,
        help: r.help,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parse_v2;

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
            other => panic!("{src} is not one range: {other:?}"),
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

    fn draw(src: &str, l: &Lets) -> Draw {
        match lower(&parse_v2(src).unwrap(), l, 0).unwrap() {
            Part::Draw(d) => d,
            other => panic!("{src} is not a sum: {other:?}"),
        }
    }

    #[test]
    fn two_ranges_in_one_control_are_two_rolls() {
        let l = lets(&[("s", "[1 .. 2]")]);
        // a name used twice is two leaves, each its own ordinal
        let d = draw("s - s", &l);
        assert_eq!(d.ranges(), 2);
        assert_eq!(d.bounds(), (-1.0, 1.0));
        let d = draw("[1 .. 2] + [3 .. 4]", &l);
        assert_eq!(d.bounds(), (4.0, 6.0));
        // a number shifts it, a negative factor mirrors it
        assert_eq!(draw("2 * ([1 .. 2] + [3 .. 4])", &l).bounds(), (8.0, 12.0));
        assert_eq!(draw("-([1 .. 2] + [3 .. 4])", &l).bounds(), (-6.0, -4.0));
        assert_eq!(draw("[1 .. 2] - [3 .. 4]", &l).bounds(), (-3.0, -1.0));
        let e = lower(&parse_v2("3 / s").unwrap(), &l, 0).unwrap_err();
        assert!(e.message.contains("divides by a range"), "{e:?}");
        let e = lower(&parse_v2("s / s").unwrap(), &l, 0).unwrap_err();
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

    #[test]
    fn the_gallery_reads_one_range_and_says_what_it_is() {
        let r = ranged("low(1..6, 12.5)").unwrap();
        assert_eq!((r.function.as_str(), r.args.clone()), ("low", vec![12.5]));
        assert_eq!(r.dist.written(), "low(1..6, 12.5)");
        let r = ranged("1..6").unwrap();
        assert_eq!(r.function, "flat");
        let r = ranged("[6:4, 7, 10:3]").unwrap();
        assert_eq!((r.function.as_str(), r.args), ("list", vec![4.0, 1.0, 3.0]));
        // out of limits: the same refusal a document gets
        let e = ranged("low(1..6, -2)").unwrap_err();
        assert_eq!(e.code, "dt::range");
        assert_eq!(e.message, "low's ratio must be above 0, not -2");
        assert_eq!(ranged("[1:4, 2:5, 2:1]").unwrap_err().code, "dt::weighting");
        // not one range
        assert!(ranged("6").is_err());
        assert!(ranged("1..2 + 3..4").is_err());
        assert!(ranged("(").is_err());
    }

    #[test]
    fn the_editors_check_names_a_weighting_past_its_limits() {
        let flaw = |s: &str| weighting_flaw_in(s, crate::units::Units::default());
        assert!(flaw("low(1dmm..6dmm, 3)").is_none());
        let e = flaw("bell(1dmm..6dmm, 9)").unwrap();
        assert_eq!(e.code, "dt::range");
        assert_eq!(e.message, "bell's spread must be from 0.05 to 1, not 9");
        assert_eq!(flaw("[1dmm, 1dmm]").unwrap().code, "dt::weighting");
        // a plain number, a plain range, an unreadable value: the reader's to say
        assert!(flaw("6dmm").is_none());
        assert!(flaw("1dmm..6dmm").is_none());
        assert!(flaw("(").is_none());
    }
}
