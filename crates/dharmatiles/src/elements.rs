//! `dharmatiles elements [NAME]`: the elements a stack holds and their controls, from
//! dt-doc's schema tables, with what this build takes so far (the
//! schema's planned controls aside).

use std::fmt::Write as _;

use dt_doc::elements::ELEMENTS;
use dt_doc::schema::{self, Axis, Control, Element, Node, PlaceWord, Role, Word};
use dt_doc::wire::{
    ControlJson, CornerRow, ElementJson, ElementList, ElementSummary, LanguageJson, NodeSchema,
    SeamRow, SeamsJson, WeightingArg, WeightingJson, WeightingRest, WordJson,
};
use dt_eval::Diag;

use crate::{ERROR, OK, report};

/// The documentation site with its trailing slash.  It is derived from the repository address in `Cargo.toml`
/// ([`dt_lang::codes::site_base`]), the one place the address is written.
#[must_use]
pub fn docs() -> String {
    format!("{}/", dt_lang::codes::site_base())
}

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
fn seams_json() -> SeamsJson {
    let failed = |errors: Vec<String>| SeamsJson::Failed { errors };
    match dt_eval::doc::read(&dt_eval::Library, dt_eval::doc::EDGES) {
        Ok(r) => match &r.file.doc.seams {
            Some(table) => SeamsJson::Read {
                seams: table
                    .seams
                    .iter()
                    .map(|s| SeamRow {
                        at: s.at,
                        north: s.words[0].to_string(),
                        south: s.words[1].to_string(),
                        west: s.words[2].to_string(),
                        east: s.words[3].to_string(),
                    })
                    .collect(),
                corners: table
                    .corners
                    .iter()
                    .map(|(c, w)| CornerRow {
                        corner: c.as_str().to_owned(),
                        word: w.to_string(),
                    })
                    .collect(),
            },
            None => failed(vec!["the edge contract has no `seams` block".to_owned()]),
        },
        Err(es) => failed(es.iter().map(ToString::to_string).collect()),
    }
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
        Axis::Dmm => "dmm",
        Axis::Deg => "degrees",
        Axis::Share => "percent",
        Axis::Count => "count",
        Axis::PerSquare => "per square",
        Axis::Number => "number",
        Axis::Bool => "switch",
        Axis::Word => "word",
        Axis::Tag => "tag",
        Axis::Height => "height",
        Axis::Size => "size",
        Axis::Text => "text",
        Axis::Raw => "raw",
        Axis::Names => "several",
    }
}

