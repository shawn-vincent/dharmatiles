//! An element's controls are declared twice: in dt-doc's schema and in the
//! element itself. This keeps the two lists equal.

use std::collections::BTreeSet;

/// Controls the schema names that a built element does not take yet (the
/// evaluator sets them aside as not built). Shrink this as they land.
const NOT_BUILT_YET: &[(&str, &str)] = &[
    ("scatter", "clumps"),
    ("scatter", "mostly"),
    ("scatter", "spread"),
    ("rock", "wear"),
    ("stone", "wear"),
    ("boulder", "wear"),
    ("pebble", "wear"),
    ("cliff", "bank-height"),
    ("cliff", "bank-reach"),
    ("cliff", "boulders"),
    ("cliff", "grit"),
    ("cliff", "scree"),
];

#[test]
fn every_built_look_takes_what_the_schema_names() {
    for name in dt_elements::built() {
        let schema = dt_doc::schema::element(name)
            .unwrap_or_else(|| panic!("`{name}` is built but has no schema entry"));
        let from_schema: BTreeSet<&str> = schema.controls.iter().map(|c| c.name).collect();
        let from_element: BTreeSet<&str> = dt_elements::takes(name)
            .unwrap_or_else(|| panic!("`{name}` has no takes()"))
            .iter()
            // `bumps/down` is a word's number riding on `bumps`
            .map(|n| n.split('/').next().unwrap_or(n))
            .collect();
        let unbuilt: BTreeSet<&str> = NOT_BUILT_YET
            .iter()
            .filter(|(l, _)| *l == name)
            .map(|(_, c)| *c)
            .collect();
        let expected: BTreeSet<&str> = from_schema.difference(&unbuilt).copied().collect();
        assert_eq!(from_element, expected, "controls of `{name}` disagree");
    }
}

#[test]
fn every_schema_look_is_built_or_known_unbuilt() {
    let unbuilt: Vec<&str> = dt_doc::elements::ELEMENTS
        .iter()
        .map(|l| l.name)
        .filter(|n| dt_elements::takes(n).is_none())
        .collect();
    assert!(
        unbuilt.is_empty(),
        "schema elements with no build: {unbuilt:?}"
    );
}
