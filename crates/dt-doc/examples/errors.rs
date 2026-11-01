//! Print the deliberate-mistake reports, as an author would see them.
//! `cargo run -p dt-doc --example errors`

#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let known = common::known();
    for case in common::CASES {
        let (name, text) = case.tile;
        let broken = text.replacen(case.from, case.to, 1);
        println!(
            "=== {} ({name}: `{}` -> `{}`)",
            case.title, case.from, case.to
        );
        match dt_doc::decode(name, &broken, &known) {
            Ok(_) => println!("(decoded without error)"),
            Err(e) => print!("{}", common::render(&e)),
        }
        println!();
    }
}
