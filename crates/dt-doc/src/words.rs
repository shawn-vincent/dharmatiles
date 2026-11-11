//! The language's own words, which a plain name (a `let`, a `param`) may not
//! take in format 1, and the `$` names an element sets while it builds.

/// Coordinates: where a point is, how far from the middle, the angle, the
/// region above.
pub const COORDINATES: &[&str] = &["x", "y", "d", "a", "parent"];

/// Functions.
pub const FUNCTIONS: &[&str] = &[
    "sin", "cos", "abs", "min", "max", "clamp", "smooth", "point", "polar",
];

/// Weightings.
pub const WEIGHTINGS: &[&str] = &[
    "flat", "low", "high", "bell", "peak", "ends", "steps", "mix",
];

/// Units.
pub const UNITS: &[&str] = &["sq", "mm", "in", "layer", "deg", "rad", "pi", "dmm"];

/// The `$` names an element sets at each point it works a height out:
/// where it is, what it stands on.  `check` knows these; no `let` or
/// `override` may take one.
pub const POINT_NAMES: &[&str] = &["$x", "$y", "$bottom"];

/// The `$` names an element will set in later steps of format 1
/// (`$d $a $region`): reserved now so no tile takes them.
pub const RESERVED_POINT_NAMES: &[&str] = &["$d", "$a", "$region"];

/// True when `name` is a word of the language (a plain name may not be it).
#[must_use]
pub fn is_reserved(name: &str) -> bool {
    [COORDINATES, FUNCTIONS, WEIGHTINGS, UNITS]
        .iter()
        .any(|l| l.contains(&name))
}

/// What kind of word it is, for a message.
#[must_use]
pub fn kind(name: &str) -> &'static str {
    if COORDINATES.contains(&name) {
        "a coordinate"
    } else if FUNCTIONS.contains(&name) {
        "a function"
    } else if WEIGHTINGS.contains(&name) {
        "a weighting"
    } else {
        "a unit"
    }
}

/// True when an element sets `name` itself while it builds (so a document
/// may read it but never `let` or `override` it).
#[must_use]
pub fn is_element_set(name: &str) -> bool {
    POINT_NAMES.contains(&name) || RESERVED_POINT_NAMES.contains(&name)
}
