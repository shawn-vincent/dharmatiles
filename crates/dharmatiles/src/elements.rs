//! `dharmatiles elements [NAME]`: the elements a stack holds and their controls, from
//! dt-doc's schema tables, with what this build takes so far (the
//! schema's planned controls aside).

use std::fmt::Write as _;

use dt_doc::elements::ELEMENTS;
use dt_doc::schema::{self, Axis, Control, Element, PlaceWord, Word};
use dt_doc::wire::ElementList;
use dt_eval::Diag;

use crate::{ERROR, OK, report};

use dt_service::elements_json::{
    built, element_json, role, seams_json, summary_json, taken, takes_weighting, unit,
};

/// List every element, or one element's controls.
pub fn run(name: Option<&str>, json: bool) -> u8 {
    let Some(name) = name else {
        if json {
            say!(
                "{}",
                to_json(&ElementList {
                    elements: ELEMENTS.iter().map(summary_json).collect(),
                    seams: seams_json(),
                })
            );
        } else {
            list();
        }
        return OK;
    };
    let Some(element) = schema::element(name) else {
        // a word typed on the command line: no document text to mend, so the words without a fix
        let hint = schema::did_you_mean(name, ELEMENTS.iter().map(|l| l.name)).map_or_else(
            || format!("the elements: {}", names().join(", ")),
            dt_doc::error::did_you_mean_help,
        );
        let e = Diag::plain(
            dt_lang::codes::UNKNOWN_ELEMENT,
            format!("no element is called `{name}`"),
        )
        .help(hint);
        if json {
            crate::say_json(&crate::cli_wire::Refused::of(dt_eval::mistakes(&[e])));
        } else {
            report(&[e]);
        }
        return ERROR;
    };
    if json {
        say!("{}", to_json(&element_json(element)));
    } else {
        show(element);
    }
    OK
}

/// The seams of the edge contract, from the library on disk: what the
/// tile-system page's table is made from. A contract that does not read is
/// reported in the answer, never left out.
fn names() -> Vec<&'static str> {
    ELEMENTS.iter().map(|l| l.name).collect()
}

/// The unit as the text table shows it: how a format 1 document writes it
/// (`6dmm`, `34deg`); the JSON keeps `unit`'s names for the editor.
fn unit_text(a: Axis) -> &'static str {
    match a {
        Axis::Deg => "deg",
        other => unit(other),
    }
}

/// A type as the JSON text the command prints.
fn to_json<T: serde::Serialize>(t: &T) -> String {
    serde_json::to_string(t).expect("the element schema is plain data")
}

fn list() {
    let wn = ELEMENTS.iter().map(|l| l.name.len()).max().unwrap_or(0);
    for l in ELEMENTS {
        let built = if built(l).is_some() {
            "built"
        } else {
            "not yet"
        };
        say!(
            "{:wn$}  {:7}  {:7}  {}",
            l.name,
            role(l.role),
            built,
            l.about
        );
    }
    say!("\n`dharmatiles elements NAME` lists an element's controls.");
}

/// One row of the controls table.
struct Row {
    name: String,
    kind: String,
    default: String,
    about: String,
    words: &'static [Word],
}

fn rows(
    takes: Option<&'static Element>,
    path: &str,
    c: &'static Control,
    depth: usize,
    out: &mut Vec<Row>,
) {
    let mut kind = c
        .value
        .map_or_else(|| "group".to_owned(), |a| unit_text(a).to_owned());
    if let Some(t) = c.takes() {
        let _ = write!(kind, ", {t}");
    }
    let mut flags = Vec::new();
    if c.required {
        flags.push("required");
    }
    if c.expert {
        flags.push("expert");
    }
    if takes.is_some() && !taken(takes, path, c) {
        flags.push("not built yet");
    }
    let mut about = c.about.to_owned();
    if !flags.is_empty() {
        let _ = write!(about, " [{}]", flags.join(", "));
    }
    let indent = "  ".repeat(depth);
    let eq = if depth > 0 { "=" } else { "" };
    out.push(Row {
        name: format!("{indent}{}{eq}", c.name),
        kind,
        default: c.default_written(),
        about,
        words: c.words,
    });
    for s in c.subs {
        rows(takes, &format!("{path}/{}", s.name), s, depth + 1, out);
    }
}

