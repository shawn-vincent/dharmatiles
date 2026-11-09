//! Weighted ranges: a range plus the odds of each part of it.
//!
//! A [`Weighting`] is a monotone map from one uniform draw `u` in [0, 1) to a
//! position `t` in [0, 1] along a range; the value is `lo + (hi - lo) * t`.
//! A range draws exactly one `u`, whatever the weighting: nothing here
//! rejects and retries, so the draws after it never move.  [`Weighting::Flat`]
//! is `t = u`, so a flat [`Dist`] evaluates the very expression
//! [`Rng::uniform`] does.
//!
//! Only `+ - * /`, `sqrt` and libm are used (an inverse normal CDF with
//! fixed constants), so the bits are the same on every platform.

// Exact comparisons are meant: a ratio of exactly 1 is the plain range.
#![allow(clippy::float_cmp)]

use crate::seed::Rng;

/// A call that does not make a weighting: what is wrong and what to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightingError {
    /// What is wrong.
    pub message: String,
    /// What to write instead.
    pub help: String,
}

/// The weighting functions: name, the arguments after the range, and the
/// call as written with its defaults.  One list, for the checks, the
/// documentation and `dharmatiles elements`.
pub const WEIGHTINGS: &[(&str, &str, &str)] = &[
    (
        "flat",
        "",
        "a plain range: every value equally likely (same as writing the range alone)",
    ),
    (
        "low",
        "ratio = 4",
        "mostly low: the low end is `ratio` times as likely as the high end; 1 is flat, a fraction leans high",
    ),
    ("high", "ratio = 4", "mostly high: the mirror of low"),
    (
        "bell",
        "spread = 0.2, at = 0.5",
        "mostly near `at`, falling off as a normal curve over `spread` (a share of the width), cut at the range",
    ),
    (
        "peak",
        "spread = 0.5, at = 0.5",
        "a favourite value `at`, falling off in a straight line over `spread`, cut at the range",
    ),
];

