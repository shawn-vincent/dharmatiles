//! The gate's numbers as the program sends them in a message, and as the page reads them: one fixture
//! (`web/preview/tests/fixtures/gate-numbers.json`) that this test holds the program to and the page's test
//! (`web/preview/tests/messages.test.js`) reads the panel's rows from, so a field the program renames cannot be one the page
//! still reads by its old name (the messages panel showed "NaN mm²" for want of this).

use super::*;

const FIXTURE: &str = include_str!("../../../../../web/preview/tests/fixtures/gate-numbers.json");

#[test]
fn a_finished_builds_gate_message_carries_the_numbers_the_page_reads_under_the_names_it_reads() {
    let want: Value = serde_json::from_str(FIXTURE).unwrap();
    let record = serde_json::json!({
        "build_s": 1.0,
        "audit": {
            "triangles": 461_038, "watertight": true, "band_mm2": 1.25, "band_budget_mm2": 3.5,
            "overhang_mm2": 0.0, "overhang_budget_mm2": 3.5, "dropped": 0, "holes": 0, "voids_filled": 2, "failures": [],
        },
    });
    let said = super::messages::of_stl(&record, "id", "name");
    let gate = said
        .iter()
        .find(|m| m.json()["kind"] == "gate")
        .expect("a gate message");
    let data = gate.json()["data"].clone();
    let (got, want) = (data.as_object().unwrap(), want.as_object().unwrap());
    let mut names: Vec<_> = got.keys().collect();
    names.sort();
    let mut wanted: Vec<_> = want.keys().collect();
    wanted.sort();
    assert_eq!(
        names, wanted,
        "the program's gate numbers are not named as the fixture the page is tested with"
    );
    for (k, v) in want {
        // a count may be sent as 0 or 0.0: the same number
        let same = got[k] == *v || (got[k].is_number() && got[k].as_f64() == v.as_f64());
        assert!(
            same,
            "{k}: the program sends {}, the fixture has {v}",
            got[k]
        );
    }
}
