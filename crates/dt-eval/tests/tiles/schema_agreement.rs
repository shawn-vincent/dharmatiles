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

/// The tree reads its branch knobs by name; each name is a control the
/// schema has, so a typo cannot fall back to a default unseen.
#[test]
fn the_names_the_tree_reads_are_in_the_schema() {
    let tree = dt_doc::schema::element("tree").unwrap();
    let branch = tree.control("branch").unwrap();
    let branching = tree.control("crown").unwrap().sub("branching").unwrap();
    for k in dt_elements::tree::KNOBS {
        let name = k.expert.strip_prefix("branch/").unwrap();
        assert!(branch.sub(name).is_some(), "{}", k.expert);
        let word = k.word.strip_prefix("crown/branching.").unwrap();
        assert!(
            branching
                .words
                .iter()
                .all(|w| w.sets.iter().any(|(n, _)| *n == word)),
            "{}",
            k.word
        );
    }
}

/// A word the wall height and the cliff stone size take stands for a
/// number the element reads: every word carries it.
#[test]
fn words_that_stand_for_a_number_carry_it() {
    let wall = dt_doc::schema::element("wall").unwrap();
    for w in wall.control("height").unwrap().words {
        assert!(w.sets.iter().any(|(n, _)| *n == "height"), "{}", w.word);
    }
    let cliff = dt_doc::schema::element("cliff").unwrap();
    for w in cliff.control("stoneSize").unwrap().words {
        assert!(w.sets.iter().any(|(n, _)| *n == "stoneSize"), "{}", w.word);
    }
}
