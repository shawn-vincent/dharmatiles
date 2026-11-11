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
/// (`$d $a`): reserved now so no tile takes them.
pub const RESERVED_POINT_NAMES: &[&str] = &["$d", "$a"];

/// The `$` names that are regions: `$tile`, the root, and `$region`, the
/// region of the thing being built (at a declaration, the enclosing one).
/// A region is its own kind: it goes to `parent()` and, in later steps, to
/// the coordinate functions, never to arithmetic.
pub const REGION_NAMES: &[&str] = &["$tile", "$region"];

/// The `$` names an element sets while it builds a height at each point:
/// what every element with a `top=` declares ([`sets`]).
pub const TOP_SETS: &[&str] = &["$x", "$y", "$bottom", "$region"];

/// The `$` names `element` sets when it calls a formula it was given (its
/// `top=` or a pin's): `check` refuses a read of any other that no caller
/// installed.  An element with no formula of its own still has a region.
#[must_use]
pub fn sets(element: &str) -> &'static [&'static str] {
    match element {
        "dirt" | "water" | "flagstones" | "planks" | "cliff" | "tree" => TOP_SETS,
        _ => &["$region"],
    }
}

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
    POINT_NAMES.contains(&name)
        || RESERVED_POINT_NAMES.contains(&name)
        || REGION_NAMES.contains(&name)
}
