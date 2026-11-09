//! An element's settings, typed once.  What a control may be (its range,
//! whether it takes a range, its words) is the schema's, checked when the
//! document is read (`dt_doc::schema::Control::check`); [`Settings`] is the
//! one place that takes a value apart, by what the element wants it as (a
//! number, a draw, a word), and refuses a value of the wrong shape.
//! An element asks for its controls by name and gets typed values or its
//! default; it never loops over the controls matching names or `Value`s.

use crate::{Control, Degrees, Draw, LookError, Value};

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

/// One number, a range, a weighted range or a sum of ranges.
fn dist(c: &Control) -> Result<Draw, LookError> {
    match &c.value {
        Value::Number(v) if v.is_finite() => Ok(Draw::flat(*v, *v)),
        Value::Range(a, b) if a.is_finite() && b.is_finite() => Ok(Draw::flat(*a, *b)),
        Value::Dist(d) if d.lo.is_finite() && d.hi.is_finite() => Ok(Draw::Range(d.clone())),
        Value::Draw(d) if d.lo().is_finite() && d.hi().is_finite() => Ok(d.clone()),
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

    /// A number, a range or a weighted range: the control draws with its odds.
    pub(crate) fn dist(&self, name: &str) -> Result<Option<Draw>, LookError> {
        self.control(name).map(dist).transpose()
    }

    /// A [`Settings::dist`], or `default`.
    pub(crate) fn dist_or(&self, name: &str, default: Draw) -> Result<Draw, LookError> {
        Ok(self.dist(name)?.unwrap_or(default))
    }

    /// A word or a number.
    pub(crate) fn pick(&self, name: &str) -> Result<Option<Pick<'a>>, LookError> {
        self.control(name).map(pick).transpose()
    }

    /// The word, the numbers it stands for (`rock.columns`, …) and the
    /// numbers written after it (`rock/columns`) in order, as
    /// `(name, what)`: a later write wins, so the caller folds them in
    /// order.
    pub(crate) fn family(&self, family: &str) -> Result<Vec<(&'a str, Pick<'a>)>, LookError> {
        self.0
            .iter()
            .filter(|c| {
                c.name == family
                    || c.name
                        .strip_prefix(family)
                        .is_some_and(|rest| rest.starts_with(['.', '/']))
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
    use crate::{Dist, Weighting};

    fn c(name: &str, value: Value) -> Control {
        Control {
            name: name.into(),
            value,
        }
    }

    #[test]
    fn a_draw_takes_a_number_a_range_or_odds() {
        let d = Dist::new(1.0, 6.0, Weighting::Low(11.0));
        let a = [
            c("h", Value::Number(3.0)),
            c("r", Value::Range(2.0, 4.0)),
            c("size", Value::Dist(d.clone())),
        ];
        let s = Settings::new(&a);
        assert_eq!(s.dist("h"), Ok(Some(Draw::flat(3.0, 3.0))));
        assert_eq!(s.dist("r"), Ok(Some(Draw::flat(2.0, 4.0))));
        assert_eq!(s.dist("size"), Ok(Some(Draw::Range(d))));
        assert_eq!(s.dist("none"), Ok(None));
        assert!(s.number("r").is_err());
        let w = [c("h", Value::Word("tall".into()))];
        let e = Settings::new(&w).dist("h").unwrap_err();
        assert_eq!(e.control.as_deref(), Some("h"));
    }

    #[test]
    fn counts_round_and_the_last_write_wins() {
        let a = [c("n", Value::Range(1.4, 2.6))];
        let n = Settings::new(&a).dist("n").unwrap().unwrap();
        let mut rng = dt_kit::Rng::new(dt_kit::Seed::new(1));
        for _ in 0..50 {
            assert!((1..=3).contains(&n.int_by(1.0, &mut rng)));
        }
        let b = [c("n", Value::Number(1.0)), c("n", Value::Number(2.0))];
        assert_eq!(Settings::new(&b).number_or("n", 0.0), Ok(2.0));
    }
}
