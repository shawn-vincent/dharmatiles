//! Reading a control's value: one set of checks, worded the same way for
//! every element.  Each refusal names the control and says what may be
//! written; a caller with better help for its own control (a named level,
//! a word that stands for numbers) replaces the help with
//! [`LookError::with_help`].

use crate::{Control, LookError, Value};

fn refuse(c: &Control, what: &str, help: String) -> LookError {
    LookError::control(
        &c.name,
        format!("`{}` must be {what}, not {}", c.name, c.value),
        help,
    )
}

/// A number from `lo` to `hi`.
pub(crate) fn number(c: &Control, lo: f64, hi: f64) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() && (lo..=hi).contains(&v) => Ok(v),
        _ => Err(refuse(
            c,
            &format!("{lo} to {hi}"),
            format!("write a number from {lo} to {hi}"),
        )),
    }
}

/// Any finite number.
pub(crate) fn any_number(c: &Control) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(v),
        _ => Err(refuse(c, "a number", "write a number".into())),
    }
}

/// A finite number over 0.
pub(crate) fn positive(c: &Control) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(refuse(c, "over 0", "write a number over 0".into())),
    }
}

/// A number from `lo` to `hi`, or a range within them (low end first), as
/// `(low, high)`.
pub(crate) fn span(c: &Control, lo: f64, hi: f64) -> Result<(f64, f64), LookError> {
    let ok = |v: f64| v.is_finite() && (lo..=hi).contains(&v);
    match c.value {
        Value::Number(v) if ok(v) => Ok((v, v)),
        Value::Range(a, b) if ok(a) && ok(b) && a <= b => Ok((a, b)),
        _ => Err(refuse(
            c,
            &format!("{lo} to {hi}, or a range within it"),
            format!(
                "write a number from {lo} to {hi}, or a range such as {}=\"{lo}..{hi}\"",
                c.name
            ),
        )),
    }
}

/// A number over 0, or a range over 0 (low end first), as `(low, high)`.
pub(crate) fn positive_span(c: &Control) -> Result<(f64, f64), LookError> {
    let ok = |v: f64| v.is_finite() && v > 0.0;
    match c.value {
        Value::Number(v) if ok(v) => Ok((v, v)),
        Value::Range(a, b) if ok(a) && ok(b) && a <= b => Ok((a, b)),
        _ => Err(refuse(
            c,
            "over 0, or a range over 0",
            format!(
                "write a number over 0, or a range such as {}=\"2..3\"",
                c.name
            ),
        )),
    }
}

/// A whole count from `lo` to `hi`, or a range of them, as `(low, high)`
/// rounded.
pub(crate) fn counts(c: &Control, lo: i64, hi: i64) -> Result<(i64, i64), LookError> {
    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    let (a, b) = span(c, lo as f64, hi as f64).map_err(|_| {
        refuse(
            c,
            &format!("{lo} to {hi}, or a range of them"),
            format!(
                "write a count, or a range such as {}=\"{lo}..{hi}\"",
                c.name
            ),
        )
    })?;
    #[allow(clippy::cast_possible_truncation, reason = "within lo..hi")]
    let r = |v: f64| libm::round(v) as i64;
    Ok((r(a), r(b)))
}

/// A share from `lo` to `hi` (written as a percent, read as a fraction).
pub(crate) fn share(c: &Control, lo: f64, hi: f64) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() && (lo..=hi).contains(&v) => Ok(v),
        ref v => {
            let shown = match v {
                Value::Number(n) if n.is_finite() => format!("{}%", n * 100.0),
                other => other.to_string(),
            };
            Err(LookError::control(
                &c.name,
                format!(
                    "`{}` must be {}% to {}%, not {shown}",
                    c.name,
                    lo * 100.0,
                    hi * 100.0
                ),
                format!(
                    "write a percent, such as {}=\"{}%\"",
                    c.name,
                    50.0 * (lo + hi)
                ),
            ))
        }
    }
}

/// One of `words`.
pub(crate) fn word<'a>(c: &'a Control, words: &[&str]) -> Result<&'a str, LookError> {
    match &c.value {
        Value::Word(w) if words.contains(&w.as_str()) => Ok(w),
        _ => Err(refuse(
            c,
            "one of the choices",
            format!("write one of {}", words.join(", ")),
        )),
    }
}

impl LookError {
    /// The same refusal with `help` instead.
    pub(crate) fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
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
    fn spans_take_a_number_or_an_ordered_range() {
        assert_eq!(span(&c("h", Value::Number(3.0)), 1.0, 5.0), Ok((3.0, 3.0)));
        assert_eq!(
            span(&c("h", Value::Range(2.0, 4.0)), 1.0, 5.0),
            Ok((2.0, 4.0))
        );
        for bad in [
            Value::Range(4.0, 2.0),
            Value::Range(0.0, 2.0),
            Value::Number(f64::NAN),
        ] {
            let e = span(&c("h", bad), 1.0, 5.0).unwrap_err();
            assert_eq!(e.control.as_deref(), Some("h"));
            assert!(e.message.starts_with("`h` must be 1 to 5"), "{}", e.message);
        }
    }

    #[test]
    fn counts_round_and_shares_show_percents() {
        assert_eq!(counts(&c("n", Value::Range(1.4, 2.6)), 0, 30), Ok((1, 3)));
        let e = share(&c("cover", Value::Number(1.5)), 0.0, 1.0).unwrap_err();
        assert_eq!(e.message, "`cover` must be 0% to 100%, not 150%");
    }
}
