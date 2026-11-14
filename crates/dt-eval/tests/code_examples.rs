//! The registry's broken and fixed documents (`dt_lang::codes`) are what the error pages show, so each broken one
//! must give its code and each fixed one must read clean.  An example cannot drift from the program.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Options, check};

fn codes_of(text: &str) -> Vec<&'static str> {
    let files = common::memory(&[("t.dtile", text)]);
    match check(&files, "t.dtile", &Options::default()) {
        Ok(_) => Vec::new(),
        Err(errs) => errs.iter().map(|e| e.code).collect(),
    }
}

#[test]
fn every_example_breaks_as_it_says_and_mends_as_it_says() {
    let mut wrong = Vec::new();
    let mut shown = 0;
    for c in dt_lang::codes::ALL {
        let Some(ex) = c.example else { continue };
        shown += 1;
        let broken = codes_of(ex.broken);
        if !broken.contains(&c.id) {
            wrong.push(format!(
                "{}: the broken example gives {broken:?}\n{}",
                c.id, ex.broken
            ));
        }
        let fixed = codes_of(ex.fixed);
        if !fixed.is_empty() {
            wrong.push(format!(
                "{}: the fixed example still gives {fixed:?}\n{}",
                c.id, ex.fixed
            ));
        }
    }
    assert!(shown > 10, "only {shown} examples");
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}
