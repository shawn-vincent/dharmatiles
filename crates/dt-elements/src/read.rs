//! Reading a control's value as the element uses it.  What a control may
//! be (its range, whether it takes a range, its words) is the schema's,
//! checked once when the document is read (`dt_doc::schema::Control::check`);
//! these only take the value apart, and refuse a value of the wrong shape.

use crate::{Control, LookError, Value};

fn refuse(c: &Control, what: &str) -> LookError {
    LookError::control(
        &c.name,
        format!("`{}` must be {what}, not {}", c.name, c.value),
        format!("write {what}"),
    )
}

/// One number.
pub(crate) fn number(c: &Control) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(v),
        _ => Err(refuse(c, "a number")),
    }
}

/// One number or a range, as `(low, high)`.
pub(crate) fn span(c: &Control) -> Result<(f64, f64), LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok((v, v)),
        Value::Range(a, b) if a.is_finite() && b.is_finite() => Ok((a, b)),
        _ => Err(refuse(c, "a number or a range")),
    }
}

/// A count or a range of them, as `(low, high)` rounded.
pub(crate) fn counts(c: &Control) -> Result<(i64, i64), LookError> {
    let (a, b) = span(c)?;
    #[allow(clippy::cast_possible_truncation, reason = "the schema bounds counts")]
    let r = |v: f64| libm::round(v) as i64;
    Ok((r(a), r(b)))
}

/// A word.
pub(crate) fn word(c: &Control) -> Result<&str, LookError> {
    match &c.value {
        Value::Word(w) => Ok(w),
        _ => Err(refuse(c, "a word")),
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
        assert_eq!(span(&c("h", Value::Number(3.0))), Ok((3.0, 3.0)));
        assert_eq!(span(&c("h", Value::Range(2.0, 4.0))), Ok((2.0, 4.0)));
        let e = span(&c("h", Value::Word("tall".into()))).unwrap_err();
        assert_eq!(e.control.as_deref(), Some("h"));
        assert!(number(&c("h", Value::Range(2.0, 4.0))).is_err());
    }

    #[test]
    fn counts_round() {
        assert_eq!(counts(&c("n", Value::Range(1.4, 2.6))), Ok((1, 3)));
    }
}
