//! The element schema and the language's tables as the editor's page reads them (`/api/elements`, `/api/language`):
//! built from dt-doc's schema, dt-lang's words and what this build takes (`dt_elements::built`), with no transport in
//! it.  The program's HTTP handler, its `elements` command and a web worker's `Service` all call these.

use dt_doc::elements::ELEMENTS;
use dt_doc::schema::{Axis, Control, Element, Node, Role, Word};
use dt_doc::wire::{
    ControlJson, CornerRow, ElementJson, ElementSummary, LanguageJson, NodeSchema, SeamRow,
    SeamsJson, WeightingArg, WeightingJson, WeightingRest, WordJson,
};

pub use crate::stack::docs;

pub fn seams_json() -> SeamsJson {
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

pub fn role(r: Role) -> &'static str {
    match r {
        Role::Surface => "surface",
        Role::Thing => "thing",
        Role::Run => "run",
        Role::Scatter => "scatter",
    }
}

/// The unit a value is written in, as a reader says it.
pub fn unit(a: Axis) -> &'static str {
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

/// Whether this build takes `path` (`bumps`, or `bumps/up` for a
/// sub-control).  A group counts as taken when any of its subs is.
pub fn taken(built: Option<&'static Element>, path: &str, c: &Control) -> bool {
    built.is_some_and(|l| {
        l.takes(path)
            || (c.is_group()
                && c.subs
                    .iter()
                    .any(|s| l.takes(&format!("{path}/{}", s.name))))
    })
}

/// The element, if this build builds it.
pub fn built(l: &'static Element) -> Option<&'static Element> {
    dt_elements::built().any(|n| n == l.name).then_some(l)
}

pub fn summary_json(l: &'static Element) -> ElementSummary {
    ElementSummary {
        name: l.name.to_owned(),
        role: role(l.role).to_owned(),
        generator: l.generator().word().to_owned(),
        about: l.about.to_owned(),
        href: format!("{}elements/{}.html", docs(), l.name),
        family: l.family.as_str().to_owned(),
        built: built(l).is_some(),
    }
}

/// Every element with its controls, for the editor's lens and completion.
pub fn all_json() -> Vec<ElementJson> {
    ELEMENTS.iter().map(element_json).collect()
}

pub fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

pub fn element_json(l: &'static Element) -> ElementJson {
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
pub fn weightings_json(l: &Element) -> Vec<WeightingJson> {
    if !takes_weighting(l.controls) {
        return Vec::new();
    }
    dt_kit::WEIGHTINGS.iter().map(weighting_json).collect()
}

/// One weighting function: how it is written, each number after the range
/// with its default and limits (`most` none for no limit; `above` refuses
/// `least` itself), and the numbers it takes any number of.
pub fn weighting_json(f: &dt_kit::WeightingFn) -> WeightingJson {
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

pub fn takes_weighting(controls: &[Control]) -> bool {
    controls
        .iter()
        .any(|c| c.weighted || takes_weighting(c.subs))
}

/// Whether a control is built: by an element, or always (a node's).
#[derive(Clone, Copy)]
pub enum Taken {
    By(Option<&'static Element>),
    Always,
}

/// One control on the wire, the same shape for every element and node.  The element or node it belongs to
/// carries the `href` of its page.
pub fn control_json(takes: Taken, path: &str, c: &Control) -> ControlJson {
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

pub fn node_json(n: &'static Node) -> NodeSchema {
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
pub fn language_json() -> LanguageJson {
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

pub fn schema_nodes() -> &'static [Node] {
    dt_doc::elements::NODES
}

pub fn word_json(w: &Word) -> WordJson {
    WordJson {
        word: w.word.to_owned(),
        about: w.about.to_owned(),
        sets: w.sets.iter().map(|(k, n)| ((*k).to_owned(), *n)).collect(),
        level: w.level.map(str::to_owned),
    }
}
