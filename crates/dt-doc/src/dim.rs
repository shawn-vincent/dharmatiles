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
}

impl Dim {
    /// How the dimension reads in a message.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Dim::Scalar => "a plain number",
            Dim::Mm => "a height or length in mm",
            Dim::Squares => "squares (x or y)",
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
        Expr::Neg(a) => dim_of(a, name),
        Expr::Range { lo, hi } => unify(dim_of(lo, name)?, dim_of(hi, name)?, "a range"),
        Expr::Bin { op, lhs, rhs } => {
            let (a, b) = (dim_of(lhs, name)?, dim_of(rhs, name)?);
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

fn unify(a: Dim, b: Dim, what: &str) -> Result<Dim, DimError> {
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
}
