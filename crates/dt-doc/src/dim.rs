//! What a height expression measures, so a mistake like `groundHeight + x`
//! is refused where it is written.
//!
//! Numbers in mm, constants included, take their meaning where they are
//! used: in a top a height above zero, in `within` a length.  `x` and `y`
//! are squares; a bare number adapts to a length but not to squares, and
//! squares times a number are still squares (an angle `720 * x` for `cos`).

use crate::expr::{BinOp, Expr, FUNCTIONS};

/// What a value measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    /// A plain number (it adapts to a length when added to one).
    Scalar,
    /// A length or a height, mm.
    Mm,
    /// Squares: `x` and `y`, and angles made from them.
    Squares,
    /// A region (format 1): `$tile`, `$region`, `parent(…)`.  It goes to
    /// `parent()` only; as a number it is an error.
    Region,
}

impl Dim {
    /// How the dimension reads in a message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Dim::Scalar => "a plain number",
            Dim::Mm => "a height or length in mm",
            Dim::Squares => "squares (x or y)",
            Dim::Region => "a region",
        }
    }
}

/// Why an expression does not measure what it should.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimError {
    /// What is wrong.
    pub message: String,
    /// What to write instead, if there is a way.
    pub help: Option<String>,
}

fn err<T>(message: impl Into<String>, help: Option<&str>) -> Result<T, DimError> {
    Err(DimError {
        message: message.into(),
        help: help.map(str::to_owned),
    })
}

/// The dimension of `e`.  `name` gives the dimension of a name (`None`:
/// unknown names are the caller's to refuse before).
///
/// # Errors
/// The first mismatch: squares added to a length, two lengths multiplied,
/// a percent, or a function called with the wrong values.
pub fn dim_of(e: &Expr, name: &dyn Fn(&str) -> Option<Dim>) -> Result<Dim, DimError> {
    match e {
        Expr::Num { percent: true, .. } => err(
            "a percent is a share, not a height",
            Some("write the number of mm"),
        ),
        Expr::Num { .. } => Ok(Dim::Scalar),
        Expr::Name(n) => name(n).map_or_else(|| err(format!("unknown name `{n}`"), None), Ok),
        Expr::Neg(a) => not_region(dim_of(a, name)?, "a negation"),
        Expr::Range { lo, hi } => unify(dim_of(lo, name)?, dim_of(hi, name)?, "a range"),
        Expr::Weighted { range, .. } => dim_of(range, name),
        Expr::List { items } => {
            let mut dim = None;
            for (v, _) in items {
                let d = dim_of(v, name)?;
                dim = Some(match dim {
                    Some(prev) => unify(prev, d, "a list")?,
                    None => d,
                });
            }
            Ok(dim.unwrap_or(Dim::Scalar))
        }
        Expr::Bin { op, lhs, rhs } => {
            let (a, b) = (dim_of(lhs, name)?, dim_of(rhs, name)?);
            not_region(a, "arithmetic")?;
            not_region(b, "arithmetic")?;
            match op {
                BinOp::Add | BinOp::Sub => unify(a, b, "a sum"),
                BinOp::Mul => match (a, b) {
                    (Dim::Scalar, d) | (d, Dim::Scalar) => Ok(d),
                    _ => err(
                        format!("{} times {} measures nothing", a.describe(), b.describe()),
                        Some("multiply by a plain number"),
                    ),
                },
                BinOp::Div => match (a, b) {
                    (d, Dim::Scalar) => Ok(d),
                    (x, y) if x == y => Ok(Dim::Scalar),
                    _ => err(
                        format!(
                            "{} divided by {} measures nothing",
                            a.describe(),
                            b.describe()
                        ),
                        None,
                    ),
                },
            }
        }
        Expr::Call { name: f, args } if f == "parent" => {
            if args.len() != 1 {
                return err(
                    format!("`parent` takes 1 value, not {}", args.len()),
                    Some("`parent($region)` is the region containing this one"),
                );
            }
            if matches!(&args[0], Expr::Name(n) if n == "$tile") {
                return err(
                    "the tile has no parent",
                    Some(
                        "`$tile` is the root; `parent($region)` is the region containing a thing's own",
                    ),
                );
            }
            match dim_of(&args[0], name)? {
                Dim::Region => Ok(Dim::Region),
                d => err(
                    format!("`parent` takes a region, not {}", d.describe()),
                    Some("write `parent($region)`: the region containing this thing's own"),
                ),
            }
        }
        Expr::Call { name: f, args } => {
            let Some((_, n)) = FUNCTIONS.iter().find(|(k, _)| k == f) else {
                let names: Vec<&str> = FUNCTIONS.iter().map(|(k, _)| *k).collect();
                return err(
                    format!("no function is called `{f}`"),
                    Some(&format!("the functions are {}", names.join(", "))),
                );
            };
            if args.len() != *n {
                return err(
                    format!("`{f}` takes {n} value(s), not {}", args.len()),
                    None,
                );
            }
            let ds = args
                .iter()
                .map(|a| dim_of(a, name))
                .collect::<Result<Vec<_>, _>>()?;
            for d in &ds {
                not_region(*d, &format!("`{f}`"))?;
            }
            match f.as_str() {
                "sin" | "cos" => match ds[0] {
                    Dim::Mm => err(
                        format!("`{f}` takes degrees, not a height"),
                        Some("write an angle such as `720 * x`"),
                    ),
                    _ => Ok(Dim::Scalar),
                },
                "smooth" => match ds[0] {
                    Dim::Scalar => Ok(Dim::Scalar),
                    d => err(
                        format!("`smooth` takes a plain number, not {}", d.describe()),
                        None,
                    ),
                },
                _ => ds
                    .iter()
                    .try_fold(Dim::Scalar, |acc, d| unify(acc, *d, "its values")),
            }
        }
    }
}