/// The names of the weighting functions, for messages: `flat, low, high, bell or peak`.
#[must_use]
pub fn weighting_names() -> String {
    let names: Vec<&str> = WEIGHTINGS.iter().map(|w| w.0).collect();
    match names.split_last() {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The odds along a range, as a quantile map.  See the module docs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Weighting {
    /// `t = u`.
    Flat,
    /// The low end is `ratio` times as likely as the high end, falling off
    /// exponentially: density proportional to `ratio^(-t)`.  A fraction
    /// leans the other way.
    Low(f64),
    /// The mirror of [`Weighting::Low`]: density proportional to `ratio^t`.
    High(f64),
    /// A normal curve centred on `at`, `spread` of the width wide, cut at
    /// the range (never retried).
    Bell {
        /// Standard deviation, a share of the width.
        spread: f64,
        /// Centre, 0 to 1.
        at: f64,
    },
    /// A tent: density falling in a straight line to zero `spread` of the
    /// width either side of `at`, cut at the range (not squeezed into it).
    Peak {
        /// Half width of the tent, a share of the width.
        spread: f64,
        /// Centre, 0 to 1.
        at: f64,
    },
}

/// Least and most `spread` of a bell or a peak.
pub const SPREAD: (f64, f64) = (0.05, 1.0);

fn bad(message: String, help: &str) -> WeightingError {
    WeightingError {
        message,
        help: help.to_owned(),
    }
}

fn ratio(name: &str, args: &[f64], default: f64) -> Result<f64, WeightingError> {
    let r = args.first().copied().unwrap_or(default);
    if r.is_finite() && r > 0.0 && (1.0 / r).is_finite() {
        Ok(r)
    } else {
        Err(bad(
            format!("{name}'s ratio must be above 0, not {r}"),
            "1 is flat, 4 leans low, 0.25 leans high (the same as high(1..6, 4))",
        ))
    }
}

fn centre(name: &str, args: &[f64], spread_default: f64) -> Result<(f64, f64), WeightingError> {
    let spread = args.first().copied().unwrap_or(spread_default);
    if !(spread.is_finite() && (SPREAD.0..=SPREAD.1).contains(&spread)) {
        return Err(bad(
            format!(
                "{name}'s spread must be from {} to {}, not {spread}",
                SPREAD.0, SPREAD.1
            ),
            "spread is a share of the width: 0.2 is narrow, 1 is the whole range",
        ));
    }
    let at = args.get(1).copied().unwrap_or(0.5);
    if !(at.is_finite() && (0.0..=1.0).contains(&at)) {
        return Err(bad(
            format!("{name}'s centre must be from 0 to 1, not {at}"),
            "the centre is a share of the width: 0 is the low end, 1 the high end",
        ));
    }
    Ok((spread, at))
}

impl Weighting {
    /// The weighting a call such as `low(1..6, 3)` names, from its numbers
    /// after the range.
    ///
    /// # Errors
    /// An unknown function, too many numbers, or a number outside its
    /// limits.
    pub fn from_call(name: &str, args: &[f64]) -> Result<Self, WeightingError> {
        let most = match name {
            "flat" => 0,
            "low" | "high" => 1,
            "bell" | "peak" => 2,
            _ => {
                return Err(bad(
                    format!("unknown function `{name}`"),
                    &format!("weightings are {}", weighting_names()),
                ));
            }
        };
        if args.len() > most {
            return Err(bad(
                format!("`{name}` takes the range and {most} number(s) at most"),
                &format!("write {name}(range{})", args_text(name)),
            ));
        }
        Ok(match name {
            "flat" => Self::Flat,
            "low" => Self::Low(ratio(name, args, 4.0)?),
            "high" => Self::High(ratio(name, args, 4.0)?),
            "bell" => {
                let (spread, at) = centre(name, args, 0.2)?;
                Self::Bell { spread, at }
            }
            _ => {
                let (spread, at) = centre(name, args, 0.5)?;
                Self::Peak { spread, at }
            }
        })
    }

    /// The call that writes it: the function and the numbers after the range.
    #[must_use]
    pub fn call(&self) -> (&'static str, Vec<f64>) {
        match *self {
            Self::Flat => ("flat", Vec::new()),
            Self::Low(r) => ("low", vec![r]),
            Self::High(r) => ("high", vec![r]),
            Self::Bell { spread, at } => ("bell", vec![spread, at]),
            Self::Peak { spread, at } => ("peak", vec![spread, at]),
        }
    }

    /// True for the plain range.
    #[must_use]
    pub fn is_flat(&self) -> bool {
        matches!(self, Self::Flat) || matches!(self, Self::Low(r) | Self::High(r) if *r == 1.0)
    }

    /// The same odds read from the other end (a negative factor swaps the
    /// ends of a range).
    #[must_use]
    pub fn mirrored(&self) -> Self {
        match *self {
            Self::Flat => Self::Flat,
            Self::Low(r) => Self::High(r),
            Self::High(r) => Self::Low(r),
            Self::Bell { spread, at } => Self::Bell {
                spread,
                at: 1.0 - at,
            },
            Self::Peak { spread, at } => Self::Peak {
                spread,
                at: 1.0 - at,
            },
        }
    }

    /// Where along the range one uniform draw `u` lands, 0 to 1.  Monotone
    /// in `u`, 0 at 0, and never outside [0, 1]; one draw, no retries.
    #[must_use]
    pub fn unit(&self, u: f64) -> f64 {
        if matches!(self, Self::Flat) {
            return u;
        }
        if u <= 0.0 {
            return 0.0;
        }
        let t = match *self {
            Self::Flat => u,
            Self::Low(r) => low(u, r),
            Self::High(r) => low(u, 1.0 / r),
            Self::Bell { spread, at } => bell(u, spread, at),
            Self::Peak { spread, at } => peak(u, spread, at),
        };
        // a NaN cannot get out
        if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
    }
}

// The arguments of one function as written, for a message.
fn args_text(name: &str) -> String {
    WEIGHTINGS
        .iter()
        .find(|w| w.0 == name)
        .map(|w| w.1)
        .filter(|a| !a.is_empty())
        .map_or_else(String::new, |a| format!(", {a}"))
}

/// Density proportional to `r^(-t)`: `t = -ln(1 - u (1 - 1/r)) / ln r`.
/// `r = 1` is the plain range.  `log1p` keeps it exact as `r` nears 1.
fn low(u: f64, r: f64) -> f64 {
    if r == 1.0 {
        return u;
    }
    let ln_r = libm::log1p(r - 1.0);
    -libm::log1p(-u * ((r - 1.0) / r)) / ln_r
}

/// The standard normal CDF.
fn phi(z: f64) -> f64 {
    0.5 * libm::erfc(-z * core::f64::consts::FRAC_1_SQRT_2)
}

/// A normal truncated to [0, 1], by its inverse CDF.
fn bell(u: f64, s: f64, at: f64) -> f64 {
    let a = phi(-at / s);
    let b = phi((1.0 - at) / s);
    let p = a + u * (b - a);
    if p >= 1.0 {
        return 1.0;
    }
    if p <= 0.0 {
        return 0.0;
    }
    at + s * normal_quantile(p)
}

/// Area of the tent `max(0, 1 - |x - c| / s)` from far left to `x`.
fn tent_area(x: f64, c: f64, s: f64) -> f64 {
    if x <= c - s {
        0.0
    } else if x <= c {
        let d = x - (c - s);
        d * d / (2.0 * s)
    } else if x < c + s {
        let d = c + s - x;
        s - d * d / (2.0 * s)
    } else {
        s
    }
}

/// The tent truncated to [0, 1]: inverse of its area, by sqrt.
fn peak(u: f64, s: f64, c: f64) -> f64 {
    let g0 = tent_area(0.0, c, s);
    let g1 = tent_area(1.0, c, s);
    let g = g0 + u * (g1 - g0);
    if g <= 0.5 * s {
        c - s + libm::sqrt(2.0 * s * g)
    } else {
        c + s - libm::sqrt(2.0 * s * (s - g).max(0.0))
    }
}

/// The inverse of the standard normal CDF for p in (0, 1): Wichura's
/// algorithm AS241 (PPND16), accurate to about 1e-16.
#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
fn normal_quantile(p: f64) -> f64 {
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180625 - q * q;
        let num = (((((((2.5090809287301226727e3 * r + 3.3430575583588128105e4) * r
            + 6.7265770927008700853e4)
            * r
            + 4.5921953931549871457e4)
            * r
            + 1.3731693765509461125e4)
            * r
            + 1.9715909503065514427e3)
            * r
            + 1.3314166789178437745e2)
            * r)
            + 3.3871328727963666080;
        let den = (((((((5.2264952788528545610e3 * r + 2.8729085735721942674e4) * r
            + 3.9307895800092710610e4)
            * r
            + 2.1213794301586595867e4)
            * r
            + 5.3941960214247511077e3)
            * r
            + 6.8718700749205790830e2)
            * r
            + 4.2313330701600911252e1)
            * r)
            + 1.0;
        return q * num / den;
    }
    let tail = if q < 0.0 { p } else { 1.0 - p };
    let r = libm::sqrt(-libm::log(tail));
    let v = if r <= 5.0 {
        let r = r - 1.6;
        let num = (((((((7.74545014278341407640e-4 * r + 2.27238449892691845833e-2) * r
            + 2.41780725177450611770e-1)
            * r
            + 1.27045825245236838258)
            * r
            + 3.64784832476320460504)
            * r
            + 5.76949722146069140550)
            * r
            + 4.63033784615654529590)
            * r)
            + 1.42343711074968357734;
        let den = (((((((1.05075007164441684324e-9 * r + 5.47593808499534494600e-4) * r
            + 1.51986665636164571966e-2)
            * r
            + 1.48103976427480074590e-1)
            * r
            + 6.89767334985100004550e-1)
            * r
            + 1.67638483018380384940)
            * r
            + 2.05319162663775882187)
            * r)
            + 1.0;
        num / den
    } else {
        let r = r - 5.0;
        let num = (((((((2.01033439929228813265e-7 * r + 2.71155556874348757815e-5) * r
            + 1.24266094738807843860e-3)
            * r
            + 2.65321895265761230930e-2)
            * r
            + 2.96560571828504891230e-1)
            * r
            + 1.78482653991729133580)
            * r
            + 5.46378491116411436990)
            * r)
            + 6.65790464350110377720;
        let den = (((((((2.04426310338993978564e-15 * r + 1.42151175831644588870e-7) * r
            + 1.84631831751005468180e-5)
            * r
            + 7.86869131145613259100e-4)
            * r
            + 1.48753612908506148525e-2)
            * r
            + 1.36929880922735805310e-1)
            * r
            + 5.99832206555887937690e-1)
            * r)
            + 1.0;
        num / den
    };
    if q < 0.0 { -v } else { v }
}

/// A range with its odds: what one draw of a weighted range is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dist {
    /// The low end.
    pub lo: f64,
    /// The high end.
    pub hi: f64,
    /// The odds along it.
    pub weighting: Weighting,
}

