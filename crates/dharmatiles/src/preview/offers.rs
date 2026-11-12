//! `/api/tiles/ID/offers` `{text, at, file?}`: what the editor's Add menu
//! may offer at place `at` of the stack (0 is the top, the stack's length is
//! the end): every built element with the line of KDL it would write, whether
//! the plan lets it stand there, and when it does not, why.
//!
//! The answer is the plan's own.  The stack is read and planned as written
//! ([`dt_eval::check`]); a default instance of the element is planned in the
//! place and [`dt_eval::plan::schedule`] says yes or names the refusal, in
//! the words the checker would use for it.  Nothing is written.

use dt_doc::elements::ELEMENTS;
use dt_elements::{Child, Plan};
use dt_eval::plan::{Refusal, schedule};
use serde_json::{Value, json};

use super::edit::{Overlay, bad};
use super::*;

/// A thing to offer: its name, what it writes into the stack and whether a
/// tile may hold more than one.
struct Candidate {
    name: &'static str,
    family: &'static str,
    about: &'static str,
    node: String,
    plan: Result<Plan, String>,
}

/// The element named `name` as a stack entry with nothing set, planned.
fn default_plan(name: &str) -> Result<Plan, String> {
    // a scatter is what its children are: one rock is the plainest
    let kids = if name == "scatter" {
        vec![Child {
            name: "rock".to_owned(),
            controls: Vec::new(),
            weight: 1.0,
        }]
    } else {
        Vec::new()
    };
    dt_elements::element_with(name, &[], &kids)
        .map(|e| e.plan())
        .map_err(|e| e.message)
}

/// The line of KDL that adds the element with nothing set but what it must
/// have: a wall runs along something, so along the first drawn line (else
/// the north edge); a scatter holds a thing and says how many; a second dirt
/// says the band it fills, which the first is given by the library's edge.
fn node_of(name: &str, lines: &[&String], has_dirt: bool) -> String {
    match name {
        "dirt" if has_dirt => "dirt top=\"$dirtEdgeHeight + 35dmm\"".to_owned(),
        "wall" => format!("wall at={}", lines.first().map_or("north", |l| l.as_str())),
        "scatter" => "scatter count=3 {\n    rock\n}".to_owned(),
        other => other.to_owned(),
    }
}

/// The candidates, in the order the schema lists the elements, the base last.
fn candidates(lines: &[&String], has_dirt: bool) -> Vec<Candidate> {
    let built: Vec<&str> = dt_elements::built().collect();
    ELEMENTS
        .iter()
        .filter(|e| built.contains(&e.name))
        .map(|e| Candidate {
            name: e.name,
            family: e.family.as_str(),
            about: e.about,
            node: node_of(e.name, lines, has_dirt),
            plan: default_plan(e.name),
        })
        .collect()
}

/// Why the plan refuses `with` (the stack with the candidate inserted at
/// `k`), in words that name the candidate "it" and any other entry by its
/// element; `names` are the elements of the stack as written.
fn why(r: &Refusal, k: usize, names: &[String]) -> String {
    let who = |i: usize| -> String {
        if i == k {
            "it".to_owned()
        } else {
            let was = if i > k { i - 1 } else { i };
            format!(
                "the `{}` listed there",
                names.get(was).map_or("base", String::as_str)
            )
        }
    };
    match r {
        Refusal::NoWriter { entry, kind } => format!(
            "{} reads every {kind}, and nothing in the stack makes one: add one above first",
            who(*entry)
        ),
        Refusal::SurfaceReadsSolid { entry } => {
            format!(
                "{} is a surface that reads solids, which cannot be yet",
                who(*entry)
            )
        }
        Refusal::Early { entry, kind, .. } => {
            let (what, fix) = match kind {
                Some(dt_elements::Kind::Terrain) => (
                    "stands on the ground, and the ground is painted after it here".to_owned(),
                    "list a `dirt` before it".to_owned(),
                ),
                Some(read) => (
                    format!("reads every {read}, and the first one is listed after it here"),
                    "list it after what it reads".to_owned(),
                ),
                None => (
                    "lies on the ground, and a surface would be painted after it".to_owned(),
                    "list it after every `dirt` it lies on".to_owned(),
                ),
            };
            format!("{} {what}: {fix}", who(*entry))
        }
    }
}

/// The offers at place `at`.
pub(super) fn offers(shared: &Shared, tile: &str, file: &str, v: &Value) -> (&'static str, Value) {
    let Some(text) = v["text"].as_str() else {
        return bad("give the text");
    };
    let Some(at) = v["at"].as_u64().and_then(|a| usize::try_from(a).ok()) else {
        return bad("give the place in the stack: 0 is the top");
    };
    let src = Overlay(BTreeMap::from([(file.to_owned(), text.to_owned())]));
    let opts = shared.options();
    let checked = match dt_eval::check(&src, tile, &opts) {
        Ok(c) => c,
        Err(errs) => {
            // a tile with a mistake has no plan to ask: the mistake is what to fix first
            let first = errs.first().map_or_else(String::new, |e| e.message.clone());
            return (
                "200 OK",
                json!({ "ok": false, "why": first, "errors": errors_json(&errs) }),
            );
        }
    };
    let Some(t) = &checked.tile else {
        return bad("this file has no tile");
    };
    let plans = checked.plans();
    // the entries of the stack, then the connector when the tile has a base
    let entries = t.stack.len();
    let k = at.min(entries);
    let names: Vec<String> = t.stack.iter().map(|e| e.what.value.clone()).collect();
    let lines: Vec<&String> = t.lines.keys().collect();
    let mut out = Vec::new();
    for c in candidates(&lines, names.iter().any(|n| n == "dirt")) {
        let verdict = match &c.plan {
            Err(e) => Err(format!("cannot be added with nothing set: {e}")),
            Ok(p) => {
                let mut with = plans.clone();
                with.insert(k, p.clone());
                schedule(&with).map(|_| ()).map_err(|r| why(&r, k, &names))
            }
        };
        out.push(json!({
            "name": c.name, "family": c.family, "about": c.about, "node": c.node,
            "ok": verdict.is_ok(), "why": verdict.err(),
        }));
    }
    // the base is the connector the build target picks; a tile has one
    out.push(json!({
        "name": "base", "family": "base",
        "about": "the connector under zero: the build target picks DungeonBlocks or OpenLOCK",
        "node": "base", "ok": !t.has_base,
        "why": t.has_base.then_some("the tile already has a base"),
    }));
    ("200 OK", json!({ "ok": true, "at": k, "offers": out }))
}