/// The unit as the text table shows it: how a format 1 document writes it
/// (`6dmm`, `34deg`); the JSON keeps `unit`'s names for the editor.
fn unit_text(a: Axis) -> &'static str {
    match a {
        Axis::Deg => "deg",
        other => unit(other),
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

fn summary_json(l: &'static Element) -> ElementSummary {
    ElementSummary {
        name: l.name.to_owned(),
        role: role(l.role).to_owned(),
        about: l.about.to_owned(),
        href: format!("{}elements/{}.html", docs(), l.name),
        family: l.family.as_str().to_owned(),
        built: built(l).is_some(),
    }
}

/// Every element with its controls, for the editor's lens and completion.
pub(crate) fn all_json() -> Vec<ElementJson> {
    ELEMENTS.iter().map(element_json).collect()
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

fn element_json(l: &'static Element) -> ElementJson {
    let takes = built(l);
    ElementJson {
        summary: summary_json(l),
        place: strings(&l.place_words()),
        takes: strings(&l.place_kinds()),
        needs: l.needs.iter().map(|p| p.as_str().to_owned()).collect(),
        holds_scatter: l.holds_scatter,
        body: l.is_body(),
        controls: l
            .controls
            .iter()
            .map(|c| control_json(Taken::By(takes), c.name, c))
            .collect(),
        weightings: weightings_json(l),
    }
}

/// The weighting functions, for an element with a control that takes one;
/// empty for the rest.
fn weightings_json(l: &Element) -> Vec<WeightingJson> {
    if !takes_weighting(l.controls) {
        return Vec::new();
    }
    dt_kit::WEIGHTINGS.iter().map(weighting_json).collect()
}

/// One weighting function: how it is written, each number after the range
/// with its default and limits (`most` none for no limit; `above` refuses
/// `least` itself), and the numbers it takes any number of.
fn weighting_json(f: &dt_kit::WeightingFn) -> WeightingJson {
    WeightingJson {
        name: f.name.to_owned(),
        call: f.call.to_owned(),
        about: f.about.to_owned(),
        args: f
            .args
            .iter()
            .map(|a| WeightingArg {
                name: a.name.to_owned(),
                default: a.default,
                least: a.least,
                above: a.above,
                most: a.most.is_finite().then_some(a.most),
            })
            .collect(),
        rest: f.rest.map(|r| WeightingRest {
            name: r.name.to_owned(),
            least: r.least,
            at_least: r.at_least,
            at_most: r.at_most,
        }),
    }
}

fn takes_weighting(controls: &[Control]) -> bool {
    controls
        .iter()
        .any(|c| c.weighted || takes_weighting(c.subs))
}

/// Whether a control is built: by an element, or always (a node's).
#[derive(Clone, Copy)]
enum Taken {
    By(Option<&'static Element>),
    Always,
}

/// One control on the wire, the same shape for every element and node.  The element or node it belongs to
/// carries the `href` of its page.
fn control_json(takes: Taken, path: &str, c: &Control) -> ControlJson {
    let taken = match takes {
        Taken::Always => true,
        Taken::By(t) => taken(t, path, c),
    };
    let given = (!c.default.is_empty()).then_some(c.default);
    ControlJson {
        name: c.name.to_owned(),
        unit: c.value.map(|a| unit(a).to_owned()),
        numeric: c.value.is_some_and(Axis::is_numeric),
        // [min, max], max none for no high end; `above` refuses min itself; `takes` says it all in words
        limits: c
            .limits
            .map(|l| (l.min, l.max.is_finite().then_some(l.max))),
        above: c.limits.is_some_and(|l| l.above),
        ranges: c.ranges,
        weighted: c.weighted,
        takes: c.takes(),
        default: given.map(str::to_owned),
        // as a format 1 document writes it (a length with its unit)
        default1: given.map(|_| c.default_written()),
        // what leaving it out means, where there is no value to show
        absent: (!c.absent.is_empty()).then(|| c.absent.to_owned()),
        required: c.required,
        expert: c.expert,
        taken,
        about: c.about.to_owned(),
        words: c.words.iter().map(word_json).collect(),
        subs: c
            .subs
            .iter()
            .map(|s| control_json(takes, &format!("{path}/{}", s.name), s))
            .collect(),
    }
}

fn node_json(n: &'static Node) -> NodeSchema {
    NodeSchema {
        name: n.name.to_owned(),
        about: n.about.to_owned(),
        href: format!("{}{}", docs(), n.doc),
        controls: n
            .controls
            .iter()
            .map(|c| control_json(Taken::Always, c.name, c))
            .collect(),
    }
}

/// The language's own tables for the editor: the tile's nodes with their
/// controls, the units, the function words and the `$` names, so the page
/// keeps no copy of any.
pub(crate) fn language_json() -> LanguageJson {
    let square = |system| dt_kit::Frame::for_system(system, 1, 1).square_mm;
    let (db, ol) = (square(dt_lang::System::Db), square(dt_lang::System::Ol));
    LanguageJson {
        docs: docs(),
        nodes: schema_nodes().iter().map(node_json).collect(),
        units: strings(&dt_lang::UNITS),
        // what each unit word means, for the tooltip on a written unit
        unit_about: dt_lang::UNITS
            .iter()
            .filter_map(|u| dt_lang::unit_about(u, db, ol).map(|a| ((*u).to_owned(), a)))
            .collect(),
        functions: strings(&dt_lang::function_words()),
        weightings: dt_lang::WEIGHTINGS.iter().map(|w| w.0.to_owned()).collect(),
        names: strings(&[dt_lang::POINT_NAMES, dt_lang::REGION_NAMES].concat()),
    }
}

fn schema_nodes() -> &'static [Node] {
    dt_doc::elements::NODES
}

fn word_json(w: &Word) -> WordJson {
    WordJson {
        word: w.word.to_owned(),
        about: w.about.to_owned(),
        sets: w.sets.iter().map(|(k, n)| ((*k).to_owned(), *n)).collect(),
        level: w.level.map(str::to_owned),
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
        let spread = scatter.control("spread").expect("spread");
        assert!(taken(Some(scatter), "count", count));
        assert!(!taken(Some(scatter), "spread", spread));
    }
}