impl Dist {
    /// `lo..hi` with the odds `weighting`.
    #[must_use]
    pub const fn new(lo: f64, hi: f64, weighting: Weighting) -> Self {
        Self { lo, hi, weighting }
    }

    /// The plain range.
    #[must_use]
    pub const fn flat(lo: f64, hi: f64) -> Self {
        Self::new(lo, hi, Weighting::Flat)
    }

    /// The value one uniform draw `u` gives.
    #[must_use]
    pub fn at(&self, u: f64) -> f64 {
        self.lo + (self.hi - self.lo) * self.weighting.unit(u)
    }

    /// One draw from `rng`, whatever the weighting or the width.  A flat
    /// range is bit-identical to [`Rng::uniform`].
    pub fn draw(&self, rng: &mut Rng) -> f64 {
        self.at(rng.random())
    }

    /// True for a range of no width: a number never draws.
    #[must_use]
    pub fn is_point(&self) -> bool {
        self.hi <= self.lo
    }

    /// A whole number from `lo` to `hi`, both included, every whole number
    /// equally likely under a flat range: [0, 1] is cut into `hi - lo + 1`
    /// equal bins.  One rule for flat and weighted: see [`Dist::bin`].
    #[must_use]
    pub fn int(lo: i64, hi: i64, weighting: Weighting, rng: &mut Rng) -> i64 {
        if hi <= lo {
            return lo;
        }
        Self::bin(lo, hi, weighting.unit(rng.random()))
    }