/// A region is not a number: it goes to `parent()` and nothing else yet.
fn not_region(d: Dim, what: &str) -> Result<Dim, DimError> {
    if d == Dim::Region {
        return err(
            format!("a region is not a number, and {what} needs one"),
            Some("a region goes to `parent(…)`; it has no size to add or multiply"),
        );
    }
    Ok(d)
}

fn unify(a: Dim, b: Dim, what: &str) -> Result<Dim, DimError> {
    not_region(a, what)?;
    not_region(b, what)?;
    match (a, b) {
        (x, y) if x == y => Ok(x),
        (Dim::Scalar, Dim::Mm) | (Dim::Mm, Dim::Scalar) => Ok(Dim::Mm),
        _ => err(
            format!("{what} mixes {} and {}", a.describe(), b.describe()),
            Some(
                "x and y are squares: scale them by a plain number inside sin or cos, or use them alone",
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parse_v2;

    fn names(n: &str) -> Option<Dim> {
        match n {
            "x" | "y" => Some(Dim::Squares),
            "bottom" | "groundHeight" => Some(Dim::Mm),
            "count" => Some(Dim::Scalar),
            "$region" | "$tile" => Some(Dim::Region),
            _ => None,
        }
    }

    fn dim(s: &str) -> Result<Dim, DimError> {
        dim_of(&parse_v2(s).unwrap(), &names)
    }

    #[test]
    fn heights_and_angles() {
        assert_eq!(dim("groundHeight + [1 .. 2]"), Ok(Dim::Mm));
        assert_eq!(dim("groundHeight + 0.6 * cos(720 * (x + y))"), Ok(Dim::Mm));
        assert_eq!(dim("bottom + 8"), Ok(Dim::Mm));
        assert_eq!(dim("3 * count"), Ok(Dim::Scalar));
        assert_eq!(dim("x"), Ok(Dim::Squares));
        assert_eq!(dim("groundHeight / groundHeight"), Ok(Dim::Scalar));
    }

    #[test]
    fn mistakes() {
        assert!(
            dim("groundHeight + x")
                .unwrap_err()
                .message
                .contains("mixes")
        );
        assert!(dim("x + 1").is_err());
        assert!(dim("groundHeight * groundHeight").is_err());
        assert!(dim("cos(groundHeight)").is_err());
        assert!(dim("20%").is_err());
        assert!(dim("clamp(x, 1)").is_err());
    }

    #[test]
    fn a_region_is_its_own_kind() {
        assert_eq!(dim("parent($region)"), Ok(Dim::Region));
        assert_eq!(dim("parent(parent($region))"), Ok(Dim::Region));
        assert_eq!(dim("$tile"), Ok(Dim::Region));
        let e = dim("parent($tile)").unwrap_err();
        assert!(e.message.contains("no parent"), "{e:?}");
        for bad in [
            "$region + 1",
            "2 * $region",
            "-$region",
            "min($region, 1)",
            "[$region .. 2]",
        ] {
            let e = dim(bad).unwrap_err();
            assert!(e.message.contains("not a number"), "{bad}: {e:?}");
        }
        assert!(
            dim("parent(1)")
                .unwrap_err()
                .message
                .contains("takes a region")
        );
    }
}
