//! What the decoder says about a mistake is true: a message and its help describe the program as it is.

mod common;

use common::*;

fn tile(entry: &str) -> String {
    format!(
        "dharmatiles version=1\ntile size=\"1x1\" seed=1 {{\n    stack {{\n        {entry}\n    }}\n}}\n"
    )
}

/// The words of one failing document's report on one line.
fn report(text: &str) -> String {
    flat(&fails(text).1)
}

#[test]
fn sin_is_said_to_exist_where_heights_are_written() {
    let said = report(&tile("rock size=\"sin(30deg) * 5dmm\""));
    assert!(!said.contains("unknown function"), "{said}");
    assert!(
        said.contains("`sin` is not read in this control")
            && said.contains("function of heights")
            && said.contains("where a height is written")
            && said.contains("top="),
        "{said}"
    );
    // and where it is read, it reads
    snippet(&tile("tree top=\"$bottom + 20dmm * sin(30deg)\""));
    // a word that is no function at all still says so, with the weightings
    let tan = report(&tile("rock size=\"tan(30deg) * 5dmm\""));
    assert!(
        tan.contains("unknown function `tan`") && tan.contains("weightings are"),
        "{tan}"
    );
}

#[test]
fn a_tree_size_help_says_what_a_tree_takes_now() {
    let said = report(&tile("tree size=4"));
    assert!(said.contains("`tree` has no control `size`"), "{said}");
    assert!(
        said.contains("top=\"$bottom + 40dmm\"") && !said.contains("tree size=40"),
        "{said}"
    );
    // and what it names is accepted
    snippet(&tile("tree top=\"$bottom + 40dmm\""));
}

#[test]
fn a_bare_share_says_what_it_was_read_as() {
    let said = report(&tile("grass cover=50"));
    assert!(said.contains("0% to 100%, not 5000%"), "{said}");
    assert!(
        said.contains("a bare 50 is read as 5000%")
            && said.contains("cover=\"50%\"")
            && said.contains("cover=0.5"),
        "{said}"
    );
    snippet(&tile("grass cover=\"50%\""));
    snippet(&tile("grass cover=0.5"));
}

#[test]
fn defaults_seams_and_printer_are_refused_outside_their_file_kind() {
    let own = tile("dirt");
    let defaults = format!("{own}defaults {{\n}}\n");
    let said = report(&defaults);
    assert!(
        said.contains("`defaults` is not written in a tile")
            && said.contains("parts file")
            && said.contains(dt_lang::codes::UNKNOWN_NODE),
        "{said}"
    );
    let seams = format!("{own}seams {{\n}}\n");
    assert!(said_has(&seams, "`seams` is not written in a tile"));
    let printer = format!("{own}printer mine {{\n}}\n");
    assert!(said_has(&printer, "`printer` is not written in a tile"));
    // a parts file holds defaults and seams, a printer file holds its printer
    let part =
        "dharmatiles version=1\nlet lawn {\n    stack {\n        grass\n    }\n}\ndefaults {\n}\n";
    snippet(part);
    let part_printer = "dharmatiles version=1\nlet lawn {\n    stack {\n        grass\n    }\n}\nprinter mine {\n}\n";
    assert!(said_has(
        part_printer,
        "`printer` is not written in a parts file"
    ));
}

fn said_has(text: &str, what: &str) -> bool {
    let said = report(text);
    assert!(said.contains(what), "{said}");
    true
}
