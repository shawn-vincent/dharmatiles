//! A point written against a region whose box is known only once the tile's
//! regions exist (format 1).
//!
//! `"(50%, 2sq)"` inside a pin of a body that acts in `bank` is a point in
//! `bank`'s frame; `point(parent($region), 25%, 75%)` and
//! `polar(parent($region), 40%, 90deg)` name a frame by a formula.  The
//! decoder runs before the region tree exists, so it checks the point's kinds
//! and keeps it as written (plain names already in place); `dt-eval`'s
//! `check` works it out once the tree is built, and a part's points are
//! worked out per call site the same way.
//!
//! A point against the tile itself is worked out where it is read, as before.

use std::fmt;

use crate::expr::{BinOp, Expr};
use crate::units::{Base, KindError, Units, lower};

/// A point in a frame found later.
#[derive(Debug, Clone, PartialEq)]
pub struct Framed {
    /// The frame as written: `$tile`, `$region`, `parent(…)`.
    pub frame: Expr,
    /// Where in it.
    pub at: FramedAt,
}

/// How the point is written in its frame.
#[derive(Debug, Clone, PartialEq)]
pub enum FramedAt {
    /// Across and down from the frame's north-west corner, as written
    /// (units not yet lowered; a `%` is a share of the frame's width or
    /// height).
    Xy {
        /// Across.
        x: Expr,
        /// Down.
        y: Expr,
    },
    /// `polar`: a share of the way out from the frame's deepest point, and
    /// the angle round it (0 east, turning towards the south), worked out.
    Polar {
        /// Out, as a fraction of the depth (100% is 1).
        d: f64,
        /// Round, degrees.
        a_deg: f64,
    },
}

impl Framed {
    /// The point in squares from the south-west corner, as geometry keeps
    /// it, for a frame whose box is `bbox` (`[west, south, east, north]`,
    /// squares).  `None` for a `polar` point, which needs the frame's depth
    /// (see `dt_eval::coords::RegionField::polar`).
    ///
    /// # Errors
    /// A unit the target does not have.
    pub fn xy_in(&self, bbox: [f64; 4], units: Units) -> Result<Option<(Expr, Expr)>, KindError> {
        let FramedAt::Xy { x, y } = &self.at else {
            return Ok(None);
        };
        let [west, _, east, north] = bbox;
        let (w, h) = (east - west, north - bbox[1]);
        let x = lower(x, Base::Squares, units, Some(w))?;
        let y = lower(y, Base::Squares, units, Some(h))?;
        let across = fold(Expr::Bin {
            op: BinOp::Add,
            lhs: Box::new(Expr::num(west)),
            rhs: Box::new(x),
        });
        Ok(Some((across, north_west(north, y))))
    }
}

impl fmt::Display for Framed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.at {
            FramedAt::Xy { x, y } => write!(f, "point({}, {x}, {y})", self.frame),
            FramedAt::Polar { d, a_deg } => {
                write!(f, "polar({}, {}%, {a_deg}deg)", self.frame, d * 100.0)
            }
        }
    }
}

/// `top - y`: a north-west `y` (running south) in the geometry's own frame,
/// `top` being the frame's north edge.  A number is worked out here, exactly
/// as the mirror image written by hand would be.
#[must_use]
pub fn north_west(top: f64, y: Expr) -> Expr {
    fold(Expr::Bin {
        op: BinOp::Sub,
        lhs: Box::new(Expr::num(top)),
        rhs: Box::new(y),
    })
}

/// A sum or product of plain numbers, worked out: the same operations in the
/// same order, so no float differs.
#[must_use]
pub fn fold(e: Expr) -> Expr {
    match &e {
        Expr::Bin { op, lhs, rhs } => {
            let (a, b) = (fold(*lhs.clone()), fold(*rhs.clone()));
            if let (
                Expr::Num {
                    value: x,
                    percent: false,
                },
                Expr::Num {
                    value: y,
                    percent: false,
                },
            ) = (&a, &b)
            {
                return Expr::num(match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                });
            }
            Expr::Bin {
                op: *op,
                lhs: Box::new(a),
                rhs: Box::new(b),
            }
        }
        Expr::Neg(a) => match fold(*a.clone()) {
            Expr::Num {
                value,
                percent: false,
            } => Expr::num(-value),
            other => Expr::Neg(Box::new(other)),
        },
        _ => e,
    }
}
