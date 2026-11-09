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
use std::sync::Arc;

/// A call that does not make a weighting: what is wrong and what to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightingError {
    /// The diagnostic code: `dt::range` for a number outside its limits,
    /// `dt::weighting` for a weighting used in a way it cannot be.
    pub code: &'static str,
    /// What is wrong.
    pub message: String,
    /// What to write instead.
    pub help: String,
}

/// One number a weighting function takes after the range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightingArg {
    /// Its name.
    pub name: &'static str,
    /// What it is when left out; `None` when it must be written.
    pub default: Option<f64>,
    /// The least it may be.
    pub least: f64,
    /// True when `least` itself is refused ("above 0").
    pub above: bool,
    /// The most it may be; infinite for no limit.
    pub most: f64,
}

/// The numbers a function takes any number of: each bin's weight in
/// `steps`, each part's weight in `mix`, each value's weight in a list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightingRest {
    /// What one is called.
    pub name: &'static str,
    /// The least one may be (0 is allowed while another is above 0).
    pub least: f64,
    /// The fewest to write.
    pub at_least: usize,
    /// The most to write.
    pub at_most: usize,
}

/// A weighting function: how it is written, its numbers and what it does.
/// One list, for the checks, the documentation and `dharmatiles elements`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightingFn {
    /// The function's name (`list` is the bracketed list of values).
    pub name: &'static str,
    /// The call as written, with the defaults left out.
    pub call: &'static str,
    /// The numbers after the range, in order.
    pub args: &'static [WeightingArg],
    /// The numbers it takes any number of, after those.
    pub rest: Option<WeightingRest>,
    /// What it does.
    pub about: &'static str,
}

/// Most bins `steps` takes.
pub const MAX_BINS: usize = 32;
/// Most values a list takes.
pub const MAX_LIST: usize = 64;
/// Most parts a `mix` takes.
pub const MAX_MIX: usize = 16;
/// Least and most power of `ends`.
pub const ENDS_POWER: (f64, f64) = (0.0, 8.0);

const fn arg(name: &'static str, default: f64, least: f64, most: f64) -> WeightingArg {
    WeightingArg {
        name,
        default: Some(default),
        least,
        above: false,
        most,
    }
}

const RATIO: WeightingArg = WeightingArg {
    name: "ratio",
    default: Some(4.0),
    least: 0.0,
    above: true,
    most: f64::INFINITY,
};

/// The weighting functions.  Nine: the eight written as calls and the list.
pub const WEIGHTINGS: &[WeightingFn] = &[
    WeightingFn {
        name: "flat",
        call: "flat(range)",
        args: &[],
        rest: None,
        about: "a plain range: every value equally likely (same as writing the range alone)",
    },
    WeightingFn {
        name: "low",
        call: "low(range, ratio = 4)",
        args: &[RATIO],
        rest: None,
        about: "mostly low: the low end is `ratio` times as likely as the high end; 1 is flat, a fraction leans high",
    },
    WeightingFn {
        name: "high",
        call: "high(range, ratio = 4)",
        args: &[RATIO],
        rest: None,
        about: "mostly high: the mirror of low",
    },
    WeightingFn {
        name: "bell",
        call: "bell(range, spread = 0.2, at = 0.5)",
        args: &[
            arg("spread", 0.2, SPREAD.0, SPREAD.1),
            arg("at", 0.5, 0.0, 1.0),
        ],
        rest: None,
        about: "mostly near `at`, falling off as a normal curve over `spread` (a share of the width), cut at the range",
    },
    WeightingFn {
        name: "peak",
        call: "peak(range, spread = 0.5, at = 0.5)",
        args: &[
            arg("spread", 0.5, SPREAD.0, SPREAD.1),
            arg("at", 0.5, 0.0, 1.0),
        ],
        rest: None,
        about: "a favourite value `at`, falling off in a straight line over `spread`, cut at the range",
    },
    WeightingFn {
        name: "ends",
        call: "ends(range, k = 1, at = 0.5)",
        args: &[
            arg("k", 1.0, ENDS_POWER.0, ENDS_POWER.1),
            arg("at", 0.5, 0.0, 1.0),
        ],
        rest: None,
        about: "mostly away from `at`, the bell turned over: likelihood grows as the distance from `at` to the power `k`; 1 is a straight V, 2 a parabola, 0 flat",
    },
    WeightingFn {
        name: "steps",
        call: "steps(range, weight, weight, …)",
        args: &[],
        rest: Some(WeightingRest {
            name: "weight",
            least: 0.0,
            at_least: 2,
            at_most: MAX_BINS,
        }),
        about: "drawn by hand: the range cut into as many equal bins as there are weights, each bin as likely as its weight, flat inside",
    },
    WeightingFn {
        name: "list",
        call: "[value:weight, value, …]",
        args: &[],
        rest: Some(WeightingRest {
            name: "weight",
            least: 0.0,
            at_least: 1,
            at_most: MAX_LIST,
        }),
        about: "one of the values written, each as likely as its weight (1 when left out); there is no range, only these values",
    },
    WeightingFn {
        name: "mix",
        call: "mix(part, weight, part, weight, …)",
        args: &[],
        rest: Some(WeightingRest {
            name: "weight",
            least: 0.0,
            at_least: 2,
            at_most: MAX_MIX,
        }),
        about: "a blend of weightings over the same range, each part as likely as its weight; a part is a call such as bell(1..6)",
    },
];

