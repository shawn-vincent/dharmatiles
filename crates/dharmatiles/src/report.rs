//! Report this defect: the filled-in new-issue form for a message that means the program went wrong.
//!
//! One producer for every surface (the editor's `GET /api/messages/report`, the command line's `report:` line).  The body is
//! the message's own copied text ([`Message::text`], the one formatter), then the environment, then the tile's source unless
//! it is left out.  A URL past [`URL_LIMIT`] carries a short body that says to paste the report, and the full report goes to
//! the clipboard (the page) or to a file (the command line).

use crate::message::{Kind, Message, Scope, Severity};
use crate::preview::fail::Panic;
use serde_json::json;

/// Where the issue is filed.
const ISSUES: &str = "https://github.com/shawn-vincent/dharmatiles/issues/new";
/// The longest URL put in a browser: past it the short body is used.
pub(crate) const URL_LIMIT: usize = 8000;

/// A report ready to file; also `GET /api/messages/report` on the wire.
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Report {
    pub url: String,
    pub title: String,
    pub labels: Vec<String>,
    /// The whole report: what the preview shows, and what goes to the clipboard when the URL is shortened.
    pub body: String,
    /// The URL carries a short body; the whole is in `body`.
    pub truncated: bool,
}

/// What the machine says about itself, for a report made where there is no page to say it (the command line).
pub(crate) fn environment() -> String {
    format!(
        "Environment\n  dharmatiles   {}\n  platform      {} {}",
        crate::cli::version_line(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url(title: &str, body: &str, labels: &[String]) -> String {
    format!(
        "{ISSUES}?title={}&labels={}&body={}",
        encode(title),
        encode(&labels.join(",")),
        encode(body)
    )
}

/// The report of `m`: `environment` as the surface says it, `source` the tile's document (`None` when left out or not known).
pub(crate) fn of(m: &Message, environment: &str, source: Option<&str>) -> Report {
    let first = m.text.lines().next().unwrap_or("").trim();
    let title = match &m.code {
        Some(c) => format!("{c}: {first}"),
        None => first.to_owned(),
    };
    let mut labels = vec!["defect".to_owned()];
    // the code's label is the code itself, when the registry has it: one source
    // (a gate message says `gate`, a diagnostic `dt::gate`: the word is what they share)
    if let Some(c) = m.code.as_deref().and_then(|code| {
        let word = dt_lang::codes::slug(code);
        dt_lang::codes::ALL
            .iter()
            .find(|c| dt_lang::codes::slug(c.id) == word)
    }) {
        labels.push(c.id.to_owned());
    }
    let mut body = format!("{}\n\n{}", m.text(), environment.trim_end());
    if let Some(src) = source {
        body.push_str(&format!("\n\nSource\n```kdl\n{}\n```", src.trim_end()));
    }
    let whole = url(&title, &body, &labels);
    if whole.len() <= URL_LIMIT {
        return Report {
            url: whole,
            title,
            labels,
            body,
            truncated: false,
        };
    }
    // the short version: what the message says, to paste the report under; shortened until the URL fits
    let lead =
        "Paste the report here (it is on your clipboard or in the file the program named).\n\n";
    let mut short = format!("{lead}{}", m.text());
    let mut u = url(&title, &short, &labels);
    while u.len() > URL_LIMIT {
        let keep = short.chars().count() * 3 / 4;
        short = short.chars().take(keep).collect::<String>();
        short.push_str("\n...");
        u = url(&title, &short, &labels);
    }
    Report {
        url: u,
        title,
        labels,
        body,
        truncated: true,
    }
}

/// The command line met a panic (a bug, never a mistake in a document): one more line, the issue's address.
pub(crate) fn tell_panic(p: &Panic) {
    let mut m = Message::new(
        Kind::Server,
        Severity::Error,
        Scope::Server,
        "",
        "",
        p.message.clone(),
    )
    .code("bug");
    m.data = json!({"origin": "cli", "kind": "bug", "causes": [format!("panicked at {}", p.location)], "backtrace": p.backtrace});
    let r = of(&m, &environment(), None);
    if r.truncated {
        let path =
            std::env::temp_dir().join(format!("dharmatiles-report-{}.txt", std::process::id()));
        match std::fs::write(&path, &r.body) {
            Ok(()) => eprintln!(
                "report: {}\nreport: the full report is in {}",
                r.url,
                path.display()
            ),
            Err(e) => eprintln!(
                "report: {}\nreport: the full report could not be written to {}: {e}",
                r.url,
                path.display()
            ),
        }
    } else {
        eprintln!("report: {}", r.url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bug() -> Message {
        Message::new(
            Kind::Server,
            Severity::Error,
            Scope::Server,
            "t1",
            "pool",
            "the kernel gave up\nsecond line",
        )
        .code(dt_lang::codes::GATE)
        .data(json!({"origin": "server", "kind": "bug", "causes": ["inner"], "backtrace": "a\nb"}))
    }

    fn decode(u: &str, key: &str) -> String {
        let q = u.split('?').nth(1).unwrap();
        let v = q
            .split('&')
            .find_map(|p| p.strip_prefix(&format!("{key}=")))
            .unwrap();
        let bytes = v.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_bug_makes_a_filled_in_issue() {
        let m = bug();
        let r = of(
            &m,
            "Environment\n  dharmatiles   1.2.3",
            Some("tile size=\"1x1\""),
        );
        assert!(r.url.starts_with(ISSUES));
        assert_eq!(
            decode(&r.url, "title"),
            format!("{}: the kernel gave up", dt_lang::codes::GATE)
        );
        assert_eq!(
            decode(&r.url, "labels"),
            format!("defect,{}", dt_lang::codes::GATE)
        );
        let body = decode(&r.url, "body");
        assert!(body.contains(&m.text()), "the message's own text: {body}");
        assert!(body.contains("dharmatiles   1.2.3"), "{body}");
        assert!(body.contains("tile size=\"1x1\""), "{body}");
        assert!(!r.truncated);
        let without = of(&m, "Environment", None);
        assert!(!decode(&without.url, "body").contains("Source"));
        assert!(!without.body.contains("Source"));
    }

    #[test]
    fn a_long_report_is_shortened_in_the_url_and_whole_in_the_body() {
        let big = "line of source\n".repeat(2000);
        let r = of(&bug(), "Environment", Some(&big));
        assert!(r.truncated);
        assert!(r.url.len() <= URL_LIMIT, "{}", r.url.len());
        assert!(decode(&r.url, "body").starts_with("Paste the report here"));
        assert!(r.body.contains(big.trim_end()));
    }

    #[test]
    fn only_the_programs_own_failures_are_reportable() {
        assert!(bug().reportable());
        let mistake = Message::new(Kind::Error, Severity::Error, Scope::Check, "t", "t", "x")
            .code(dt_lang::codes::TYPE);
        assert!(!mistake.reportable(), "a document mistake is the maker's");
        let gate = Message::new(Kind::Gate, Severity::Error, Scope::Build, "t", "t", "x");
        assert!(gate.reportable());
        let note = Message::new(Kind::Note, Severity::Note, Scope::Check, "t", "t", "x");
        assert!(!note.reportable());
    }
}
