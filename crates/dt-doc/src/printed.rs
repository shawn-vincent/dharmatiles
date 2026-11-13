//! Printed sizes: what a written length comes to on a build target, and the
//! way back (a printed size typed in, written in the value's own unit).
//!
//! The editor shows each length as written (`7dmm`) and, beside it, what it
//! prints on the current target (`5.08 mm` on OpenLOCK).  This is the one
//! place that works it out: the value is read with the document's own
//! reader ([`crate::expr::parse_units`]), its units lowered with the
//! target's square and layer ([`crate::units::lower`]), and the result
//! evaluated by the one evaluator ([`Expr::eval`], through
//! [`crate::consts::ConstTable::scalar`], so formulas, `$` heights and
//! ranges resolve as a build resolves them).  The page does no unit
//! arithmetic.
//!
//! **Precision.**  A printed size reads to 0.01 mm.  A printed size typed in
//! is written in the value's own unit with the fewest decimals that still
//! print exactly that size to 0.01 mm: 5 mm against `7dmm` on OpenLOCK
//! writes `6.89dmm`, and on DungeonBlocks writes `5dmm`.

use crate::consts::ConstTable;
use crate::expr::{BinOp, Expr, UnitWord, parse_units};
use crate::schema::{Axis, SQUARE_MM};
use crate::units::{Base, Units, Want, expect, lower};

/// What a length prints.
#[derive(Debug, Clone, PartialEq)]
pub struct Printed {
    /// The printed size of each end, mm, as it reads (`"5.08"`): one for a
    /// number, two (low, high) for a range.
    pub ends: Vec<String>,
    /// Whether the printed size can be typed in: the value is a number or a
    /// range of numbers, each with its unit.  A formula only shows it.
    pub editable: bool,
}

/// Printed mm in one of `unit`, on this target; `None` for a unit that is
/// not a length.
fn per_unit_mm(unit: UnitWord, units: Units) -> Option<f64> {
    match unit {
        UnitWord::Dmm => Some(units.square_mm / SQUARE_MM),
        UnitWord::Mm => Some(1.0),
        UnitWord::In => Some(25.4),
        UnitWord::Sq => Some(units.square_mm),
        UnitWord::Layer => Some(units.layer_mm),
        _ => None,
    }
}

/// `n unit`, a number written with a length unit.
fn literal(e: &Expr) -> Option<(f64, UnitWord)> {
    match e {
        Expr::Bin {
            op: BinOp::Mul,
            lhs,
            rhs,
        } => match (&**lhs, &**rhs) {
            (
                Expr::Num {
                    value,
                    percent: false,
                },
                Expr::Unit(u),
            ) => Some((*value, *u)),
            _ => None,
        },
        _ => None,
    }
}

/// The number or the two ends of a range, each `n unit`; `None` for
/// anything else (a formula, a weighting, a list, a name).
fn literal_ends(e: &Expr) -> Option<Vec<(f64, UnitWord)>> {
    match e {
        Expr::Range { lo, hi } => Some(vec![literal(lo)?, literal(hi)?]),
        e => literal(e).map(|l| vec![l]),
    }
}

/// Whether the printed size would read as the written number: every end is
/// a number in a unit that prints as itself (`mm`; `dmm` where the square is
/// the reference's), or a bare zero.  Formulas are never the same.
fn same_as_written(e: &Expr, units: Units) -> bool {
    match e {
        Expr::Num {
            value,
            percent: false,
        } => *value == 0.0,
        Expr::Neg(a) => same_as_written(a, units),
        Expr::Range { lo, hi } => same_as_written(lo, units) && same_as_written(hi, units),
        Expr::Weighted { range, .. } => same_as_written(range, units),
        Expr::List { items } => items.iter().all(|(v, _)| same_as_written(v, units)),
        e => literal(e)
            .and_then(|(_, u)| per_unit_mm(u, units))
            .is_some_and(|per| (per - 1.0).abs() < 1e-12),
    }
}