/// The functions written as calls over a range, for messages: `flat, low, high, bell, peak, ends, steps or mix`.
#[must_use]
pub fn weighting_names() -> String {
    let names: Vec<&str> = WEIGHTINGS
        .iter()
        .filter(|w| w.name != "list")
        .map(|w| w.name)
        .collect();
    match names.split_last() {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The odds along a range, as a quantile map.  See the module docs.
#[derive(Clone, Debug, PartialEq)]
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
    /// A valley at `at`: density proportional to `|t - at|^k`.
    Ends {
        /// The power: 0 is flat, 1 a straight V, 2 a parabola.
        k: f64,
        /// Where the valley is, 0 to 1.
        at: f64,
    },
    /// Equal bins along the range, each with its own weight, flat inside.
    Steps(Arc<[f64]>),
    /// Values along the range as positions `t` (0 to 1, ascending) with
    /// their weights: one of them is drawn.  Built by [`Dist::list`].
    List(Arc<[(f64, f64)]>),
    /// Weightings over the same range, blended by their weights.
    Mix(Arc<[(f64, Weighting)]>),
}

/// Least and most `spread` of a bell or a peak.
pub const SPREAD: (f64, f64) = (0.05, 1.0);

fn bad(message: String, help: &str) -> WeightingError {
    WeightingError {
        code: "dt::range",
        message,
        help: help.to_owned(),
    }
}

fn misuse(message: String, help: String) -> WeightingError {
    WeightingError {
        code: "dt::weighting",
        message,
        help,
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

fn at_arg(name: &str, at: Option<f64>) -> Result<f64, WeightingError> {
    let at = at.unwrap_or(0.5);
    if at.is_finite() && (0.0..=1.0).contains(&at) {
        Ok(at)
    } else {
        let poss = if name.ends_with('s') {
            format!("{name}'")
        } else {
            format!("{name}'s")
        };
        Err(bad(
            format!("{poss} centre must be from 0 to 1, not {at}"),
            "the centre is a share of the width: 0 is the low end, 1 the high end",
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
    Ok((spread, at_arg(name, args.get(1).copied())?))
}

/// Weights: finite, none below 0, at least one above 0.
fn check_weights(name: &str, ws: &[f64]) -> Result<(), WeightingError> {
    if let Some(w) = ws.iter().find(|w| !w.is_finite() || **w < 0.0) {
        return Err(bad(
            format!("a weight in {name} must be 0 or more, not {w}"),
            "a weight is how likely a part is next to the others: 1 is ordinary, 0 never",
        ));
    }
    if !ws.iter().any(|w| *w > 0.0) {
        return Err(bad(
            format!("{name} needs a weight above 0"),
            "give at least one part a weight above 0",
        ));
    }
    Ok(())
}

/// A number as an author writes it: no trailing noise.
fn num(x: f64) -> String {
    let t = format!("{x:.9}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".to_owned()
    } else {
        t.to_owned()
    }
}

impl Weighting {
    /// The weighting a call such as `low(1..6, 3)` names, from its numbers
    /// after the range.  (A list and a mix are built by [`Dist::list`] and
    /// [`Weighting::mix`].)
    ///
    /// # Errors
    /// An unknown function, too many numbers, or a number outside its
    /// limits.
    pub fn from_call(name: &str, args: &[f64]) -> Result<Self, WeightingError> {
        let most = match name {
            "flat" => 0,
            "low" | "high" => 1,
            "bell" | "peak" | "ends" => 2,
            "steps" => MAX_BINS,
            _ => {
                return Err(bad(
                    format!("unknown function `{name}`"),
                    &format!(
                        "weightings are {}, or a list like [1:4, 2:5]",
                        weighting_names()
                    ),
                ));
            }
        };
        if args.len() > most {
            return Err(if name == "steps" {
                bad(
                    format!(
                        "`steps` takes {MAX_BINS} weights at most, not {}",
                        args.len()
                    ),
                    "fewer, wider bins say the same: steps(1..6, 1, 4, 2)",
                )
            } else {
                bad(
                    format!("`{name}` takes the range and {most} number(s) at most"),
                    &format!("write {}", WeightingFn::call_of(name)),
                )
            });
        }
        Ok(match name {
            "flat" => Self::Flat,
            "low" => Self::Low(ratio(name, args, 4.0)?),
            "high" => Self::High(ratio(name, args, 4.0)?),
            "bell" => {
                let (spread, at) = centre(name, args, 0.2)?;
                Self::Bell { spread, at }
            }
            "peak" => {
                let (spread, at) = centre(name, args, 0.5)?;
                Self::Peak { spread, at }
            }
            "ends" => {
                let k = args.first().copied().unwrap_or(1.0);
                if !(k.is_finite() && (ENDS_POWER.0..=ENDS_POWER.1).contains(&k)) {
                    return Err(bad(
                        format!(
                            "ends' power must be from {} to {}, not {k}",
                            ENDS_POWER.0, ENDS_POWER.1
                        ),
                        "0 is flat, 1 a straight V, 2 a parabola: the higher, the harder the values crowd the ends",
                    ));
                }
                Self::Ends {
                    k,
                    at: at_arg(name, args.get(1).copied())?,
                }
            }
            _ => Self::steps(args)?,
        })
    }

    /// Equal bins, each with its own weight.
    ///
    /// # Errors
    /// Fewer than 2 weights or more than [`MAX_BINS`], a weight below 0, or
    /// none above 0.
    pub fn steps(weights: &[f64]) -> Result<Self, WeightingError> {
        if weights.len() < 2 {
            return Err(bad(
                "`steps` needs two weights at least".to_owned(),
                "one weight per bin: steps(1..6, 1, 4, 2) cuts the range in three",
            ));
        }
        if weights.len() > MAX_BINS {
            return Err(bad(
                format!(
                    "`steps` takes {MAX_BINS} weights at most, not {}",
                    weights.len()
                ),
                "fewer, wider bins say the same: steps(1..6, 1, 4, 2)",
            ));
        }
        check_weights("`steps`", weights)?;
        Ok(Self::Steps(weights.into()))
    }

    /// A blend of weightings over the same range, each part as likely as
    /// its weight.
    ///
    /// # Errors
    /// Fewer than 2 parts or more than [`MAX_MIX`], a part that is a list or
    /// another mix, a weight below 0, or none above 0.
    pub fn mix(parts: Vec<(f64, Weighting)>) -> Result<Self, WeightingError> {
        if parts.len() < 2 {
            return Err(bad(
                "`mix` blends two parts at least".to_owned(),
                "write mix(bell(1..6), 3, flat(1..6), 1)",
            ));
        }
        if parts.len() > MAX_MIX {
            return Err(bad(
                format!("`mix` takes {MAX_MIX} parts at most, not {}", parts.len()),
                "fewer parts say the same; a part may be steps(…) for anything drawn by hand",
            ));
        }
        if parts
            .iter()
            .any(|(_, p)| matches!(p, Self::List(_) | Self::Mix(_)))
        {
            return Err(misuse(
                "a part of a mix is a call over the range, not a list or another mix".to_owned(),
                "weightings do not nest: write the parts side by side in one mix(…)".to_owned(),
            ));
        }
        let ws: Vec<f64> = parts.iter().map(|p| p.0).collect();
        check_weights("`mix`", &ws)?;
        Ok(Self::Mix(parts.into()))
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
        match self {
            Self::Flat => Self::Flat,
            Self::Low(r) => Self::High(*r),
            Self::High(r) => Self::Low(*r),
            Self::Bell { spread, at } => Self::Bell {
                spread: *spread,
                at: 1.0 - at,
            },
            Self::Peak { spread, at } => Self::Peak {
                spread: *spread,
                at: 1.0 - at,
            },
            Self::Ends { k, at } => Self::Ends {
                k: *k,
                at: 1.0 - at,
            },
            Self::Steps(ws) => Self::Steps(ws.iter().rev().copied().collect()),
            Self::List(items) => {
                Self::List(items.iter().rev().map(|(t, w)| (1.0 - t, *w)).collect())
            }
            Self::Mix(parts) => Self::Mix(parts.iter().map(|(w, p)| (*w, p.mirrored())).collect()),
        }
    }

    /// Where along the range one uniform draw `u` lands, 0 to 1.  Monotone
    /// in `u`, and never outside [0, 1]; one draw, no retries.  The
    /// continuous weightings start at 0; `steps` and a list start at their
    /// first bin or value with a weight.
    #[must_use]
    pub fn unit(&self, u: f64) -> f64 {
        if matches!(self, Self::Flat) {
            return u;
        }
        let discrete = matches!(self, Self::Steps(_) | Self::List(_));
        if u <= 0.0 && !discrete {
            return 0.0;
        }
        let t = match self {
            Self::Flat => u,
            Self::Low(r) => low(u, *r),
            Self::High(r) => low(u, 1.0 / r),
            Self::Bell { spread, at } => bell(u, *spread, *at),
            Self::Peak { spread, at } => peak(u, *spread, *at),
            Self::Ends { k, at } => ends(u, *k, *at),
            Self::Steps(ws) => steps(u, ws),
            Self::List(items) => list(u, items),
            Self::Mix(parts) => mix(u, parts),
        };
        // a NaN cannot get out
        if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
    }

    /// The call that writes it over `range`, as written.  A list is written
    /// by [`Dist::written`], which knows its values.
    fn text(&self, range: &str) -> String {
        let call = |name: &str, args: &[f64]| {
            let mut t = format!("{name}({range}");
            for a in args {
                t.push_str(", ");
                t.push_str(&num(*a));
            }
            t.push(')');
            t
        };
        match self {
            Self::Flat => call("flat", &[]),
            Self::Low(r) => call("low", &[*r]),
            Self::High(r) => call("high", &[*r]),
            Self::Bell { spread, at } => call("bell", &[*spread, *at]),
            Self::Peak { spread, at } => call("peak", &[*spread, *at]),
            Self::Ends { k, at } => call("ends", &[*k, *at]),
            Self::Steps(ws) => call("steps", ws),
            Self::List(_) => range.to_owned(),
            Self::Mix(parts) => {
                let inner: Vec<String> = parts
                    .iter()
                    .map(|(w, p)| format!("{}, {}", p.text(range), num(*w)))
                    .collect();
                format!("mix({})", inner.join(", "))
            }
        }
    }

    /// How likely each of `bins` equal slices of the range is, as a multiple
    /// of a flat range's share (a flat range is all 1s; the mean is 1).
    /// `None` for a list, which has values and no slices (see
    /// [`Weighting::masses`]).  What the editor draws as the curve.
    #[must_use]
    pub fn density(&self, bins: usize) -> Option<Vec<f64>> {
        if matches!(self, Self::List(_)) || bins == 0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let n = bins as f64;
        let mut at = 0.0;
        Some(
            (1..=bins)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = i as f64 / n;
                    let d = self.cdf(x) - self.cdf(at);
                    at = x;
                    // rounding in a cdf must not draw a hollow
                    (d * n).max(0.0)
                })
                .collect(),
        )
    }

    /// For a list: each value's position along the range (0 to 1) and its
    /// share of the draws (they add to 1).  `None` for every other weighting.
    #[must_use]
    pub fn masses(&self) -> Option<Vec<(f64, f64)>> {
        let Self::List(items) = self else {
            return None;
        };
        let total: f64 = items.iter().map(|i| i.1).sum();
        Some(items.iter().map(|&(t, w)| (t, w / total)).collect())
    }

    /// The share of the range at or below position `x` (0 to 1), for a
    /// weighting that has a density; a list has none.
    fn cdf(&self, x: f64) -> f64 {
        match self {
            Self::Flat | Self::List(_) => x,
            Self::Low(r) => low_cdf(x, *r),
            Self::High(r) => low_cdf(x, 1.0 / r),
            Self::Bell { spread, at } => bell_cdf(x, *spread, *at),
            Self::Peak { spread, at } => peak_cdf(x, *spread, *at),
            Self::Ends { k, at } => ends_cdf(x, *k, *at),
            Self::Steps(ws) => steps_cdf(x, ws),
            Self::Mix(parts) => mix_cdf(x, parts),
        }
    }
}

impl WeightingFn {
    /// The call as written, for the function called `name`.
    #[must_use]
    pub fn call_of(name: &str) -> &'static str {
        WEIGHTINGS
            .iter()
            .find(|w| w.name == name)
            .map_or("", |w| w.call)
    }
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

/// The antiderivative of `|x - at|^k`, with its sign: `d |d|^k / (k + 1)`
/// for `d = x - at`, `p = k + 1`.
fn ends_area(x: f64, at: f64, p: f64) -> f64 {
    let d = x - at;
    let m = libm::pow(d.abs(), p) / p;
    if d < 0.0 { -m } else { m }
}

/// A valley at `at`, truncated to [0, 1]: the inverse of the area under
/// `|x - at|^k`, by `pow`.
fn ends(u: f64, k: f64, at: f64) -> f64 {
    let p = k + 1.0;
    let g0 = ends_area(0.0, at, p);
    let g1 = ends_area(1.0, at, p);
    let g = g0 + u * (g1 - g0);
    let m = libm::pow(g.abs() * p, 1.0 / p);
    if g < 0.0 { at - m } else { at + m }
}

fn ends_cdf(x: f64, k: f64, at: f64) -> f64 {
    let p = k + 1.0;
    let g0 = ends_area(0.0, at, p);
    let g1 = ends_area(1.0, at, p);
    (ends_area(x, at, p) - g0) / (g1 - g0)
}

/// Equal bins with a weight each: the bin `u` falls in by cumulative
/// weight, and how far along it.  Bins of weight 0 are skipped.
fn steps(u: f64, ws: &[f64]) -> f64 {
    let total: f64 = ws.iter().sum();
    let target = u * total;
    #[allow(clippy::cast_precision_loss)]
    let n = ws.len() as f64;
    let (mut cum, mut end) = (0.0, 0.0);
    for (i, &w) in ws.iter().enumerate() {
        if w > 0.0 {
            let next = cum + w;
            #[allow(clippy::cast_precision_loss)]
            let i = i as f64;
            if target < next {
                return (i + (target - cum) / w) / n;
            }
            end = (i + 1.0) / n;
            cum = next;
        }
    }
    // rounding put the target at the very top: the end of the last bin
    end
}

fn steps_cdf(x: f64, ws: &[f64]) -> f64 {
    let total: f64 = ws.iter().sum();
    #[allow(clippy::cast_precision_loss)]
    let pos = x * ws.len() as f64;
    let mut cum = 0.0;
    for (i, &w) in ws.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let i = i as f64;
        if pos < i + 1.0 {
            return (cum + w * (pos - i).max(0.0)) / total;
        }
        cum += w;
    }
    1.0
}

/// One of the listed positions, by cumulative weight; weight 0 never.
fn list(u: f64, items: &[(f64, f64)]) -> f64 {
    let total: f64 = items.iter().map(|i| i.1).sum();
    let target = u * total;
    let (mut cum, mut last) = (0.0, 0.0);
    for &(t, w) in items {
        if w > 0.0 {
            cum += w;
            last = t;
            if target < cum {
                return t;
            }
        }
    }
    last
}

/// The blended CDF of a mix.
fn mix_cdf(x: f64, parts: &[(f64, Weighting)]) -> f64 {
    let total: f64 = parts.iter().map(|p| p.0).sum();
    let mut acc = 0.0;
    for (w, p) in parts {
        if *w > 0.0 {
            acc += (w / total) * p.cdf(x);
        }
    }
    acc
}

/// A mix by its blended CDF, inverted by a fixed 64-step bisection (never
/// a loop that ends when it feels close): one draw, the same bits everywhere.
fn mix(u: f64, parts: &[(f64, Weighting)]) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if mix_cdf(mid, parts) < u {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// The share at or below `x` of density `r^(-t)`.
fn low_cdf(x: f64, r: f64) -> f64 {
    if r == 1.0 {
        return x;
    }
    let ln_r = libm::log1p(r - 1.0);
    libm::expm1(-x * ln_r) / libm::expm1(-ln_r)
}

fn bell_cdf(x: f64, s: f64, at: f64) -> f64 {
    let a = phi(-at / s);
    let b = phi((1.0 - at) / s);
    (phi((x - at) / s) - a) / (b - a)
}

fn peak_cdf(x: f64, s: f64, c: f64) -> f64 {
    let g0 = tent_area(0.0, c, s);
    let g1 = tent_area(1.0, c, s);
    (tent_area(x, c, s) - g0) / (g1 - g0)
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
#[derive(Clone, Debug, PartialEq)]
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
    pub fn new(lo: f64, hi: f64, weighting: Weighting) -> Self {
        Self { lo, hi, weighting }
    }

    /// The plain range.
    #[must_use]
    pub const fn flat(lo: f64, hi: f64) -> Self {
        Self {
            lo,
            hi,
            weighting: Weighting::Flat,
        }
    }

    /// A list of values with weights (`None` is 1): one of the values is
    /// drawn.  They are kept in ascending order, so one uniform draw still
    /// maps monotonically; the range is from the least to the greatest.
    ///
    /// # Errors
    /// More than [`MAX_LIST`] values, a value or weight that is not finite,
    /// a weight below 0, none above 0, or a value written twice
    /// (`dt::weighting`, with the list to write instead).
    pub fn list(items: &[(f64, Option<f64>)]) -> Result<Self, WeightingError> {
        if items.is_empty() {
            return Err(bad(
                "a list needs a value".to_owned(),
                "write [1:4, 2:5, 3:1]: each value, and its weight after a colon",
            ));
        }
        if items.len() > MAX_LIST {
            return Err(bad(
                format!(
                    "a list holds {MAX_LIST} values at most, not {}",
                    items.len()
                ),
                "a range 1..6 covers every value between; steps(…) draws a histogram by hand",
            ));
        }
        if let Some((v, _)) = items.iter().find(|(v, _)| !v.is_finite()) {
            return Err(bad(
                format!("a value in a list must be a number, not {v}"),
                "write [1:4, 2:5, 3:1]",
            ));
        }
        let weights: Vec<f64> = items.iter().map(|i| i.1.unwrap_or(1.0)).collect();
        check_weights("a list", &weights)?;
        // a value written twice: say it once, with the weights added
        let mut merged: Vec<(f64, f64)> = Vec::new();
        let mut twice = None;
        for (&(v, _), &w) in items.iter().zip(&weights) {
            if let Some(m) = merged.iter_mut().find(|m| m.0 == v) {
                m.1 += w;
                twice.get_or_insert(v);
            } else {
                merged.push((v, w));
            }
        }
        if let Some(v) = twice {
            let show = |it: &mut dyn Iterator<Item = (f64, Option<f64>)>| {
                it.map(|(v, w)| w.map_or_else(|| num(v), |w| format!("{}:{}", num(v), num(w))))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let written = show(&mut items.iter().copied());
            let once = show(&mut merged.iter().map(|&(v, w)| (v, Some(w))));
            return Err(misuse(
                format!("{} is listed twice in [{written}]", num(v)),
                format!("give each value once, with its total weight: [{once}]"),
            ));
        }
        merged.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (lo, hi) = (merged[0].0, merged[merged.len() - 1].0);
        let span = hi - lo;
        let at: Arc<[(f64, f64)]> = merged
            .iter()
            .map(|&(v, w)| (if span > 0.0 { (v - lo) / span } else { 0.0 }, w))
            .collect();
        Ok(Self {
            lo,
            hi,
            weighting: Weighting::List(at),
        })
    }

    /// As an author writes it: `1..6`, `low(1..6, 4)`, `[1:4, 2:5]`.
    #[must_use]
    pub fn written(&self) -> String {
        if let Weighting::List(items) = &self.weighting {
            let parts: Vec<String> = items
                .iter()
                .map(|&(t, w)| {
                    let v = num(self.lo + (self.hi - self.lo) * t);
                    if w == 1.0 {
                        v
                    } else {
                        format!("{v}:{}", num(w))
                    }
                })
                .collect();
            return format!("[{}]", parts.join(", "));
        }
        let range = format!("{}..{}", num(self.lo), num(self.hi));
        match &self.weighting {
            Weighting::Flat => range,
            w => w.text(&range),
        }
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
    pub fn int(lo: i64, hi: i64, weighting: &Weighting, rng: &mut Rng) -> i64 {
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

    /// Every length scaled by `k` (a size in mm follows the system): a
    /// range keeps its odds and has both ends scaled, so one range draws
    /// exactly what a range of the scaled ends does.  A product or a quotient
    /// scales its left side.
    #[must_use]
    pub fn scaled(&self, k: f64) -> Self {
        match self {
            Self::Num(x) => Self::Num(x * k),
            Self::Range(d) => Self::Range(Dist::new(d.lo * k, d.hi * k, d.weighting.clone())),
            Self::Neg(a) => Self::Neg(Box::new(a.scaled(k))),
            Self::Bin(op @ (Op::Add | Op::Sub), a, b) => {
                Self::Bin(*op, Box::new(a.scaled(k)), Box::new(b.scaled(k)))
            }
            Self::Bin(op, a, b) => Self::Bin(*op, Box::new(a.scaled(k)), b.clone()),
        }
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
            Self::Range(d) => Some(d.clone()),
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
            Self::Range(d) => Dist::int(l, h, &d.weighting, rng),
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
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    fn all() -> Vec<Weighting> {
        let dist = |items: &[(f64, Option<f64>)]| Dist::list(items).unwrap().weighting;
        vec![
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
            Weighting::Ends { k: 1.0, at: 0.5 },
            Weighting::Ends { k: 2.0, at: 0.0 },
            Weighting::Ends { k: 0.0, at: 0.3 },
            Weighting::Ends { k: 8.0, at: 1.0 },
            Weighting::steps(&[1.0, 4.0, 2.0]).unwrap(),
            Weighting::steps(&[0.0, 3.0, 0.0, 1.0]).unwrap(),
            dist(&[(1.0, Some(4.0)), (2.0, Some(5.0)), (3.0, Some(1.0))]),
            dist(&[(3.0, None), (1.0, None), (2.0, Some(0.0))]),
            Weighting::mix(vec![
                (
                    3.0,
                    Weighting::Bell {
                        spread: 0.2,
                        at: 0.5,
                    },
                ),
                (1.0, Weighting::Flat),
            ])
            .unwrap(),
            Weighting::mix(vec![
                (1.0, Weighting::Ends { k: 2.0, at: 0.5 }),
                (2.0, Weighting::steps(&[1.0, 0.0, 5.0]).unwrap()),
                (0.5, Weighting::Low(9.0)),
            ])
            .unwrap(),
        ]
    }

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
        for w in all() {
            // the continuous weightings start at 0; steps and a list at their
            // first bin or value with a weight
            if !matches!(w, Weighting::Steps(_) | Weighting::List(_)) {
                assert_eq!(w.unit(0.0), 0.0, "{w:?}");
            }
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
        for w in all() {
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
                .contains("flat, low, high, bell, peak, ends, steps or mix, or a list")
        );
    }

    #[test]
    fn integer_draws_use_equal_bins() {
        let mut rng = Rng::new(crate::Seed::new(5).key("int"));
        let mut counts = [0u32; 6];
        for _ in 0..6000 {
            let k = Dist::int(1, 6, &Weighting::Low(4.0), &mut rng);
            assert!((1..=6).contains(&k));
            counts[usize::try_from(k - 1).unwrap()] += 1;
        }
        assert!(counts[0] > counts[5] * 2, "{counts:?}");
        // flat keeps the modulus
        let mut a = Rng::new(crate::Seed::new(5).key("int"));
        let mut b = a.clone();
        assert_eq!(Dist::int(1, 6, &Weighting::Flat, &mut a), b.int(1, 6));
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
        let diff = Draw::Bin(
            Op::Sub,
            Box::new(s.clone().into()),
            Box::new(s.clone().into()),
        );
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
        Draw::Range(s.clone()).draw(&mut b);
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn one_range_draws_as_the_dist_does() {
        for w in all() {
            let d = Dist::new(2.0, 9.0, w);
            let (mut a, mut b) = (Rng::new(crate::Seed::new(5)), Rng::new(crate::Seed::new(5)));
            for _ in 0..50 {
                assert_eq!(
                    Draw::Range(d.clone()).draw(&mut a).to_bits(),
                    d.draw(&mut b).to_bits()
                );
            }
        }
    }

    #[test]
    fn scaling_a_range_scales_its_ends_and_keeps_its_odds() {
        let d = Draw::Range(Dist::new(2.0, 6.0, Weighting::Low(3.0)));
        let k = 25.4 / 35.0;
        let (mut a, mut b) = (Rng::new(crate::Seed::new(8)), Rng::new(crate::Seed::new(8)));
        for _ in 0..50 {
            let scaled = Dist::new(2.0 * k, 6.0 * k, Weighting::Low(3.0));
            assert_eq!(
                d.scaled(k).draw(&mut a).to_bits(),
                scaled.draw(&mut b).to_bits()
            );
        }
        let sum = sum(Dist::flat(1.0, 2.0), Dist::flat(3.0, 4.0));
        assert_eq!(sum.scaled(2.0).bounds(), (8.0, 12.0));
        let prod = Draw::Bin(
            Op::Mul,
            Box::new(Draw::flat(1.0, 2.0)),
            Box::new(Draw::Num(3.0)),
        );
        assert_eq!(prod.scaled(2.0).bounds(), (6.0, 12.0));
        assert_eq!(Draw::Num(4.0).scaled(0.5), Draw::Num(2.0));
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
            seen[usize::try_from(Dist::int(1, 5, &Weighting::Flat, &mut rng) - 1).unwrap()] += 1;
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
        assert_eq!(Dist::int(3, 3, &Weighting::Flat, &mut rng), 3);
    }

    #[test]
    fn the_rest_of_the_set_draws_to_the_last_bit() {
        // pinned: a change here moves every tile that writes the function
        let bits = |w: &Weighting, u: f64| w.unit(u).to_bits();
        let ends1 = Weighting::Ends { k: 1.0, at: 0.5 };
        assert_eq!(bits(&ends1, 0.1), 0x3fab_06d1_d200_9138);
        assert_eq!(bits(&ends1, 0.5), 0x3fe0_0000_0000_0000);
        assert_eq!(bits(&ends1, 0.9), 0x3fee_4f92_e2df_f6ec);
        let ends2 = Weighting::Ends { k: 2.0, at: 0.25 };
        assert_eq!(bits(&ends2, 0.1), 0x3fe1_bb45_1a9d_d88e);
        assert_eq!(bits(&ends2, 0.9), 0x3fef_23d1_86c6_42ed);
        let steps = Weighting::steps(&[1.0, 4.0, 2.0]).unwrap();
        assert_eq!(bits(&steps, 0.1), 0x3fcd_dddd_dddd_dddf);
        assert_eq!(bits(&steps, 0.5), 0x3fe1_5555_5555_5555);
        assert_eq!(bits(&steps, 0.9), 0x3fec_4444_4444_4444);
        let list = Dist::list(&[(1.0, Some(4.0)), (2.0, Some(5.0)), (3.0, Some(1.0))])
            .unwrap()
            .weighting;
        assert_eq!(bits(&list, 0.1), 0);
        assert_eq!(bits(&list, 0.5), 0x3fe0_0000_0000_0000);
        assert_eq!(bits(&list, 0.9), 0x3ff0_0000_0000_0000);
        let mix = Weighting::mix(vec![
            (
                3.0,
                Weighting::Bell {
                    spread: 0.2,
                    at: 0.5,
                },
            ),
            (1.0, Weighting::Flat),
        ])
        .unwrap();
        assert_eq!(bits(&mix, 0.1), 0x3fca_471f_bc89_b6aa);
        assert_eq!(bits(&mix, 0.5), 0x3fe0_0000_0000_0000);
        assert_eq!(bits(&mix, 0.9), 0x3fe9_6e38_10dd_9255);
    }

    #[test]
    fn ends_is_the_closed_form_inverse() {
        // a straight V centred at 0.5: half the weight each side, density |t - 0.5|
        let w = Weighting::Ends { k: 1.0, at: 0.5 };
        assert!((w.unit(0.25) - (0.5 - 0.5 * 0.5_f64.sqrt())).abs() < 1e-12);
        // 0 is flat, whatever the centre
        for at in [0.0, 0.3, 1.0] {
            let w = Weighting::Ends { k: 0.0, at };
            for u in [0.1, 0.5, 0.97] {
                assert!((w.unit(u) - u).abs() < 1e-12, "at {at} u {u}");
            }
        }
        // the CDF is the inverse: cdf(unit(u)) = u
        for w in [
            Weighting::Ends { k: 2.0, at: 0.3 },
            Weighting::Ends { k: 5.0, at: 1.0 },
        ] {
            for i in 1..100 {
                let u = f64::from(i) / 100.0;
                assert!((w.cdf(w.unit(u)) - u).abs() < 1e-9, "{w:?} {u}");
            }
        }
        // valleys favour the ends; an off-centre valley favours the far end
        let v = Weighting::Ends { k: 2.0, at: 0.0 };
        assert!(v.unit(0.5) > 0.7);
        // mirrored is the valley from the other side
        let m = Weighting::Ends { k: 2.0, at: 0.2 }.mirrored();
        assert_eq!(m, Weighting::Ends { k: 2.0, at: 0.8 });
    }

    #[test]
    fn steps_follow_their_weights() {
        let w = Weighting::steps(&[1.0, 0.0, 3.0]).unwrap();
        let mut bins = [0u32; 3];
        for i in 0..4000 {
            let t = w.unit((f64::from(i) + 0.5) / 4000.0);
            bins[usize::try_from(libm::floor(t * 3.0) as i64).unwrap().min(2)] += 1;
        }
        assert_eq!(bins, [1000, 0, 3000]);
        // flat inside a bin
        let w = Weighting::steps(&[1.0, 1.0]).unwrap();
        assert!((w.unit(0.3) - 0.3).abs() < 1e-12);
        // a leading zero starts at the first bin with a weight
        let w = Weighting::steps(&[0.0, 1.0]).unwrap();
        assert!((0.5..=1.0).contains(&w.unit(0.0)));
        assert_eq!(w.mirrored(), Weighting::Steps([1.0, 0.0].as_slice().into()));
        // limits
        for bad in [
            vec![1.0],
            vec![],
            vec![0.0, 0.0],
            vec![1.0, -1.0],
            vec![1.0, f64::NAN],
        ] {
            let e = Weighting::steps(&bad).unwrap_err();
            assert_eq!(e.code, "dt::range", "{bad:?}");
            assert!(!e.help.is_empty());
        }
        assert!(Weighting::steps(&[1.0; MAX_BINS]).is_ok());
        let e = Weighting::steps(&[1.0; MAX_BINS + 1]).unwrap_err();
        assert!(e.message.contains("32 weights at most"), "{}", e.message);
    }

    #[test]
    fn a_list_draws_the_values_written_in_ascending_order() {
        let d = Dist::list(&[(3.0, Some(1.0)), (1.0, Some(4.0)), (2.0, Some(5.0))]).unwrap();
        assert_eq!((d.lo, d.hi), (1.0, 3.0));
        assert_eq!(d.written(), "[1:4, 2:5, 3]");
        let mut seen = [0u32; 3];
        for i in 0..1000 {
            let v = d.at((f64::from(i) + 0.5) / 1000.0);
            assert!([1.0, 2.0, 3.0].contains(&v), "{v}");
            seen[usize::try_from(v as i64 - 1).unwrap()] += 1;
        }
        assert_eq!(seen, [400, 500, 100]);
        // a missing weight is 1; 0 is allowed beside one above 0
        let d = Dist::list(&[(1.0, None), (2.5, Some(0.0)), (6.0, None)]).unwrap();
        for i in 0..100 {
            let v = d.at(f64::from(i) / 100.0);
            assert!(v == 1.0 || v == 6.0, "{v}");
        }
        // values need not be whole, and a list moves and scales with its range
        let d = Dist::list(&[(2.0, None), (5.0, Some(3.0)), (8.5, None)]).unwrap();
        assert_eq!(d.written(), "[2, 5:3, 8.5]");
        // whole numbers draw as a count
        let d = Dist::list(&[(1.0, None), (2.0, None), (3.0, None), (4.0, None)]).unwrap();
        let mut rng = Rng::new(crate::Seed::new(1));
        for _ in 0..200 {
            let k = Dist::int(1, 4, &d.weighting, &mut rng);
            assert!((1..=4).contains(&k));
        }
        // a value written twice is dt::weighting, with the list to write
        let e = Dist::list(&[(1.0, Some(4.0)), (2.0, Some(5.0)), (2.0, Some(1.0))]).unwrap_err();
        assert_eq!(e.code, "dt::weighting");
        assert_eq!(e.message, "2 is listed twice in [1:4, 2:5, 2:1]");
        assert_eq!(
            e.help,
            "give each value once, with its total weight: [1:4, 2:6]"
        );
        // limits
        for bad in [
            vec![(1.0, Some(0.0)), (2.0, Some(0.0))],
            vec![(1.0, Some(-1.0)), (2.0, None)],
            vec![(f64::NAN, None)],
            vec![],
        ] {
            assert_eq!(Dist::list(&bad).unwrap_err().code, "dt::range", "{bad:?}");
        }
        let many: Vec<(f64, Option<f64>)> = (0..64).map(|i| (f64::from(i), None)).collect();
        assert!(Dist::list(&many).is_ok());
        let many: Vec<(f64, Option<f64>)> = (0..65).map(|i| (f64::from(i), None)).collect();
        assert!(
            Dist::list(&many)
                .unwrap_err()
                .message
                .contains("64 values at most")
        );
        // one value is a number
        assert!(Dist::list(&[(4.0, None)]).unwrap().is_point());
    }

    #[test]
    fn a_mix_is_inverted_through_its_blended_cdf() {
        let w = Weighting::mix(vec![
            (3.0, Weighting::Low(5.0)),
            (1.0, Weighting::High(5.0)),
            (
                2.0,
                Weighting::Bell {
                    spread: 0.1,
                    at: 0.7,
                },
            ),
            (
                1.0,
                Weighting::Peak {
                    spread: 0.3,
                    at: 0.2,
                },
            ),
            (1.0, Weighting::Ends { k: 2.0, at: 0.5 }),
            (1.0, Weighting::steps(&[1.0, 2.0, 0.0, 4.0]).unwrap()),
        ])
        .unwrap();
        for i in 1..200 {
            let u = f64::from(i) / 200.0;
            assert!((w.cdf(w.unit(u)) - u).abs() < 1e-9, "{u}");
        }
        // one part with all the weight is that part
        let one = Weighting::mix(vec![(1.0, Weighting::Low(4.0)), (0.0, Weighting::Flat)]).unwrap();
        for u in [0.1, 0.5, 0.9] {
            assert!((one.unit(u) - Weighting::Low(4.0).unit(u)).abs() < 1e-12);
        }
        // a mix is not a list or a mix
        let list = Dist::list(&[(1.0, None), (2.0, None)]).unwrap().weighting;
        assert_eq!(
            Weighting::mix(vec![(1.0, list), (1.0, Weighting::Flat)])
                .unwrap_err()
                .code,
            "dt::weighting"
        );
        assert_eq!(
            Weighting::mix(vec![(1.0, w.clone()), (1.0, Weighting::Flat)])
                .unwrap_err()
                .code,
            "dt::weighting"
        );
        // limits
        assert!(Weighting::mix(vec![(1.0, Weighting::Flat)]).is_err());
        assert!(Weighting::mix(vec![(0.0, Weighting::Flat), (0.0, Weighting::Low(2.0))]).is_err());
        assert!(Weighting::mix(vec![(-1.0, Weighting::Flat), (1.0, Weighting::Low(2.0))]).is_err());
        let parts = |n: usize| vec![(1.0, Weighting::Flat); n];
        assert!(Weighting::mix(parts(MAX_MIX)).is_ok());
        assert!(
            Weighting::mix(parts(MAX_MIX + 1))
                .unwrap_err()
                .message
                .contains("16 parts at most")
        );
    }

    #[test]
    fn the_new_calls_are_checked_and_written_back() {
        assert_eq!(
            Weighting::from_call("ends", &[]),
            Ok(Weighting::Ends { k: 1.0, at: 0.5 })
        );
        for (name, args) in [
            ("ends", vec![-1.0]),
            ("ends", vec![9.0]),
            ("ends", vec![f64::NAN]),
            ("ends", vec![1.0, 1.5]),
            ("ends", vec![1.0, 0.5, 0.5]),
            ("steps", vec![1.0]),
            ("steps", vec![]),
        ] {
            let e = Weighting::from_call(name, &args).unwrap_err();
            assert!(!e.help.is_empty(), "{name} {args:?}");
        }
        assert_eq!(
            Weighting::from_call("ends", &[9.0]).unwrap_err().message,
            "ends' power must be from 0 to 8, not 9"
        );
        assert_eq!(
            Dist::new(1.0, 6.0, Weighting::Ends { k: 2.0, at: 0.5 }).written(),
            "ends(1..6, 2, 0.5)"
        );
        assert_eq!(
            Dist::new(1.0, 6.0, Weighting::steps(&[1.0, 4.0]).unwrap()).written(),
            "steps(1..6, 1, 4)"
        );
        let mix = Weighting::mix(vec![
            (
                3.0,
                Weighting::Bell {
                    spread: 0.2,
                    at: 0.5,
                },
            ),
            (1.0, Weighting::Flat),
        ])
        .unwrap();
        assert_eq!(
            Dist::new(1.0, 6.0, mix).written(),
            "mix(bell(1..6, 0.2, 0.5), 3, flat(1..6), 1)"
        );
        // the table lists nine functions, each with its defaults and limits
        let names: Vec<&str> = WEIGHTINGS.iter().map(|w| w.name).collect();
        assert_eq!(
            names,
            [
                "flat", "low", "high", "bell", "peak", "ends", "steps", "list", "mix"
            ]
        );
        for f in WEIGHTINGS {
            for a in f.args {
                let d = a.default.unwrap();
                assert!(d >= a.least && d <= a.most, "{} {}", f.name, a.name);
            }
        }
    }

    #[test]
    fn density_is_the_curve_the_editor_draws() {
        // a flat range is all 1s, and every density averages 1
        let flat = Weighting::Flat.density(64).unwrap();
        assert!(flat.iter().all(|d| (d - 1.0).abs() < 1e-12));
        for w in [
            Weighting::Low(4.0),
            Weighting::High(4.0),
            Weighting::Bell {
                spread: 0.2,
                at: 0.5,
            },
            Weighting::Peak {
                spread: 0.5,
                at: 0.3,
            },
            Weighting::Ends { k: 2.0, at: 0.5 },
            Weighting::steps(&[1.0, 3.0, 2.0]).unwrap(),
        ] {
            let d = w.density(64).unwrap();
            let mean = d.iter().sum::<f64>() / 64.0;
            assert!((mean - 1.0).abs() < 1e-9, "{w:?} {mean}");
            assert!(d.iter().all(|x| *x >= 0.0));
        }
        // low: the low end is `ratio` times as likely as the high end
        let low = Weighting::Low(4.0).density(64).unwrap();
        assert!((low[0] / low[63] - 4.0).abs() < 0.2);
        // high is its mirror
        let high = Weighting::High(4.0).density(64).unwrap();
        for (a, b) in low.iter().zip(high.iter().rev()) {
            assert!((a - b).abs() < 1e-9);
        }
        // a bell peaks in the middle; steps are flat inside a bin
        let bell = Weighting::Bell {
            spread: 0.2,
            at: 0.5,
        }
        .density(64)
        .unwrap();
        assert!(bell[32] > bell[0] * 10.0);
        let steps = Weighting::steps(&[1.0, 3.0]).unwrap().density(8).unwrap();
        assert!((steps[0] - steps[3]).abs() < 1e-9 && (steps[4] / steps[0] - 3.0).abs() < 1e-9);
        // a list has values and shares instead
        let list = Dist::list(&[(1.0, Some(3.0)), (2.0, None)])
            .unwrap()
            .weighting;
        assert!(list.density(64).is_none());
        let m = list.masses().unwrap();
        assert_eq!(m, [(0.0, 0.75), (1.0, 0.25)]);
        assert!(Weighting::Flat.masses().is_none());
    }
}
