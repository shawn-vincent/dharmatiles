//! KDL's own parse errors, rewritten to say what to write when the
//! mistake is one an author makes: KDL v1 keywords, and numbers with a
//! range, percent, size or unit glued on.

use miette::SourceSpan;

use crate::error::{DocError, Src};

const KNOWN: [&str; 3] = ["write #", "ranges, percents", "units are not"];

pub(crate) fn syntax_errors(src: &Src, text: &str, e: &kdl::KdlError) -> Vec<DocError> {
    let mut out: Vec<DocError> = Vec::new();
    for diag in &e.diagnostics {
        let from = diag.span.offset().min(text.len());
        let to = (from + diag.span.len().max(1)).min(text.len());
        let err = match suspicious_token(text, from, to) {
            Some((at, word)) if matches!(word, "true" | "false" | "null") => DocError::new(
                src,
                dt_lang::codes::SYNTAX,
                SourceSpan::new(at.into(), word.len()),
                format!("bare `{word}` is KDL v1; dharmatiles documents are KDL v2"),
                "KDL v1 keyword",
            )
            .maybe_help(Some(format!("write #{word}"))),
            Some((at, word)) if unit_suffix(word).is_some() => {
                let number = unit_suffix(word).unwrap_or(word);
                DocError::new(
                    src,
                    dt_lang::codes::SYNTAX,
                    SourceSpan::new(at.into(), word.len()),
                    format!("`{word}` has a unit written on it"),
                    "not a KDL number",
                )
                .maybe_help(Some(if text.lines().any(|l| l.trim_start().starts_with("dharmatiles version=1")) {
                    format!("a value with a unit is a quoted string: \"{word}\"")
                } else {
                    format!("units are not written: write {number} (every control knows its unit; sizes are dmm, the design millimetre), or (mm){number} as a check")
                }))
            }
            Some((at, word)) => DocError::new(
                src,
                dt_lang::codes::SYNTAX,
                SourceSpan::new(at.into(), word.len()),
                format!("`{word}` must be quoted"),
                "not a KDL number",
            )
            .maybe_help(Some(format!(
                "ranges, percents, points and sizes are written as strings: \"{word}\""
            ))),
            None => DocError::new(
                src,
                dt_lang::codes::SYNTAX,
                diag.span,
                diag.message
                    .clone()
                    .unwrap_or_else(|| "this is not valid KDL v2".to_owned()),
                diag.label.clone().unwrap_or_else(|| "here".to_owned()),
            )
            .maybe_help(diag.help.clone()),
        };
        // KDL often reports one mistake several times; keep it once.
        if !out
            .iter()
            .any(|o| o.span == err.span && o.message == err.message)
        {
            out.push(err);
        }
    }
    // An unclosed `{` is reported twice: at the `{` and again at the end
    // of the nodes; the first says it all.
    if out.iter().any(|e| e.message.starts_with("No closing '}'")) {
        out.retain(|e| !e.message.starts_with("Closing '}' was not found"));
    }
    // A recognised mistake explains the generic reports around it.
    let known = |e: &DocError| {
        e.help
            .as_deref()
            .is_some_and(|h| KNOWN.iter().any(|k| h.starts_with(k)))
    };
    if out.iter().any(known) {
        out.retain(known);
    }
    out
}

/// `4.5mm` → `Some("4.5")`: a plain number with letters glued on.
fn unit_suffix(word: &str) -> Option<&str> {
    let start = usize::from(word.starts_with('-'));
    let digits = word[start..]
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|i| i + start)?;
    let (num, unit) = word.split_at(digits);
    (!num.is_empty()
        && !num.contains("..")
        && !unit.is_empty()
        && unit.chars().all(|c| c.is_ascii_alphabetic()))
    .then_some(num)
}

/// A token in `text[from..to]` that is a known KDL v2 mistake: a bare v1
/// keyword, or a number with `..`, `%`, `,` or letters glued on.
fn suspicious_token(text: &str, from: usize, to: usize) -> Option<(usize, &str)> {
    let mut i = from;
    while i < to {
        let rest = &text[i..];
        let len = rest
            .find(|c: char| c.is_whitespace() || "{};=()\"".contains(c))
            .unwrap_or(rest.len());
        let word = &rest[..len];
        // Words in a `//` comment or inside a string are not the mistake.
        let line = text[..i].rfind('\n').map_or(0, |n| n + 1);
        let before = &text[line..i];
        if before.contains("//") || before.matches('"').count() % 2 == 1 {
            i += len.max(rest.chars().next().map_or(1, char::len_utf8));
            continue;
        }
        let numberish = word.starts_with(|c: char| c.is_ascii_digit())
            || (word.starts_with(['-', '.'])
                && word[1..].starts_with(|c: char| c.is_ascii_digit()));
        if matches!(word, "true" | "false" | "null")
            || (numberish
                && (word.contains("..")
                    || word.contains('%')
                    || word.contains(',')
                    || word.contains(|c: char| c.is_ascii_alphabetic())))
        {
            return Some((i, word));
        }
        i += len.max(rest.chars().next().map_or(1, char::len_utf8));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_suffixes() {
        assert_eq!(unit_suffix("4.5mm"), Some("4.5"));
        assert_eq!(unit_suffix("2x2"), None);
        assert_eq!(unit_suffix("1..6"), None);
        assert_eq!(unit_suffix("20%"), None);
    }
}
