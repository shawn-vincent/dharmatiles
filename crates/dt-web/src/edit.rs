//! The browser engine's text edit: the site's cards change a tile's source only through this call, which is the program's own
//! `dt_service::edit::apply` (the editor's `/api/edit` is the same function).  A card sends a typed [`EditReq`]: an op naming a
//! node by its id (the ids a check lists in `nodes`) and the fingerprint of the text they were read at (`read`, a check's
//! `nodesRev`); it gets back the new text, the node to select and the fingerprint of the new text, or the refusal.  The page
//! never writes a quote or counts a byte.

use dt_lang::System;
use dt_service::edit::{EditReq, EditRes, Failure, text_rev};
use serde::Serialize;

use crate::{JsError, LIBRARY, MAIN, Overlay, options};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// What an edit made: the program's [`EditRes`] and the fingerprint of its text, which the next edit of a run (a slider
/// dragged) carries as `read`.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Edited {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub made: EditRes,
    /// The revision of `made.text` (`EditReq::read` of an edit made to it).
    pub rev: String,
}

/// Make the edit `req` (the JSON of an [`EditReq`]) with the program's `dt_service::edit::apply`, reading the document as the page's
/// own check does (the library's heights and seams).  JSON ([`Edited`]).
///
/// # Errors
/// `req` is not an `EditReq`; the library was never set; or the edit was not made: it is incomplete, it is stale (the text
/// is not the one its ids were read from), or the program refuses it (a node the text does not have, or a mistake it would add).
/// The error is the program's words.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn edit_tile(req: &str, system: &str) -> Result<String, JsError> {
    let req: EditReq = serde_json::from_str(req)
        .map_err(|e| JsError::new(&format!("the edit is not an EditReq: {e}")))?;
    let system: System = system.parse().map_err(|e: String| JsError::new(&e))?;
    let text = match &req.op {
        dt_service::edit::EditOp::Set { text, .. }
        | dt_service::edit::EditOp::Insert { text, .. }
        | dt_service::edit::EditOp::Make { text, .. }
        | dt_service::edit::EditOp::Move { text, .. }
        | dt_service::edit::EditOp::Remove { text, .. }
        | dt_service::edit::EditOp::RemoveChildren { text, .. }
        | dt_service::edit::EditOp::Rename { text, .. }
        | dt_service::edit::EditOp::About { text, .. }
        | dt_service::edit::EditOp::Arg { text, .. }
        | dt_service::edit::EditOp::Args { text, .. }
        | dt_service::edit::EditOp::Point { text, .. }
        | dt_service::edit::EditOp::Hide { text, .. }
        | dt_service::edit::EditOp::Fix { text, .. } => text.as_str(),
    };
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        // the library as the page's check reads it, the document in place of the one the edit is made to
        let source = Overlay {
            library: &lib,
            text,
        };
        let opts = options(&source, system)?;
        let reader = dt_eval::Evaluated {
            base: &source,
            path: MAIN,
            opts: &opts,
        };
        let seams = || {
            dt_eval::doc::read(&source, dt_eval::doc::EDGES)
                .map_err(|ds| {
                    ds.iter()
                        .map(|d| d.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .and_then(|r| {
                    r.file
                        .doc
                        .seams
                        .clone()
                        .ok_or_else(|| "the edge contract has no `seams` block".to_owned())
                })
        };
        let made = dt_service::edit::apply(&reader, &req, &seams)
            .map_err(|f: Failure<String>| JsError::new(&f.to_string()))?;
        let rev = text_rev(&made.text);
        crate::json(&Edited { made, rev })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "tile size=\"1x1\" seed=97 {\n    line shore wobble=5 { through \"0,0.38\" \"1,0.42\" }\n    stack {\n    }\n}\n";

    fn set(text: &str, read: &str, value: i64) -> String {
        serde_json::json!({
            "read": read, "op": "set", "text": text, "id": "tile/line shore", "name": "wobble", "value": value
        })
        .to_string()
    }

    #[test]
    fn a_control_is_set_by_node_id_and_the_answer_carries_the_next_fingerprint() {
        crate::tests::lib();
        let first = edit_tile(&set(DOC, &text_rev(DOC), 8), "db").unwrap();
        let v: serde_json::Value = serde_json::from_str(&first).unwrap();
        let text = v["text"].as_str().unwrap();
        assert!(text.contains("wobble=8"), "{text}");
        // a run of edits: each carries the fingerprint the last answered
        let again = edit_tile(&set(text, v["rev"].as_str().unwrap(), 3), "db").unwrap();
        assert!(again.contains("wobble=3"), "{again}");
    }

    #[test]
    fn an_edit_of_a_text_the_ids_were_not_read_from_is_refused() {
        crate::tests::lib();
        let e = edit_tile(&set(DOC, "0000000000000000", 8), "db").unwrap_err();
        assert!(e.to_string().contains("text changed"), "{e}");
    }

    #[test]
    fn a_node_the_text_does_not_have_is_refused() {
        crate::tests::lib();
        let req = serde_json::json!({
            "read": text_rev(DOC), "op": "set", "text": DOC, "id": "tile/line nothere", "name": "wobble", "value": 1
        })
        .to_string();
        assert!(edit_tile(&req, "db").is_err());
    }
}
