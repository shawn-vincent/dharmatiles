//! Print the deliberate-mistake reports, as an author would see them.
//! `cargo run -p dt-migrate --example errors`

#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    for case in &common::CASES {
        println!(
            "=== {} ({}: `{}` -> `{}`)",
            case.title, case.tile.0, case.from, case.to
        );
        let (_, shown) = case.report();
        print!("{shown}");
        println!();
    }
}
