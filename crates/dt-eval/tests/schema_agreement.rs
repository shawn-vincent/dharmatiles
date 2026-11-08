//! dt-doc's schema says what each element is and takes; dt-elements only
//! builds them.  This keeps the two sets of names equal.

#[test]
fn every_schema_element_is_built_and_every_built_one_is_in_the_schema() {
    let schema: Vec<&str> = dt_doc::elements::ELEMENTS.iter().map(|l| l.name).collect();
    let built: Vec<&str> = dt_elements::built().collect();
    for name in &schema {
        assert!(
            built.contains(name),
            "`{name}` is in the schema but not built"
        );
    }
    for name in &built {
        assert!(
            schema.contains(name),
            "`{name}` is built but not in the schema"
        );
    }
}
