//! The messages as the program sends them and as the page reads them: fixtures in `web/preview/tests/fixtures/` that this
//! test holds the program to and the page's test (`web/preview/tests/messages.test.js`) reads its rows from.
//!
//! - `gate-numbers.json` is the engine's `AuditSummary`, the numbers of a gate message, whose TypeScript type the page is
//!   checked against.
//! - `messages.json` is a list of whole messages, one for each sort of data a message carries, in the form the stream sends
//!   them: the program reads it as `MessageJson` and writes it again unchanged.

const FIXTURE: &str = include_str!("../../../../../web/preview/tests/fixtures/gate-numbers.json");
const MESSAGES: &str = include_str!("../../../../../web/preview/tests/fixtures/messages.json");
const RECORD: &str = include_str!("../../../../../web/preview/tests/fixtures/tile-record.json");

#[test]
fn a_finished_builds_gate_message_carries_the_audit_summary_the_page_is_tested_with() {
    let summary: dt_kit::AuditSummary =
        serde_json::from_str(FIXTURE).expect("the fixture is an AuditSummary");
    let record = dt_eval::TileRecord::of_stl("tile.stl", summary.clone(), 1.0);
    let said = super::messages::of_stl(&record, "id", "name");
    let gate = said
        .iter()
        .find(|m| m.kind == dt_service::message::Kind::Gate)
        .expect("a gate message");
    let Some(dt_service::message::MessageData::Gate(carried)) = gate.wire().data else {
        panic!("a gate message carries the audit summary");
    };
    assert_eq!(carried, summary, "a gate message's data is the summary");
}

#[test]
fn the_message_fixture_is_what_the_program_writes() {
    let messages: Vec<dt_service::message::MessageJson> =
        serde_json::from_str(MESSAGES).expect("the fixture is a list of messages");
    let written = serde_json::to_value(&messages).expect("messages write as JSON");
    let fixture: serde_json::Value = serde_json::from_str(MESSAGES).expect("the fixture is JSON");
    assert_eq!(
        written, fixture,
        "the fixture is the program's own form of its messages"
    );
    // the gate message of the fixture carries the gate fixture's numbers
    let summary: dt_kit::AuditSummary = serde_json::from_str(FIXTURE).unwrap();
    assert!(
        messages.iter().any(|m| matches!(
            &m.data,
            Some(dt_service::message::MessageData::Gate(g)) if *g == summary
        )),
        "a gate message with the gate fixture's numbers"
    );
}

/// `tile-record.json` is a whole build record (`dt_eval::TileRecord`, what the browser engine's `BuildMeta.record` and
/// the editor carry), which the site's tests build their messages from: it reads as the type and writes back the same,
/// and its audit is the gate fixture's.
#[test]
fn the_record_fixture_is_a_whole_build_record() {
    let record: dt_eval::TileRecord =
        serde_json::from_str(RECORD).expect("the fixture is a TileRecord");
    let written = serde_json::to_value(&record).expect("a record writes");
    let fixture: serde_json::Value = serde_json::from_str(RECORD).expect("the fixture is JSON");
    assert_eq!(
        written, fixture,
        "the fixture is the program's own form of a record"
    );
    let gate: dt_kit::AuditSummary = serde_json::from_str(FIXTURE).expect("the gate fixture");
    assert_eq!(
        serde_json::to_value(&record.audit).expect("an audit writes"),
        serde_json::to_value(&gate).expect("an audit writes"),
        "the record's audit is the gate fixture's"
    );
}
