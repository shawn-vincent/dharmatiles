//! An element's settings, typed once.  What a control may be (its range,
//! whether it takes a range, its words) is the schema's, checked when the
//! document is read (`dt_doc::schema::Control::check`); [`Settings`] is the
//! one place that takes a value apart, by what the element wants it as (a
//! number, a span, counts, a word), and refuses a value of the wrong shape.
//! An element asks for its controls by name and gets typed values or its
//! default; it never loops over the controls matching names or `Value`s.

use crate::{Control, Degrees, LookError, Span, Value};

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

/// One number or a range, as `(low, high)`.
fn span(c: &Control) -> Result<Span, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(Span::point(v)),
        Value::Range(a, b) if a.is_finite() && b.is_finite() => Ok(Span::new(a, b)),
        _ => Err(refuse(c, "a number or a range")),
    }
}

/// A word.
fn word(c: &Control) -> Result<&str, LookError> {
    match &c.value {
        Value::Word(w) => Ok(w),
        _ => Err(refuse(c, "a word")),
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

    /// The first of `names` that is set: the strongest spelling first.
    pub(crate) fn first(&self, names: &[&str]) -> Option<&'a Control> {
        names.iter().find_map(|n| self.control(n))
    }

    /// The value as written, for a control that is a number or a word (or a
    /// range): the element says what each means.
    pub(crate) fn value(&self, name: &str) -> Option<&'a Value> {
        self.control(name).map(|c| &c.value)
    }

    /// A number.
    pub(crate) fn number(&self, name: &str) -> Result<Option<f64>, LookError> {
        self.control(name).map(number).transpose()
    }

    /// A number, or `default`.
    pub(crate) fn number_or(&self, name: &str, default: f64) -> Result<f64, LookError> {
        Ok(self.number(name)?.unwrap_or(default))
    }

    /// A number under any of `names` (the strongest first).
    pub(crate) fn number_of(&self, names: &[&str]) -> Result<Option<f64>, LookError> {
        self.first(names).map(number).transpose()
    }

    /// A number or a range, as `(low, high)`.
    pub(crate) fn span(&self, name: &str) -> Result<Option<Span>, LookError> {
        self.control(name).map(span).transpose()
    }

    /// A span, or `default`.
    pub(crate) fn span_or(&self, name: &str, default: Span) -> Result<Span, LookError> {
        Ok(self.span(name)?.unwrap_or(default))
    }

    /// A count or a range of them, as `(low, high)` rounded.
    pub(crate) fn counts_or(
        &self,
        name: &str,
        default: (i64, i64),
    ) -> Result<(i64, i64), LookError> {
        #[allow(clippy::cast_possible_truncation, reason = "the schema bounds counts")]
        let r = |v: f64| libm::round(v) as i64;
        Ok(self.span(name)?.map_or(default, |s| (r(s.lo), r(s.hi))))
    }

    /// A number or a range kept as written (a range is drawn per thing when
    /// it grows, so it must not be turned into a span).
    pub(crate) fn length(&self, name: &str) -> Result<Option<Value>, LookError> {
        self.control(name)
            .map(|c| span(c).map(|_| c.value.clone()))
            .transpose()
    }

    /// An angle in degrees.
    pub(crate) fn degrees_of(&self, names: &[&str]) -> Result<Option<Degrees>, LookError> {
        Ok(self.number_of(names)?.map(Degrees))
    }

    /// A word.
    pub(crate) fn word(&self, name: &str) -> Result<Option<&'a str>, LookError> {
        self.control(name).map(word).transpose()
    }
}

/// One number or range of a [`Value`], when it is not a word.
pub(crate) fn range_of(v: &Value) -> Option<Span> {
    match v {
        Value::Number(a) => Some(Span::point(*a)),
        Value::Range(a, b) => Some(Span::new(*a, *b)),
        Value::Word(_) => None,
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
    fn counts_round_and_the_last_write_wins() {
        let a = [c("n", Value::Range(1.4, 2.6))];
        assert_eq!(Settings::new(&a).counts_or("n", (0, 0)), Ok((1, 3)));
        let b = [c("n", Value::Number(1.0)), c("n", Value::Number(2.0))];
        assert_eq!(Settings::new(&b).number_or("n", 0.0), Ok(2.0));
    }
}
