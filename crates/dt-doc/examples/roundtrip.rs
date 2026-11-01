//! Edit one control in each library file and print the line diff.
//! `cargo run -p dt-doc --example roundtrip`

#[path = "../tests/common/mod.rs"]
mod common;

use dt_doc::edit::{Key, set_value};

fn main() {
    let edits: [(&str, &str, &[&str], Key<'_>, kdl::KdlValue); 3] = [
        (
            common::GRASS.0,
            common::GRASS.1,
            &["define long-grass", "stack", "lawn"],
            Key::Prop("length"),
            "9..15".into(),
        ),
        (
            common::GRASS_TREE.0,
            common::GRASS_TREE.1,
            &["tile grass-tree", "stack", "tree", "leaf"],
            Key::Prop("length"),
            5.5.into(),
        ),
        (
            common::CLIFF_RUN.0,
            common::CLIFF_RUN.1,
            &["tile cliff-run", "stack", "cliff", "height"],
            Key::Arg(0),
            1.2.into(),
        ),
    ];
    for (name, text, path, key, value) in edits {
        let new = set_value(text, path, key, value).expect("edit");
        println!("=== {name}: {} > {key:?}", path.join(" > "));
        for (i, (a, b)) in text.lines().zip(new.lines()).enumerate() {
            if a != b {
                println!("{:>3} - {a}\n{:>3} + {b}", i + 1, i + 1);
            }
        }
        let lines = text.lines().count();
        println!("    the other {} lines are byte-identical\n", lines - 1);
    }
}
