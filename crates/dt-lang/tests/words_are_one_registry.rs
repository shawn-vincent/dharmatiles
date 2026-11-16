//! The registry (`src/words.rs`) says each word once.  Its rows are well made, its constants are its rows, and the
//! tables of this crate that hold a word's other facts (a function's argument count, the units' order) name the
//! same words as the rows of their kind.

use std::collections::BTreeSet;

use dt_lang::words::{self, WordKind};
use dt_lang::{COORD_CALLS, COORDINATES, FUNCTIONS, UNITS, WEIGHTINGS};

#[test]
fn a_word_is_one_row_of_its_kind_with_a_meaning() {
    let mut seen = BTreeSet::new();
    for w in words::all() {
        assert!(
            !w.word.is_empty() && !w.word.contains(char::is_whitespace),
            "`{}` is not one word",
            w.word
        );
        assert!(
            w.meaning.len() >= 3 && !w.meaning.ends_with('.'),
            "`{}` has no meaning, or its meaning is not a short sentence without a full stop: `{}`",
            w.word,
            w.meaning
        );
        assert!(
            seen.insert((w.kind, w.word)),
            "`{}` is registered twice as a {:?}: one word, one meaning",
            w.word,
            w.kind
        );
    }
}

#[test]
fn every_constant_is_a_row() {
    let mut names = BTreeSet::new();
    for (name, word, kind) in words::NAMES {
        assert!(names.insert(*name), "the constant {name} is made twice");
        assert!(
            words::find(*kind, word).is_some(),
            "the constant {name} holds `{word}`, which is no row"
        );
    }
}

fn kind_words(kind: WordKind) -> BTreeSet<&'static str> {
    words::all()
        .filter(|w| w.kind == kind)
        .map(|w| w.word)
        .collect()
}

#[test]
fn the_expression_tables_name_the_registered_functions() {
    let table: BTreeSet<&str> = FUNCTIONS
        .iter()
        .map(|(n, _)| *n)
        .chain(COORD_CALLS.iter().map(|(n, _)| *n))
        .chain(WEIGHTINGS.iter().map(|(n, _)| *n))
        .chain(COORDINATES.iter().copied())
        .collect();
    assert_eq!(
        table,
        kind_words(WordKind::Function),
        "dt-lang's function, weighting and coordinate tables and the registry's functions are two lists: add the word to both (the registry says what it means)"
    );
}

#[test]
fn the_unit_table_names_the_registered_units() {
    let table: BTreeSet<&str> = UNITS.iter().copied().collect();
    assert_eq!(
        table,
        kind_words(WordKind::Unit),
        "dt-lang's UNITS and the registry's units are two lists: add the word to both"
    );
}