    /// The whole number position `t` (0 to 1) falls in when `lo..=hi` is
    /// cut into equal bins: `lo + floor((hi - lo + 1) t)`, clamped to `hi`.
    #[must_use]
    pub fn bin(lo: i64, hi: i64, t: f64) -> i64 {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let k = libm::floor((hi - lo + 1) as f64 * t) as i64;
        (lo + k).clamp(lo, hi)
    }
}

/// An arithmetic operator between draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `a + b`
    Add,
    /// `a - b`
    Sub,
    /// `a * b`
    Mul,
    /// `a / b`
    Div,
}

/// What a control holds when it draws: an expression over numbers and
/// ranges.  Every range is its own roll, like a die; the arithmetic then
/// runs on the plain numbers.  One range is [`Dist`] and draws exactly as
/// it does.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    /// A number: it never draws.
    Num(f64),
    /// One range with its odds.
    Range(Dist),
    /// `-a`
    Neg(Box<Draw>),
    /// `a op b`
    Bin(Op, Box<Draw>, Box<Draw>),
}

impl From<Dist> for Draw {
    fn from(d: Dist) -> Self {
        Self::Range(d)
    }
}

impl Draw {
    /// The plain range `lo..hi`.
    #[must_use]
    pub const fn flat(lo: f64, hi: f64) -> Self {
        Self::Range(Dist::flat(lo, hi))
    }