fn show(l: &'static Element) {
    let takes = built(l);
    say!("{}: {}", l.name, l.about);
    let mut facts = vec![role(l.role).to_owned()];
    if !l.place.is_empty() {
        let takes = l.place_kinds();
        let mut place = Vec::new();
        if !takes.is_empty() {
            place.push(format!("at= {}", takes.join(", ")));
        }
        if l.place.contains(&PlaceWord::Side) {
            place.push("side=".to_owned());
        }
        facts.push(format!("place {}", place.join(", ")));
    }
    if !l.needs.is_empty() {
        let mut needs: Vec<String> = l.needs.iter().map(|p| format!("{}=", p.as_str())).collect();
        needs.dedup();
        facts.push(format!("needs {}", needs.join(" ")));
    }
    if l.holds_scatter {
        facts.push("holds a scatter".to_owned());
    }
    facts.push(match takes {
        Some(_) => "built".to_owned(),
        None => "not built yet".to_owned(),
    });
    say!("{}", facts.join(" · "));
    if l.controls.is_empty() {
        return;
    }
    let mut table = Vec::new();
    for c in l.controls {
        rows(takes, c.name, c, 0, &mut table);
    }
    let wn = table
        .iter()
        .map(|r| r.name.chars().count())
        .fold(7, usize::max);
    let wk = table
        .iter()
        .map(|r| r.kind.chars().count())
        .fold(4, usize::max);
    let wd = table
        .iter()
        .map(|r| r.default.chars().count())
        .fold(7, usize::max);
    say!("");
    say!(
        "  {:wn$}  {:wk$}  {:wd$}  about",
        "control",
        "unit",
        "default"
    );
    for r in &table {
        let line = format!(
            "  {:wn$}  {:wk$}  {:wd$}  {}",
            r.name, r.kind, r.default, r.about
        );
        say!("{}", line.trim_end());
        let ww = r.words.iter().map(|w| w.word.len()).max().unwrap_or(0);
        let indent = " ".repeat(r.name.len() - r.name.trim_start().len() + 4);
        for w in r.words {
            let mut line = format!("  {indent}{:ww$}  {}", w.word, w.about);
            let sets: Vec<String> = w.sets.iter().map(|(k, n)| format!("{k}={n}")).collect();
            if !sets.is_empty() {
                let _ = write!(line, " · {}", sets.join(" "));
            }
            if let Some(level) = w.level {
                let _ = write!(line, " · the `{level}` level");
            }
            say!("{line}");
        }
    }
    if takes_weighting(l.controls) {
        say!("");
        say!(
            "A control that takes \"a weighted range\" takes odds, written round the range: size=\"low(1dmm .. 6dmm, 4)\""
        );
        let wn = dt_kit::WEIGHTINGS
            .iter()
            .map(|f| f.call.chars().count())
            .max()
            .unwrap_or(0);
        for f in dt_kit::WEIGHTINGS {
            say!("  {:wn$}  {}", f.call, f.about);
            for a in f.args {
                let least = if a.above {
                    format!("over {}", a.least)
                } else {
                    format!("from {}", a.least)
                };
                let most = if a.most.is_finite() {
                    format!(" to {}", a.most)
                } else {
                    String::new()
                };
                let default = a
                    .default
                    .map_or_else(String::new, |d| format!(", default {d}"));
                say!("  {:wn$}    {}: {least}{most}{default}", "", a.name);
            }
            if let Some(r) = f.rest {
                say!(
                    "  {:wn$}    {}s: {} to {}, each from {}",
                    "",
                    r.name,
                    r.at_least,
                    r.at_most,
                    r.least
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_planned_control_is_not_taken() {
        let scatter = schema::element("scatter").expect("scatter");
        let count = scatter.control("count").expect("count");
        let rock = schema::element("rock").expect("rock");
        let wear = rock.control("wear").expect("wear");
        assert!(taken(Some(scatter), "count", count));
        assert!(!taken(Some(rock), "wear", wear));
    }
}
