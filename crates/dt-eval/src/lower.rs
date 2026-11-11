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
use dt_geom::{Dist, Draw, Op, Weighting};

/// The tile's `let`s as an element control sees them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lets {
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
    pub(crate) fn from_consts(consts: &ConstTable) -> Self {
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
    pub(crate) fn mistake(message: impl Into<String>) -> Self {
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

/// The heights a tree may stand, from its foot to the top of its crown,
/// mm at the reference square.
pub(crate) const TREE_HEIGHT_MM: (f64, f64) = (20.0, 120.0);

/// A tree's `top=` as the tree takes it: `("height", …)` when it is written
/// over the tree's foot (`bottom + 40`: the tree is 40 mm tall wherever it
/// stands, drawn as a `size` was), or `("top", …)` for a height above zero
/// (`groundHeight + 40`: the tree's height is that less the ground at its
/// foot, found where it stands).
pub(crate) fn tree_top(e: &Expr, lets: &Lets) -> Result<(&'static str, Lowered), Refused> {
    let help = "a tree's top is `bottom + 40` (40 mm over the ground at its foot) or a height above zero such as `groundHeight + 40`";
    let Some(rest) = over_bottom(e).map_err(|m| Refused::mistake(m).with_help(help))? else {
        let num = Num {
            expr: e.clone(),
            unit: None,
        };
        return Ok(("top", number(&num, lets)?));
    };
    let num = Num {
        expr: rest,
        unit: None,
    };
    let low = number(&num, lets)?;
    let (lo, hi) = match &low.value {
        dt_elements::Value::Number(v) => (*v, *v),
        dt_elements::Value::Range(a, b) => (*a, *b),
        dt_elements::Value::Dist(d) => (d.lo, d.hi),
        dt_elements::Value::Draw(d) => d.bounds(),
        dt_elements::Value::Word(_) => (f64::NAN, f64::NAN),
    };
    let (min, max) = TREE_HEIGHT_MM;
    if !(lo >= min && hi <= max) {
        let fmt = |v: f64| format!("{}", (v * 10.0).round() / 10.0);
        let made = if (hi - lo).abs() < 1e-9 {
            format!("{} mm", fmt(lo))
        } else {
            format!("{} to {} mm", fmt(lo), fmt(hi))
        };
        return Err(Refused::mistake(format!(
            "a tree is {min} mm to {max} mm tall over its foot, and this top makes it {made}"
        ))
        .with_help("write the height over the ground at its foot: `bottom + 40`"));
    }
    Ok(("height", low))
}

/// `e` less one `bottom` added to it (`bottom + 30 + [1 .. 2]` is
/// `30 + [1 .. 2]`); `None` when it names no `bottom`.
fn over_bottom(e: &Expr) -> Result<Option<Expr>, String> {
    use dt_doc::expr::BinOp::{Add, Sub};
    let is_bottom = |n: &str| n == "bottom" || n == "$bottom";
    let has = |x: &Expr| x.names().iter().any(|n| is_bottom(n));
    if !has(e) {
        return Ok(None);
    }
    let alone = || {
        "`bottom` in a tree's top is added once: the ground at its foot plus the tree's height"
            .to_owned()
    };
    Ok(Some(match e {
        Expr::Name(n) if is_bottom(n) => Expr::num(0.0),
        Expr::Bin { op, lhs, rhs } if matches!(op, Add | Sub) => match (has(lhs), has(rhs)) {
            (true, false) => {
                let l = over_bottom(lhs)?.ok_or_else(alone)?;
                if matches!(lhs.as_ref(), Expr::Name(n) if is_bottom(n)) {
                    if *op == Add {
                        rhs.as_ref().clone()
                    } else {
                        Expr::Neg(rhs.clone())
                    }
                } else {
                    Expr::Bin {
                        op: *op,
                        lhs: Box::new(l),
                        rhs: rhs.clone(),
                    }
                }
            }
            (false, true) if *op == Add => {
                let r = over_bottom(rhs)?.ok_or_else(alone)?;
                if matches!(rhs.as_ref(), Expr::Name(n) if is_bottom(n)) {
                    lhs.as_ref().clone()
                } else {
                    Expr::Bin {
                        op: *op,
                        lhs: lhs.clone(),
                        rhs: Box::new(r),
                    }
                }
            }
            _ => return Err(alone()),
        },
        _ => return Err(alone()),
    }))
}

/// What a lowered expression is: a number, one range with its odds (which
/// shifts and scales without losing them), or several rolled and summed.
#[derive(Clone, Debug)]
enum Part {
    Num(f64),
    Dist(Dist),
    Draw(Draw),
}

impl Part {
    fn into_draw(self) -> Draw {
        match self {
            Self::Num(x) => Draw::Num(x),
            Self::Dist(d) => Draw::Range(d),
            Self::Draw(d) => d,
        }
    }
}

fn number(n: &Num, lets: &Lets) -> Result<Lowered, Refused> {
    let env = |name: &str| lets.num(name);
    let named_draw = n.expr.names().iter().any(|x| lets.draw(x).is_some());
    let weighted = {
        let mut any = false;
        visit(&n.expr, &mut |e| {
            any |= matches!(e, Expr::Weighted { .. } | Expr::List { .. });
        });
        any
    };
    // The plain cases are as they were: one range is its two ends, one number
    // is itself.  Two ranges or more are rolled each on its own.
    if !named_draw && !weighted && n.expr.range_count() <= 1 {
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
        Part::Draw(d) => Ok(Lowered::plain(dt_elements::Value::Draw(
            if n.unit != Some(dt_doc::expr::Unit::H) {
                d
            } else {
                Draw::Bin(Op::Mul, Box::new(d), Box::new(Draw::Num(k)))
            },
        ))),
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
        Expr::Num { .. } | Expr::Name(_) | Expr::Unit(_) => {}
        Expr::Neg(a) => visit(a, f),
        Expr::Bin { lhs, rhs, .. } | Expr::Range { lo: lhs, hi: rhs } => {
            visit(lhs, f);
            visit(rhs, f);
        }
        Expr::Weighted { range, .. } => visit(range, f),
        Expr::List { items } => items.iter().for_each(|(v, _)| visit(v, f)),
        Expr::Call { args, .. } => args.iter().for_each(|a| visit(a, f)),
    }
}

/// One coordinate of an `at=` point, in squares: a number, or a range with
/// its odds, drawn once from the entry's seed.  A number is a range of no
/// width, as a control's number is, so it takes its draw as before.
pub(crate) fn coordinate(e: &Expr) -> Result<Draw, Refused> {
    let lets = Lets::default();
    if !lets.draws_in(e) {
        let v = e.sample(&|_| None, 0.0)?;
        return Ok(Draw::flat(v, v));
    }
    Ok(lower(e, &lets, 0)?.into_draw())
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
            }
        }
        Expr::Range { .. } => {
            let (lo, hi) = ends(e, lets)?;
            Part::Dist(Dist::flat(lo, hi))
        }
        Expr::Weighted {
            name,
            args,
            range,
            parts,
        } => {
            let (lo, hi) = ends(range, lets)?;
            let w = if name == "mix" {
                let mut made = Vec::with_capacity(parts.len());
                for ((n, a), weight) in parts.iter().zip(args) {
                    made.push((*weight, Weighting::from_call(n, a).map_err(refused)?));
                }
                Weighting::mix(made)
            } else {
                Weighting::from_call(name, args)
            }
            .map_err(refused)?;
            Part::Dist(Dist::new(lo, hi, w))
        }
        Expr::List { items } => {
            let env = |name: &str| lets.num(name);
            let mut values = Vec::with_capacity(items.len());
            for (v, w) in items {
                if lets.draws_in(v) {
                    return Err(Refused::mistake(format!(
                        "`{v}` is a range inside a list: the values of a list are numbers"
                    ))
                    .with_help("write numbers: [1:4, 2:5, 3:1]"));
                }
                values.push((v.sample(&env, 0.0)?, *w));
            }
            Part::Dist(Dist::list(&values).map_err(refused)?)
        }
        Expr::Call { name, .. } => {
            return Err(Refused::mistake(format!(
                "`{name}` is given a range: a function takes numbers"
            )));
        }
        Expr::Num { .. } | Expr::Unit(_) => Part::Num(e.sample(&env, 0.0)?),
    })
}