    /// The ends the value can come to, by interval arithmetic.
    #[must_use]
    pub fn bounds(&self) -> (f64, f64) {
        match self {
            Self::Num(x) => (*x, *x),
            Self::Range(d) => (d.lo, d.hi),
            Self::Neg(a) => {
                let (lo, hi) = a.bounds();
                (-hi, -lo)
            }
            Self::Bin(op, a, b) => {
                let ((al, ah), (bl, bh)) = (a.bounds(), b.bounds());
                let at = |f: fn(f64, f64) -> f64| {
                    let c = [f(al, bl), f(al, bh), f(ah, bl), f(ah, bh)];
                    (
                        c.iter().copied().fold(f64::INFINITY, f64::min),
                        c.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    )
                };
                match op {
                    Op::Add => (al + bl, ah + bh),
                    Op::Sub => (al - bh, ah - bl),
                    Op::Mul => at(|x, y| x * y),
                    Op::Div => at(|x, y| x / y),
                }
            }
        }
    }

    /// The low end.
    #[must_use]
    pub fn lo(&self) -> f64 {
        self.bounds().0
    }

    /// The high end.
    #[must_use]
    pub fn hi(&self) -> f64 {
        self.bounds().1
    }

    /// True for a value of no width: it never draws.
    #[must_use]
    pub fn is_point(&self) -> bool {
        let (lo, hi) = self.bounds();
        hi <= lo
    }

    /// How many ranges it rolls.
    #[must_use]
    pub fn ranges(&self) -> usize {
        match self {
            Self::Num(_) => 0,
            Self::Range(_) => 1,
            Self::Neg(a) => a.ranges(),
            Self::Bin(_, a, b) => a.ranges() + b.ranges(),
        }
    }

    /// True when no range carries a weighting.
    #[must_use]
    pub fn is_flat(&self) -> bool {
        match self {
            Self::Num(_) => true,
            Self::Range(d) => d.weighting.is_flat(),
            Self::Neg(a) => a.is_flat(),
            Self::Bin(_, a, b) => a.is_flat() && b.is_flat(),
        }
    }

    /// The one range, when the value is only that.
    #[must_use]
    pub fn single(&self) -> Option<Dist> {
        match self {
            Self::Range(d) => Some(*d),
            _ => None,
        }
    }

