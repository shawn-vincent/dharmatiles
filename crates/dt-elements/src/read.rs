//! An element's settings, typed once.  What a control may be (its range,
//! whether it takes a range, its words) is the schema's, checked when the
//! document is read (`dt_doc::schema::Control::check`); [`Settings`] is the
//! one place that takes a value apart, by what the element wants it as (a
//! number, a span, counts, a word), and refuses a value of the wrong shape.
//! An element asks for its controls by name and gets typed values or its
//! default; it never loops over the controls matching names or `Value`s.

use crate::{Control, Degrees, Dist, LookError, Span, Value, Weighting};

fn refuse(c: &Control, what: &str) -> LookError {
    LookError::control(
        &c.name,
        format!("`{}` must be {what}, not {}", c.name, c.value),
        format!("write {what}"),
    )
}

/// One number.
fn number(c: &Control) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(v),
        _ => Err(refuse(c, "a number")),
    }
}

/// One number or a range, as `(low, high)`.  A weighted range is refused:
/// the control does not draw with odds yet (`dt::weighting`).
fn span(c: &Control) -> Result<Span, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(Span::point(v)),
        Value::Range(a, b) if a.is_finite() && b.is_finite() => Ok(Span::new(a, b)),
        Value::Dist(_) => Err(LookError::weighting(
            &c.name,
            format!("`{}` does not take a weighting yet", c.name),
            format!(
                "write the range {}; weightings are accepted on size, count, perSquare, clumps, stems, height on reeds, tree widths and the wall's edges rounding",
                match c.value {
                    Value::Dist(d) => format!("{}..{}", d.lo, d.hi),
                    _ => String::new(),
                }
            ),
        )),
        _ => Err(refuse(c, "a number or a range")),
    }
}

/// One number, a range or a weighted range.
fn dist(c: &Control) -> Result<Dist, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(Dist::flat(v, v)),
        Value::Range(a, b) if a.is_finite() && b.is_finite() => Ok(Dist::flat(a, b)),
        Value::Dist(d) if d.lo.is_finite() && d.hi.is_finite() => Ok(d),
        _ => Err(refuse(c, "a number, a range or a weighted range")),
    }
}

/// A word.
fn word(c: &Control) -> Result<&str, LookError> {
    match &c.value {
        Value::Word(w) => Ok(w),
        _ => Err(refuse(c, "a word")),
    }
}

/// A word or a number, for a control the schema lets be either (a size
/// that is `rubble` or `1.2`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pick<'a> {
    /// The word written.
    Word(&'a str),
    /// The number written.
    Number(f64),
}

fn pick(c: &Control) -> Result<Pick<'_>, LookError> {
    match &c.value {
        Value::Word(w) => Ok(Pick::Word(w)),
        Value::Number(v) if v.is_finite() => Ok(Pick::Number(*v)),
        _ => Err(refuse(c, "a word or a number")),
    }
}

/// The controls one stack entry (or one child of a scatter) sets, read by
/// name.  A control written twice reads as its last.
#[derive(Clone, Copy)]
pub(crate) struct Settings<'a>(&'a [Control]);

impl<'a> Settings<'a> {
    pub(crate) fn new(controls: &'a [Control]) -> Self {
        Self(controls)
    }

