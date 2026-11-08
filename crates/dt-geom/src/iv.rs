//! Intervals: a closed range of numbers, `lo <= hi`, and the arithmetic a
//! range in a height needs (`[0 .. 1] - [0 .. 1]` is `[-1 .. 1]`: a range is
//! a set, so the result holds every value some choice of the operands
//! could make). The band solver of dt-kit is built on them.

use std::ops::{Add, Mul, Neg, Sub};

/// A closed interval of numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iv {
    /// The low end.
    pub lo: f64,
    /// The high end, `>= lo`.
    pub hi: f64,
}

impl Iv {
    /// The interval from `a` to `b`, whichever is lower first.
    #[must_use]
    pub fn new(a: f64, b: f64) -> Self {
        if a <= b {
            Self { lo: a, hi: b }
        } else {
            Self { lo: b, hi: a }
        }
    }

    /// The interval holding only `v`.
    #[must_use]
    pub const fn point(v: f64) -> Self {
        Self { lo: v, hi: v }
    }

    /// `hi - lo`.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }

    /// The middle.
    #[must_use]
    pub fn mid(&self) -> f64 {
        0.5 * (self.lo + self.hi)
    }

    /// Whether `v` lies in the interval.
    #[must_use]
    pub fn contains(&self, v: f64) -> bool {
        self.lo <= v && v <= self.hi
    }

    /// Whether the two intervals share a value.
    #[must_use]
    pub fn overlaps(&self, other: Self) -> bool {
        self.lo <= other.hi && other.lo <= self.hi
    }

    /// The distance from the number `v` to the interval (0 inside it).
    #[must_use]
    pub fn dist(&self, v: f64) -> f64 {
        (self.lo - v).max(v - self.hi).max(0.0)
    }

    /// The least distance between the two intervals (0 when they overlap).
    #[must_use]
    pub fn gap(&self, other: Self) -> f64 {
        (self.lo - other.hi).max(other.lo - self.hi).max(0.0)
    }

    /// The smallest interval holding both.
    #[must_use]
    pub fn hull(&self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    /// The values in both, `None` when there are none.
    #[must_use]
    pub fn intersect(&self, other: Self) -> Option<Self> {
        let (lo, hi) = (self.lo.max(other.lo), self.hi.min(other.hi));
        (lo <= hi).then_some(Self { lo, hi })
    }

    /// Every value times `k`.
    #[must_use]
    pub fn scale(&self, k: f64) -> Self {
        Self::new(self.lo * k, self.hi * k)
    }
}

impl Add for Iv {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self {
            lo: self.lo + o.lo,
            hi: self.hi + o.hi,
        }
    }
}

impl Sub for Iv {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self {
            lo: self.lo - o.hi,
            hi: self.hi - o.lo,
        }
    }
}

impl Neg for Iv {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
}

impl Mul for Iv {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let p = [
            self.lo * o.lo,
            self.lo * o.hi,
            self.hi * o.lo,
            self.hi * o.hi,
        ];
        Self {
            lo: p.iter().copied().fold(f64::INFINITY, f64::min),
            hi: p.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_a_set() {
        let r = Iv::new(0.0, 1.0);
        assert_eq!(r - r, Iv::new(-1.0, 1.0));
        assert_eq!(r + r, Iv::new(0.0, 2.0));
        assert_eq!(-r, Iv::new(-1.0, 0.0));
        assert_eq!(Iv::new(-1.0, 2.0) * Iv::new(-3.0, 1.0), Iv::new(-6.0, 3.0));
        assert_eq!(r.scale(-2.0), Iv::new(-2.0, 0.0));
        assert_eq!(Iv::new(3.0, 1.0), Iv::new(1.0, 3.0));
    }

    #[test]
    fn gaps_and_overlaps() {
        let a = Iv::new(5.0, 6.0);
        let b = Iv::new(3.8, 4.2);
        assert!(!a.overlaps(b) && b.overlaps(Iv::new(4.0, 9.0)));
        assert!((a.gap(b) - 0.8).abs() < 1e-12);
        assert_eq!(a.gap(Iv::new(5.5, 8.0)), 0.0);
        assert!((b.dist(6.0) - 1.8).abs() < 1e-12);
        assert_eq!(a.hull(b), Iv::new(3.8, 6.0));
        assert_eq!(a.intersect(b), None);
        assert_eq!(a.intersect(Iv::new(5.5, 9.0)), Some(Iv::new(5.5, 6.0)));
    }
}
