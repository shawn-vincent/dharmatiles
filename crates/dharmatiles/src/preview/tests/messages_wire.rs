//! The gate's numbers as the program sends them in a message, and as the page reads them: one fixture
//! (`web/preview/tests/fixtures/gate-numbers.json`) that this test holds the program to and the page's test
//! (`web/preview/tests/messages.test.js`) reads the panel's rows from.  The numbers are one type, the engine's
//! `AuditSummary`, whose TypeScript type the page is checked against; the fixture is a value of it.

use super::*;

const FIXTURE: &str = include_str!("../../../../../web/preview/tests/fixtures/gate-numbers.json");

#[test]
fn a_finished_builds_gate_message_carries_the_audit_summary_the_page_is_tested_with() {
    let summary: dt_kit::AuditSummary =
        serde_json::from_str(FIXTURE).expect("the fixture is an AuditSummary");
    let record = dt_eval::TileRecord::of_stl("tile.stl", summary, 1.0);
    let said = super::messages::of_stl(&record, "id", "name");
    let gate = said
        .iter()
        .find(|m| m.json()["kind"] == "gate")
        .expect("a gate message");
    let want: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        gate.json()["data"],
        want,
        "a gate message's data is the summary"
    );
}
