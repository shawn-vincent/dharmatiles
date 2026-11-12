//! The page's side of `dharmatiles printers import`: what the installed
//! slicers have, and writing the chosen models into the user's `printers/`.

use serde_json::{Value, json};

use super::{Shared, fail};
use crate::printers as cli;
use crate::slicers::{self, Outcome};

fn bad(msg: &str) -> (&'static str, Value) {
    fail::refused("400 Bad Request", msg)
}

/// `/api/printers/scan` `{slicer?}`: without a slicer, the ones installed;
/// with one, every model it has and what could not be read.
pub(super) fn scan(v: &Value) -> (&'static str, Value) {
    let home = crate::preview::roots::personal_home().ok();
    let Some(word) = v["slicer"].as_str() else {
        return match cli::found() {
            Ok(f) => (
                "200 OK",
                json!({"slicers": f.iter().map(|i| json!({"slicer": i.slicer.word(), "label": i.label(), "title": i.slicer.title()})).collect::<Vec<_>>()}),
            ),
            Err(e) => fail::failure("500 Internal Server Error", e),
        };
    };
    match cli::install_of(word, None) {
        Ok(i) => (
            "200 OK",
            cli::scan_json(&slicers::scan(&i), home.as_deref()),
        ),
        Err(e) => bad(&e),
    }
}

/// `/api/printers/import` `{slicer, names: [..], force?}`: write the models
/// whose profile names are given into the user's printers folder, and tell
/// every page the new list.
pub(super) fn import(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let Some(word) = v["slicer"].as_str() else {
        return bad("give the slicer");
    };
    let Some(names) = v["names"].as_array() else {
        return bad("give the names of the models to import");
    };
    let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
    let install = match cli::install_of(word, None) {
        Ok(i) => i,
        Err(e) => return bad(&e),
    };
    let scan = slicers::scan(&install);
    let chosen: Vec<&slicers::Candidate> = scan
        .candidates
        .iter()
        .filter(|c| names.contains(&slicers::name_of(c).as_str()))
        .collect();
    if chosen.is_empty() {
        return bad("none of those models is in the slicer");
    }
    let home = match super::edit::home(shared) {
        Ok(h) => h,
        Err(e) => return e,
    };
    match slicers::write(
        &chosen,
        &install,
        &home,
        &cli::library_has,
        v["force"].as_bool() == Some(true),
    ) {
        Ok(done) => {
            shared.announce();
            (
                "200 OK",
                json!({"written": done.iter().map(|w| json!({"name": w.name, "kept": w.outcome == Outcome::Kept})).collect::<Vec<_>>()}),
            )
        }
        Err(e) => fail::failure("500 Internal Server Error", e),
    }
}