    fn roll(&self, rng: &mut Rng, base: &Rng, ordinal: &mut u64) -> f64 {
        match self {
            Self::Num(x) => *x,
            Self::Range(d) => {
                let k = *ordinal;
                *ordinal += 1;
                // the first range reads the stream, as a lone range always
                // did; each later one is keyed by its place in the value
                if k == 0 {
                    d.draw(rng)
                } else {
                    d.draw(&mut base.fork(k))
                }
            }
            Self::Neg(a) => -a.roll(rng, base, ordinal),
            Self::Bin(op, a, b) => {
                let x = a.roll(rng, base, ordinal);
                let y = b.roll(rng, base, ordinal);
                match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div => x / y,
                }
            }
        }
    }

    /// One draw: each range rolls on its own, then the arithmetic.  The
    /// stream moves by one draw however many ranges there are, so editing
    /// this value never moves what is drawn after it.
    pub fn draw(&self, rng: &mut Rng) -> f64 {
        let base = rng.clone();
        self.roll(rng, &base, &mut 0)
    }

    /// A whole number, both ends included, for a control that counts: the
    /// ends round, a lone range is [`Dist::int`], a sum falls in the equal bin
    /// its drawn place along its ends does.  `k` scales the value first (a density times the squares).
    pub fn int_by(&self, k: f64, rng: &mut Rng) -> i64 {
        #[allow(clippy::cast_possible_truncation)]
        let r = |v: f64| libm::round(v) as i64;
        let (lo, hi) = self.bounds();
        let (l, h) = (r(lo * k), r(hi * k));
        match self {
            Self::Range(d) => Dist::int(l, h, d.weighting, rng),
            _ if h <= l => {
                // a number never draws; a sum that cannot differ still rolls
                if self.ranges() > 0 {
                    self.draw(rng);
                }
                l
            }
            _ => {
                let (a, b) = (lo * k, hi * k);
                Dist::bin(l, h, (self.draw(rng) * k - a) / (b - a))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Weighting; 9] = [
        Weighting::Flat,
        Weighting::Low(4.0),
        Weighting::High(4.0),
        Weighting::Low(0.25),
        Weighting::Low(1000.0),
        Weighting::Bell {
            spread: 0.2,
            at: 0.5,
        },
        Weighting::Bell {
            spread: 0.05,
            at: 1.0,
        },
        Weighting::Peak {
            spread: 0.5,
            at: 0.5,
        },
        Weighting::Peak {
            spread: 1.0,
            at: 0.0,
        },
    ];

    #[test]
    fn flat_is_the_uniform_draw_to_the_last_bit() {
        let mut a = Rng::new(crate::Seed::new(97).key("flat"));
        let mut b = a.clone();
        let d = Dist::flat(1.0, 6.0);
        for _ in 0..1000 {
            assert_eq!(d.draw(&mut a).to_bits(), b.uniform(1.0, 6.0).to_bits());
        }
        // a ratio of 1 is the plain range too
        for u in [0.0, 0.1, 0.5, 0.999_999] {
            assert_eq!(Weighting::Low(1.0).unit(u).to_bits(), u.to_bits());
            assert_eq!(Weighting::High(1.0).unit(u).to_bits(), u.to_bits());
        }
    }

    #[test]
    fn fixed_draws_to_the_last_bit() {
        // pinned: a change here moves every tile that writes the function
        let at = |w: Weighting, u: f64| w.unit(u).to_bits();
        assert_eq!(at(Weighting::Low(4.0), 0.5), 0x3fd5_b2c3_da19_723b);
        assert_eq!(at(Weighting::High(4.0), 0.5), 0x3fe5_269e_12f3_46e3);
        assert_eq!(
            at(
                Weighting::Bell {
                    spread: 0.2,
                    at: 0.5
                },
                0.25
            ),
            0x3fd7_7dbe_4f48_2ac6
        );
        assert_eq!(
            at(
                Weighting::Peak {
                    spread: 0.5,
                    at: 0.5
                },
                0.25
            ),
            0x3fd6_a09e_667f_3bcd
        );
    }

    #[test]
    fn endpoints_and_no_nan() {
        for w in ALL {
            assert_eq!(w.unit(0.0), 0.0, "{w:?}");
            let top = w.unit(1.0 - f64::EPSILON / 2.0);
            assert!(
                (top - 1.0).abs() < 1e-9 || w.unit(1.0) == 1.0,
                "{w:?} {top}"
            );
            assert!(w.unit(1.0) <= 1.0, "{w:?}");
            for i in 0..=1000 {
                let u = f64::from(i) / 1000.0;
                let t = w.unit(u);
                assert!((0.0..=1.0).contains(&t), "{w:?} u={u} t={t}");
            }
        }
        // the whole width of the range: huge and tiny ratios
        for r in [1e-12, 1e-3, 0.5, 1.0 + 1e-12, 2.0, 1e6, 1e150] {
            for w in [Weighting::Low(r), Weighting::High(r)] {
                for i in 0..=100 {
                    let t = w.unit(f64::from(i) / 100.0);
                    assert!((0.0..=1.0).contains(&t), "{w:?} {t}");
                }
            }
        }
    }

    #[test]
    fn monotone() {
        for w in ALL {
            let mut last = 0.0;
            for i in 0..=20_000 {
                let t = w.unit(f64::from(i) / 20_000.0);
                assert!(t >= last - 1e-12, "{w:?} at {i}: {t} < {last}");
                last = t;
            }
        }
    }

    #[test]
    fn low_has_its_ratio() {
        // the density at t = 0 over the density at t = 1 is the ratio: the
        // quantile's slope there, inverted
        for r in [2.0, 4.0, 11.0] {
            let w = Weighting::Low(r);
            let h = 1e-6;
            let near0 = w.unit(h) / h; // dt/du at 0 = 1/density(0)
            let near1 = (1.0 - w.unit(1.0 - h)) / h;
            assert!((near1 / near0 - r).abs() < 1e-3, "{r}: {}", near1 / near0);
        }
        // a fraction leans the other way: low(0.25) is high(4)
        for u in [0.1, 0.4, 0.9] {
            let (a, b) = (Weighting::Low(0.25).unit(u), Weighting::High(4.0).unit(u));
            assert!((a - b).abs() < 1e-12);
        }
        // mostly=small's median came from u^2; low(11) keeps it near 0.5^2 = 0.25
        assert!((Weighting::Low(11.0).unit(0.5) - 0.25).abs() < 0.01);
    }

    #[test]
    fn bell_and_peak_centre_on_at() {
        let w = Weighting::Bell {
            spread: 0.2,
            at: 0.5,
        };
        assert!((w.unit(0.5) - 0.5).abs() < 1e-12);
        let w = Weighting::Bell {
            spread: 0.1,
            at: 0.3,
        };
        // truncation hardly matters 3 sigma from both ends: the median is the centre
        assert!((w.unit(0.5) - 0.3).abs() < 1e-3);
        let w = Weighting::Peak {
            spread: 0.5,
            at: 0.5,
        };
        assert!((w.unit(0.5) - 0.5).abs() < 1e-12);
        assert!((w.unit(0.125) + w.unit(0.875) - 1.0).abs() < 1e-12);
        // a centre at an end is a ramp: wide spread leaves the near end at full height
        let ramp = Weighting::Peak {
            spread: 1.0,
            at: 1.0,
        };
        // density 2t: the quantile is sqrt(u)
        assert!((ramp.unit(0.25) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn normal_quantile_inverts_the_cdf() {
        for i in 1..1000 {
            let p = f64::from(i) / 1000.0;
            let z = normal_quantile(p);
            assert!((phi(z) - p).abs() < 1e-14, "p={p}: {}", phi(z) - p);
        }
        for p in [1e-300, 1e-20, 1e-8, 1.0 - 1e-12] {
            let z = normal_quantile(p);
            assert!(((phi(z) - p) / p.min(1.0 - p)).abs() < 1e-8, "p={p}");
        }
        assert_eq!(normal_quantile(0.5), 0.0);
    }

    #[test]
    fn calls_are_checked() {
        assert_eq!(Weighting::from_call("low", &[]), Ok(Weighting::Low(4.0)));
        assert_eq!(
            Weighting::from_call("bell", &[0.3]),
            Ok(Weighting::Bell {
                spread: 0.3,
                at: 0.5
            })
        );
        for (name, args) in [
            ("low", vec![-2.0]),
            ("low", vec![0.0]),
            ("low", vec![f64::NAN]),
            ("high", vec![f64::INFINITY]),
            ("bell", vec![0.01]),
            ("bell", vec![2.0]),
            ("peak", vec![0.5, 1.5]),
            ("flat", vec![1.0]),
            ("low", vec![1.0, 2.0]),
            ("bel", vec![]),
        ] {
            assert!(
                Weighting::from_call(name, &args).is_err(),
                "{name} {args:?}"
            );
        }
        let e = Weighting::from_call("low", &[-2.0]).unwrap_err();
        assert_eq!(e.message, "low's ratio must be above 0, not -2");
        assert!(e.help.contains("0.25 leans high"));
        assert!(
            Weighting::from_call("bel", &[])
                .unwrap_err()
                .help
                .contains("flat, low, high, bell or peak")
        );
    }

    #[test]
    fn integer_draws_use_equal_bins() {
        let mut rng = Rng::new(crate::Seed::new(5).key("int"));
        let mut counts = [0u32; 6];
        for _ in 0..6000 {
            let k = Dist::int(1, 6, Weighting::Low(4.0), &mut rng);
            assert!((1..=6).contains(&k));
            counts[usize::try_from(k - 1).unwrap()] += 1;
        }
        assert!(counts[0] > counts[5] * 2, "{counts:?}");
        // flat keeps the modulus
        let mut a = Rng::new(crate::Seed::new(5).key("int"));
        let mut b = a.clone();
        assert_eq!(Dist::int(1, 6, Weighting::Flat, &mut a), b.int(1, 6));
    }

    fn sum(a: Dist, b: Dist) -> Draw {
        Draw::Bin(Op::Add, Box::new(a.into()), Box::new(b.into()))
    }

    #[test]
    fn a_sum_lands_in_the_summed_interval_and_leans_to_the_middle() {
        let d = sum(Dist::flat(1.0, 2.0), Dist::flat(3.0, 4.0));
        assert_eq!(d.bounds(), (4.0, 6.0));
        let mut mid = 0;
        for i in 0..4000 {
            let v = d.draw(&mut Rng::new(crate::Seed::new(i)));
            assert!((4.0..=6.0).contains(&v), "{v}");
            mid += usize::from((4.5..5.5).contains(&v));
        }
        // a triangle: three quarters of two dice land in the middle half
        assert!((2800..3200).contains(&mid), "{mid}");
    }

    #[test]
    fn two_rolls_are_independent_and_the_stream_moves_once() {
        let s = Dist::flat(1.0, 2.0);
        let diff = Draw::Bin(Op::Sub, Box::new(s.into()), Box::new(s.into()));
        assert_eq!(diff.bounds(), (-1.0, 1.0));
        let (mut same, mut neg) = (0, 0);
        for i in 0..2000 {
            let v = diff.draw(&mut Rng::new(crate::Seed::new(i)));
            same += usize::from(v == 0.0);
            neg += usize::from(v < 0.0);
        }
        assert_eq!(same, 0);
        assert!((800..1200).contains(&neg), "{neg}");
        // the rng ends where one range leaves it
        let (mut a, mut b) = (Rng::new(crate::Seed::new(9)), Rng::new(crate::Seed::new(9)));
        diff.draw(&mut a);
        Draw::Range(s).draw(&mut b);
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn one_range_draws_as_the_dist_does() {
        for w in ALL {
            let d = Dist::new(2.0, 9.0, w);
            let (mut a, mut b) = (Rng::new(crate::Seed::new(5)), Rng::new(crate::Seed::new(5)));
            for _ in 0..50 {
                assert_eq!(
                    Draw::Range(d).draw(&mut a).to_bits(),
                    d.draw(&mut b).to_bits()
                );
            }
        }
    }

    #[test]
    fn interval_arithmetic_handles_mirrors_and_products() {
        let r = || Box::new(Draw::flat(-2.0, 3.0));
        assert_eq!(Draw::Neg(r()).bounds(), (-3.0, 2.0));
        assert_eq!(Draw::Bin(Op::Mul, r(), r()).bounds(), (-6.0, 9.0));
        assert_eq!(
            Draw::Bin(Op::Mul, r(), Box::new(Draw::Num(-2.0))).bounds(),
            (-6.0, 4.0)
        );
    }

    #[test]
    fn every_whole_number_is_as_likely_under_a_flat_range() {
        let mut seen = [0usize; 5];
        let mut rng = Rng::new(crate::Seed::new(3));
        for _ in 0..10_000 {
            seen[usize::try_from(Dist::int(1, 5, Weighting::Flat, &mut rng) - 1).unwrap()] += 1;
        }
        // the ends are no rarer than the middle (they were half as likely
        // when a count was `round(uniform(lo, hi))`)
        for n in seen {
            assert!((1800..2200).contains(&n), "{seen:?}");
        }
        // a sum falls in the same bins from its place along its ends
        let d = Draw::flat(1.0, 5.0);
        let mut seen = [0usize; 5];
        for _ in 0..10_000 {
            seen[usize::try_from(d.int_by(1.0, &mut rng) - 1).unwrap()] += 1;
        }
        for n in seen {
            assert!((1800..2200).contains(&n), "{seen:?}");
        }
        assert_eq!(Dist::bin(1, 5, 1.0), 5);
        assert_eq!(Dist::bin(1, 5, 0.0), 1);
        assert_eq!(Dist::int(3, 3, Weighting::Flat, &mut rng), 3);
    }
}
