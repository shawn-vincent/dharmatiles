//! A written value as a built element takes it.
//!
//! A control value is a number, a word, a range, or a weighted range.  A
//! tile's `let` names resolve here: a `let` that is one number is that
//! number; a `let` that holds a range (weighted or not) holds a
//! distribution, and every use of its name is its own draw, made by the
//! control that names it (each control draws from its own key, so two uses
//! never share a roll).  What a control may be is the schema's and the
//! element's; this only turns what is written into a value.

use dt_doc::expr::Expr;
use dt_doc::model::{Num, Value};
use dt_doc::schema;
// the lowering itself is dt-doc's, so the browser rolls a range as a build does
pub(crate) use dt_doc::lower::{Lets, Refused, coordinate};
use dt_doc::lower::{Part, lower};
use dt_value::{Dist, Draw, Op, Weighting};

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
/// over the tree's foot (`$bottom + 40dmm`: the tree is 40dmm tall wherever it
/// stands, drawn as a `size` was), or `("top", …)` for a height above zero
/// (`$groundHeight + 40dmm`: the tree's height is that less the ground at its
/// foot, found where it stands).
pub(crate) fn tree_top(e: &Expr, lets: &Lets) -> Result<(&'static str, Lowered), Refused> {
    let help = "a tree's top is `$bottom + 40dmm` (40dmm over the ground at its foot) or a height above zero such as `$groundHeight + 40dmm`";
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
        dt_elements::Value::Word(_) | dt_elements::Value::Formula(_) => (f64::NAN, f64::NAN),
    };
    let (min, max) = TREE_HEIGHT_MM;
    if !(lo >= min && hi <= max) {
        let fmt = |v: f64| format!("{}", (v * 10.0).round() / 10.0);
        let made = if (hi - lo).abs() < 1e-9 {
            format!("{}dmm", fmt(lo))
        } else {
            format!("{} to {}dmm", fmt(lo), fmt(hi))
        };
        return Err(Refused::mistake(format!(
            "a tree is {min}dmm to {max}dmm tall over its foot, and this top makes it {made}"
        ))
        .with_help("write the height over the ground at its foot: `$bottom + 40dmm`"));
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

fn number(n: &Num, lets: &Lets) -> Result<Lowered, Refused> {
    let env = |name: &str| lets.num(name);
    let named_draw = n.expr.names().iter().any(|x| lets.draw(x).is_some());
    let weighted = {
        let mut any = false;
        n.expr.walk(&mut |e| {
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
