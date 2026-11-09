//! `dharmatiles elements [NAME]`: the elements a stack holds and their controls, from
//! dt-doc's schema tables, with what this build takes so far (the
//! schema's planned controls aside).

use std::fmt::Write as _;

use dt_doc::elements::ELEMENTS;
use dt_doc::schema::{self, Axis, Control, Element, PlaceWord, Role, Word};
use dt_eval::Diag;
use serde_json::{Map, Value, json};

use crate::{ERROR, OK, errors_json, report};

/// List every element, or one element's controls.
pub fn run(name: Option<&str>, json: bool) -> u8 {
    let Some(name) = name else {
        if json {
            let elements: Vec<Value> = ELEMENTS.iter().map(summary_json).collect();
            say!("{}", json!({"ok": true, "elements": elements}));
        } else {
            list();
        }
        return OK;
    };
    let Some(element) = schema::element(name) else {
        let hint = schema::did_you_mean(name, ELEMENTS.iter().map(|l| l.name)).map_or_else(
            || format!("the elements: {}", names().join(", ")),
            |m| format!("did you mean `{m}`?"),
        );
        let e = Diag::plain(
            "dt::unknown-element",
            format!("no element is called `{name}`"),
        )
        .help(hint);
        if json {
            say!("{}", json!({"ok": false, "errors": errors_json(&[e])}));
        } else {
            report(&[e]);
        }
        return ERROR;
    };
    if json {
        say!("{}", element_json(element));
    } else {
        show(element);
    }
    OK
}

fn names() -> Vec<&'static str> {
    ELEMENTS.iter().map(|l| l.name).collect()
}

fn role(r: Role) -> &'static str {
    match r {
        Role::Surface => "surface",
        Role::Thing => "thing",
        Role::Run => "run",
        Role::Scatter => "scatter",
    }
}

/// The unit a value is written in, as a reader says it.
fn unit(a: Axis) -> &'static str {
    match a {
        Axis::Mm => "mm",
        Axis::Deg => "degrees",
        Axis::Share => "percent",
        Axis::Count => "count",
        Axis::PerSquare => "per square",
        Axis::Number => "number",
        Axis::Bool => "switch",
        Axis::Word => "word",
        Axis::Tag => "tag",
    }
}

/// Whether this build takes `path` (`bumps`, or `bumps/up` for a
/// sub-control).  A group counts as taken when any of its subs is.
fn taken(built: Option<&'static Element>, path: &str, c: &Control) -> bool {
    built.is_some_and(|l| {
        l.takes(path)
            || (c.is_group()
                && c.subs
                    .iter()
                    .any(|s| l.takes(&format!("{path}/{}", s.name))))
    })
}

/// The element, if this build builds it.
fn built(l: &'static Element) -> Option<&'static Element> {
    dt_elements::built().any(|n| n == l.name).then_some(l)
}

fn summary_json(l: &'static Element) -> Value {
    json!({
        "name": l.name,
        "role": role(l.role),
        "about": l.about,
        "built": built(l).is_some(),
    })
}

/// Every element with its controls, for the editor's lens and completion.
pub(crate) fn all_json() -> Value {
    Value::Array(ELEMENTS.iter().map(element_json).collect())
}

fn element_json(l: &'static Element) -> Value {
    let takes = built(l);
    let mut v = summary_json(l);
    v["ok"] = json!(true);
    v["place"] = json!(l.place_words());
    v["takes"] = json!(l.place_kinds());
    v["needs"] = json!(l.needs.iter().map(|p| p.as_str()).collect::<Vec<_>>());
    v["holds_scatter"] = json!(l.holds_scatter);
    v["controls"] = json!(
        l.controls
            .iter()
            .map(|c| control_json(takes, c.name, c))
            .collect::<Vec<_>>()
    );
    v["weightings"] = weightings_json(l);
    v
}

/// The weighting functions, for an element with a control that takes one;
/// empty for the rest.
fn weightings_json(l: &Element) -> Value {
    if !takes_weighting(l.controls) {
        return json!([]);
    }
    Value::Array(dt_kit::WEIGHTINGS.iter().map(weighting_json).collect())
}

/// One weighting function: how it is written, each number after the range
/// with its default and limits (`most` null for no limit; `above` refuses
/// `least` itself), and the numbers it takes any number of.
pub(crate) fn weighting_json(f: &dt_kit::WeightingFn) -> Value {
    let finite = |x: f64| x.is_finite().then_some(x);
    json!({
        "name": f.name,
        "call": f.call,
        "about": f.about,
        "args": f.args.iter().map(|a| json!({
            "name": a.name,
            "default": a.default,
            "least": a.least,
            "above": a.above,
            "most": finite(a.most),
        })).collect::<Vec<_>>(),
        "rest": f.rest.map(|r| json!({
            "name": r.name,
            "least": r.least,
            "atLeast": r.at_least,
            "atMost": r.at_most,
        })),
    })
}

fn takes_weighting(controls: &[Control]) -> bool {
    controls
        .iter()
        .any(|c| c.weighted || takes_weighting(c.subs))
}

fn control_json(takes: Option<&'static Element>, path: &str, c: &Control) -> Value {
    json!({
        "name": c.name,
        "unit": c.value.map(unit),
        // [min, max], max null for no high end; `above` refuses min itself;
        // `takes` says it all in words
        "limits": c.limits.map(|l| [Some(l.min), l.max.is_finite().then_some(l.max)]),
        "above": c.limits.is_some_and(|l| l.above),
        "ranges": c.ranges,
        "weighted": c.weighted,
        "takes": c.takes(),
        "default": (!c.default.is_empty()).then_some(c.default),
        "required": c.required,
        "expert": c.expert,
        "taken": taken(takes, path, c),
        "about": c.about,
        "words": c.words.iter().map(word_json).collect::<Vec<_>>(),
        "subs": c.subs.iter().map(|s| control_json(takes, &format!("{path}/{}", s.name), s)).collect::<Vec<_>>(),
    })
}

fn word_json(w: &Word) -> Value {
    let sets: Map<String, Value> = w
        .sets
        .iter()
        .map(|(k, n)| ((*k).to_owned(), json!(n)))
        .collect();
    json!({"word": w.word, "about": w.about, "sets": sets, "level": w.level})
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
        .map_or_else(|| "group".to_owned(), |a| unit(a).to_owned());
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
        default: c.default.to_owned(),
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
            "A control that takes \"a weighted range\" takes odds, written round the range: size=\"low(1..6, 4)\""
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
        let spread = scatter.control("spread").expect("spread");
        assert!(taken(Some(scatter), "count", count));
        assert!(!taken(Some(scatter), "spread", spread));
    }
}