/// A printed size to 0.01 mm, as it reads: no trailing zeros.
fn mm_text(mm: f64) -> String {
    let t = format!("{:.2}", (mm * 100.0).round() / 100.0);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    // no negative zero
    if t == "-0" {
        "0".to_owned()
    } else {
        t.to_owned()
    }
}

/// What `typed` prints on the target, or `None` where it is not shown: not a
/// length, not readable, names this document does not give, or printing as
/// the number written (`mm`; `dmm` on DungeonBlocks).
///
/// `consts` are the `$` heights and `let`s the value may read (the
/// document's, with the library's); without them a value that names one has
/// no printed size.
#[must_use]
pub fn printed_size(
    axis: Axis,
    typed: &str,
    units: Units,
    consts: Option<&ConstTable>,
) -> Option<Printed> {
    if !matches!(axis, Axis::Dmm | Axis::Height) {
        return None;
    }
    let x = parse_units(typed.trim()).ok()?;
    let names = |_: &str| None;
    expect(&x, Want::Length, &names, false).ok()?;
    if same_as_written(&x, units) {
        return None;
    }
    let lowered = lower(&x, Base::Dmm, units, None).ok()?;
    let empty = ConstTable::default();
    let table = consts.unwrap_or(&empty);
    let at = |u: f64| table.scalar(&lowered, u, &|_| None).ok();
    let (a, b) = (at(0.0)?, at(1.0)?);
    let to_mm = units.square_mm / SQUARE_MM;
    let (lo, hi) = (a.min(b) * to_mm, a.max(b) * to_mm);
    let ends = if lowered.range_count() == 0 || mm_text(lo) == mm_text(hi) {
        vec![mm_text(lo)]
    } else {
        vec![mm_text(lo), mm_text(hi)]
    };
    Some(Printed {
        ends,
        editable: !typed.trim().starts_with('[') && literal_ends(&x).is_some(),
    })
}

/// Why a printed size could not be written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetError(pub String);