/// A weighting that cannot be made, as the diagnostic it is.
fn refused(w: dt_geom::WeightingError) -> Refused {
    Refused {
        code: w.code,
        message: w.message,
        help: Some(w.help),
        hard: true,
    }
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
    let e = dt_doc::expr::parse(src.trim()).map_err(|e| {
        not_a_range(
            format!("`{}` does not read as a value: {}", src.trim(), e.message),
            "write a range such as 1..6, or a weighted one such as low(1..6, 4)",
        )
    })?;
    let lets = Lets::default();
    let part = lower(&e, &lets, 0).map_err(|r| RangeRefusal {
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
    let (function, args) = match &e {
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
/// other `dt::weighting` refusal, as a build would say.  `None` for a
/// value that does not parse (the reader's check says so) or that names
/// things only the document knows.
#[must_use]
pub fn weighting_flaw(src: &str) -> Option<RangeRefusal> {
    let e = dt_doc::expr::parse(src.trim()).ok()?;
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
        assert!(weighting_flaw("low(1..6, 3)").is_none());
        let e = weighting_flaw("bell(1..6, 9)").unwrap();
        assert_eq!(e.code, "dt::range");
        assert_eq!(e.message, "bell's spread must be from 0.05 to 1, not 9");
        assert_eq!(weighting_flaw("[1, 1]").unwrap().code, "dt::weighting");
        // a plain number, a plain range, an unreadable value: the reader's to say
        assert!(weighting_flaw("6").is_none());
        assert!(weighting_flaw("1..6").is_none());
        assert!(weighting_flaw("(").is_none());
    }
}
