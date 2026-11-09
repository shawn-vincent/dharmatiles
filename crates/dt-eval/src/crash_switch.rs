//! The crash switch for the exit-code tests: `DT_TEST_PANIC` makes the
//! program panic so the tests can check exit code 101 and the panic
//! boundaries.  It exists only when the `test-panic` feature is on, which only
//! the dharmatiles crate's dev-dependencies enable; in every other build both
//! functions do nothing and the program reads no such variable.

/// `DT_TEST_PANIC=main` panics here: a bug reaching `main`.
pub fn panic_in_main() {
    #[cfg(feature = "test-panic")]
    if std::env::var("DT_TEST_PANIC").as_deref() == Ok("main") {
        panic!("DT_TEST_PANIC=main");
    }
}

/// Any other `DT_TEST_PANIC` value panics when that text is in the path `p`
/// of a tile being built: a bug caught in a library sweep.
pub fn panic_in_tile(p: &str) {
    #[cfg(feature = "test-panic")]
    if let Ok(t) = std::env::var("DT_TEST_PANIC")
        && p.contains(&t)
    {
        panic!("DT_TEST_PANIC in {p}");
    }
    #[cfg(not(feature = "test-panic"))]
    let _ = p;
}