/// `typed` with its end number `end` (0 for a number or a range's low end,
/// 1 for the high end) set so that it prints `mm`, written in the end's own
/// unit with the fewest decimals that print exactly `mm` to 0.01 mm.  The
/// unit is never changed, and the other end and the text round it are kept.
///
/// # Errors
/// The value is not a number or a range of numbers with their units (a
/// formula is edited as written), `end` is not one of its ends, or `mm` is
/// not a finite number.
pub fn set_printed(typed: &str, end: usize, mm: f64, units: Units) -> Result<String, SetError> {
    let refuse = |why: &str| Err(SetError(why.to_owned()));
    if !mm.is_finite() {
        return refuse("a printed size is a number of mm");
    }
    let x = parse_units(typed.trim()).map_err(|e| SetError(e.message))?;
    let ends = match literal_ends(&x) {
        Some(e) if !typed.trim().starts_with('[') => e,
        _ => {
            return refuse(
                "this value is a formula, or has odds written on it: edit it as written",
            );
        }
    };
    let Some(&(_, unit)) = ends.get(end) else {
        return refuse("this value has no such end");
    };
    let per = per_unit_mm(unit, units).filter(|p| *p > 0.0);
    let Some(per) = per else {
        return refuse("this unit is not a length");
    };
    let want = mm_text(mm);
    // the fewest decimals that print exactly what was asked
    let number = (0..=6usize)
        .map(|d| {
            let n = format!("{:.d$}", mm / per);
            let n = n.trim_end_matches('0').trim_end_matches('.').to_owned();
            if n == "-0" { "0".to_owned() } else { n }
        })
        .find(|n| n.parse::<f64>().is_ok_and(|v| mm_text(v * per) == want))
        .unwrap_or_else(|| format!("{}", mm / per));
    let written = format!("{number}{}", unit.word());
    if ends.len() == 1 {
        return Ok(written);
    }
    // a range: the other end and the spacing are kept
    let Some(at) = typed.find("..") else {
        return refuse("this range has no `..`");
    };
    let (lo, hi) = (&typed[..at], &typed[at + 2..]);
    let swap = |part: &str| {
        let core = part.trim();
        let lead = &part[..part.len() - part.trim_start().len()];
        let trail = &part[lead.len() + core.len()..];
        format!("{lead}{written}{trail}")
    };
    Ok(if end == 0 {
        format!("{}..{hi}", swap(lo))
    } else {
        format!("{lo}..{}", swap(hi))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: Units = Units {
        square_mm: 35.0,
        layer_mm: 0.2,
    };
    const OL: Units = Units {
        square_mm: 25.4,
        layer_mm: 0.2,
    };

    fn shown(typed: &str, units: Units) -> Option<Vec<String>> {
        printed_size(Axis::Dmm, typed, units, None).map(|p| p.ends)
    }

    #[test]
    fn dmm_prints_as_itself_on_dungeonblocks_and_mm_always() {
        assert_eq!(shown("7dmm", DB), None);
        assert_eq!(shown("3.5dmm..6.5dmm", DB), None);
        assert_eq!(shown("7mm", OL), None);
        assert_eq!(shown("0", OL), None);
    }

    #[test]
    fn dmm_scales_on_openlock_and_sq_and_layer_always_show() {
        assert_eq!(shown("7dmm", OL), Some(vec!["5.08".to_owned()]));
        assert_eq!(
            shown("3.5dmm .. 6.5dmm", OL),
            Some(vec!["2.54".to_owned(), "4.72".to_owned()])
        );
        assert_eq!(shown("1sq", DB), Some(vec!["35".to_owned()]));
        assert_eq!(shown("1sq", OL), Some(vec!["25.4".to_owned()]));
        assert_eq!(shown("2layer", DB), Some(vec!["0.4".to_owned()]));
        assert_eq!(shown("1in", DB), Some(vec!["25.4".to_owned()]));
    }

    #[test]
    fn a_formula_resolves_its_heights_and_is_not_editable() {
        let mut k = crate::Known::new();
        k.add_heights(
            "heights.dtile",
            include_str!("../../../library/parts/heights.dtile"),
        )
        .unwrap();
        let t = ConstTable::root(k.consts());
        let p = printed_size(Axis::Height, "$groundHeight + 4dmm", OL, Some(&t)).unwrap();
        assert_eq!(p.ends, vec!["6.53".to_owned()]); // 9 dmm
        assert!(!p.editable);
        // names the document does not give have no printed size
        assert_eq!(
            printed_size(Axis::Height, "$groundHeight + 4dmm", OL, None),
            None
        );
        assert_eq!(printed_size(Axis::Height, "$x * 1mm", OL, Some(&t)), None);
    }

    #[test]
    fn a_printed_size_is_written_in_the_values_own_unit() {
        assert_eq!(set_printed("7dmm", 0, 5.0, OL).unwrap(), "6.89dmm");
        assert_eq!(set_printed("7dmm", 0, 5.0, DB).unwrap(), "5dmm");
        assert_eq!(set_printed("2sq", 0, 50.0, DB).unwrap(), "1.4286sq");
        assert_eq!(set_printed("7mm", 0, 3.25, OL).unwrap(), "3.25mm");
        // a range: either end, the other kept as written
        assert_eq!(
            set_printed("3.5dmm .. 6.5dmm", 1, 5.0, OL).unwrap(),
            "3.5dmm .. 6.89dmm"
        );
        assert_eq!(
            set_printed("3.5dmm..6.5dmm", 0, 1.0, OL).unwrap(),
            "1.38dmm..6.5dmm"
        );
        // a formula is not set this way
        assert!(set_printed("$groundHeight + 4dmm", 0, 5.0, OL).is_err());
        assert!(set_printed("low(3dmm .. 6dmm, 2)", 0, 5.0, OL).is_err());
        assert!(set_printed("7dmm", 1, 5.0, OL).is_err());
    }
}