    /// The control called `name`, if set.
    pub(crate) fn control(&self, name: &str) -> Option<&'a Control> {
        self.0.iter().rev().find(|c| c.name == name)
    }

    /// A number.
    pub(crate) fn number(&self, name: &str) -> Result<Option<f64>, LookError> {
        self.control(name).map(number).transpose()
    }

    /// A number, or `default`.
    pub(crate) fn number_or(&self, name: &str, default: f64) -> Result<f64, LookError> {
        Ok(self.number(name)?.unwrap_or(default))
    }

    /// A number or a range, as `(low, high)`.
    pub(crate) fn span(&self, name: &str) -> Result<Option<Span>, LookError> {
        self.control(name).map(span).transpose()
    }

    /// A number, a range or a weighted range: the control draws with its odds.
    pub(crate) fn dist(&self, name: &str) -> Result<Option<Dist>, LookError> {
        self.control(name).map(dist).transpose()
    }

    /// A [`Settings::dist`], or `default`.
    pub(crate) fn dist_or(&self, name: &str, default: Dist) -> Result<Dist, LookError> {
        Ok(self.dist(name)?.unwrap_or(default))
    }

    /// A span, or `default`.
    pub(crate) fn span_or(&self, name: &str, default: Span) -> Result<Span, LookError> {
        Ok(self.span(name)?.unwrap_or(default))
    }

    /// A count or a range of them, as `(low, high)` rounded, with the odds
    /// along it.  Integer controls keep their rounding rule: a flat range
    /// is `rng.int(lo, hi)`, a weighted one `Dist::int`.
    pub(crate) fn counts_dist_or(
        &self,
        name: &str,
        default: (i64, i64),
    ) -> Result<((i64, i64), Weighting), LookError> {
        #[allow(clippy::cast_possible_truncation, reason = "the schema bounds counts")]
        let r = |v: f64| libm::round(v) as i64;
        Ok(self.dist(name)?.map_or((default, Weighting::Flat), |d| {
            ((r(d.lo), r(d.hi)), d.weighting)
        }))
    }

    /// A word or a number.
    pub(crate) fn pick(&self, name: &str) -> Result<Option<Pick<'a>>, LookError> {
        self.control(name).map(pick).transpose()
    }

    /// The word and the numbers behind it (`rock`, `rock.columns`, …) in
    /// document order, as `(name, what)`: a later write wins, so the caller
    /// folds them in order.
    pub(crate) fn family(&self, family: &str) -> Result<Vec<(&'a str, Pick<'a>)>, LookError> {
        self.0
            .iter()
            .filter(|c| {
                c.name == family
                    || c.name
                        .strip_prefix(family)
                        .is_some_and(|rest| rest.starts_with('.'))
            })
            .map(|c| pick(c).map(|p| (c.name.as_str(), p)))
            .collect()
    }

    /// An angle in degrees.
    pub(crate) fn degrees(&self, name: &str) -> Result<Option<Degrees>, LookError> {
        Ok(self.number(name)?.map(Degrees))
    }

    /// A word.
    pub(crate) fn word(&self, name: &str) -> Result<Option<&'a str>, LookError> {
        self.control(name).map(word).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, value: Value) -> Control {
        Control {
            name: name.into(),
            value,
        }
    }

    #[test]
    fn spans_take_a_number_or_a_range() {
        let a = [c("h", Value::Number(3.0)), c("r", Value::Range(2.0, 4.0))];
        let s = Settings::new(&a);
        assert_eq!(s.span("h"), Ok(Some(Span::point(3.0))));
        assert_eq!(s.span("r"), Ok(Some(Span::new(2.0, 4.0))));
        assert_eq!(s.span("none"), Ok(None));
        assert!(s.number("r").is_err());
        let w = [c("h", Value::Word("tall".into()))];
        let e = Settings::new(&w).span("h").unwrap_err();
        assert_eq!(e.control.as_deref(), Some("h"));
    }

    #[test]
    fn a_weighted_range_goes_where_the_control_draws() {
        let d = Dist::new(1.0, 6.0, Weighting::Low(11.0));
        let a = [c("size", Value::Dist(d)), c("n", Value::Range(2.0, 4.0))];
        let s = Settings::new(&a);
        assert_eq!(s.dist("size"), Ok(Some(d)));
        assert_eq!(s.dist("n"), Ok(Some(Dist::flat(2.0, 4.0))));
        let e = s.span("size").unwrap_err();
        assert_eq!(e.code, Some("dt::weighting"));
        assert!(e.message.contains("`size` does not take a weighting yet"));
        assert!(e.help.unwrap().contains("write the range 1..6"));
        assert!(
            Settings::new(&[c("w", Value::Word("x".into()))])
                .dist("w")
                .is_err()
        );
    }

    #[test]
    fn counts_round_and_the_last_write_wins() {
        let a = [c("n", Value::Range(1.4, 2.6))];
        assert_eq!(
            Settings::new(&a).counts_dist_or("n", (0, 0)),
            Ok(((1, 3), Weighting::Flat))
        );
        let w = [c(
            "n",
            Value::Dist(Dist::new(1.4, 9.6, Weighting::Low(3.0))),
        )];
        assert_eq!(
            Settings::new(&w).counts_dist_or("n", (0, 0)),
            Ok(((1, 10), Weighting::Low(3.0)))
        );
        let b = [c("n", Value::Number(1.0)), c("n", Value::Number(2.0))];
        assert_eq!(Settings::new(&b).number_or("n", 0.0), Ok(2.0));
    }
}
