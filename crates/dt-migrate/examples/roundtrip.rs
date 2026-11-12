//! Change, insert and remove one control in library files and print the
//! line diff.  `cargo run -p dt-migrate --example roundtrip`

#[path = "../tests/common/mod.rs"]
mod common;

use dt_doc::edit::{remove_control, set_control};

fn show(name: &str, what: &str, old: &str, new: &str) {
    println!("=== {name}: {what}");
    let (a, b): (Vec<_>, Vec<_>) = (old.lines().collect(), new.lines().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x == y => (i, j) = (i + 1, j + 1),
            (Some(x), Some(y)) if a.len() == b.len() => {
                println!("{:>3} - {x}\n{:>3} + {y}", i + 1, j + 1);
                (i, j) = (i + 1, j + 1);
            }
            (Some(x), _) => {
                println!("{:>3} - {x}", i + 1);
                i += 1;
            }
            (None, Some(y)) => {
                println!("{:>3} + {y}", j + 1);
                j += 1;
            }
            (None, None) => break,
        }
    }
    println!("    every other line is byte-identical\n");
}

fn main() {
    let (name, text) = common::GRASS_TREE;
    let tree = ["tile", "stack", "tree"];
    let new = set_control(text, &tree, "at", "1,1").expect("insert");
    show(name, "insert at=\"1,1\" on the tree", text, &new);
    let leaf = ["tile", "stack", "tree", "leaf"];
    let new = set_control(text, &leaf, "length", 5.5).expect("set");
    show(name, "leaf length 4.5 -> 5.5", text, &new);

    let (name, text) = common::WALL_CORNER;
    let wall = ["tile", "stack", "wall"];
    let new = set_control(text, &wall, "thickness", 6.5).expect("set");
    show(name, "thickness 7 -> 6.5 (child form)", text, &new);
    let new = remove_control(text, &wall, "coping").expect("remove");
    show(name, "remove coping", text, &new);
}
