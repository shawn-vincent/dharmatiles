//! The language's tables, in one place: what a document may name (functions,
//! weightings, units, coordinates, the `$` names) and the numbers every layer
//! has to agree on.  The decoder, the completion list, the docs generator and
//! the editor's page read these; none keeps a copy.
//!
//! Plain data and no dependencies, so the document reader (which holds no
//! geometry), the geometry kernels (which hold the odds behind each weighting
//! and check their table against this one), the elements and the editor's
//! server can all depend on it.

/// The land's steepest ease, degrees: what dirt's hills are made at, and what
/// the solver eases a land body at unless the entry says `slope=`.
pub const LAND_SLOPE_DEG: f64 = 24.0;

/// How far water settles over what is under it, mm at the reference square.
pub const WATER_SETTLE_MM: f64 = 9.0;

/// The functions of the heights language: name and argument count.
pub const FUNCTIONS: [(&str, usize); 7] = [
    ("sin", 1),
    ("cos", 1),
    ("abs", 1),
    ("min", 2),
    ("max", 2),
    ("clamp", 3),
    ("smooth", 1),
];

/// The coordinate and point functions of format 1, with how many values each
/// takes: `x(region, unit)`, `y`, `d`, `a`, `point(frame, x, y)` and
/// `polar(frame, d, a)`.
pub const COORD_CALLS: [(&str, usize); 6] = [
    ("x", 2),
    ("y", 2),
    ("d", 2),
    ("a", 2),
    ("point", 3),
    ("polar", 3),
];

/// The weighting functions written as calls, and how many numbers follow the
/// range (`steps` takes any number up to its cap, `mix` writes parts and
/// weights instead).  The odds themselves are dt-geom's, whose own table is
/// checked against this one by a test there.
pub const WEIGHTINGS: [(&str, usize); 8] = [
    ("flat", 0),
    ("low", 1),
    ("high", 1),
    ("bell", 2),
    ("peak", 2),
    ("ends", 2),
    ("steps", usize::MAX),
    ("mix", 0),
];

/// Coordinates: where a point is, how far from the middle, the angle, the
/// region above.
pub const COORDINATES: &[&str] = &["x", "y", "d", "a", "parent"];

/// Units written as words, in the order help lists them.
pub const UNITS: [&str; 8] = ["sq", "mm", "in", "dmm", "layer", "deg", "rad", "pi"];

/// The `$` names an element sets at each point it works a height out:
/// where it is in the tile (`$x`, `$y`, squares from the north-west corner,
/// plain numbers), how far in from the edge of its own region (`$d`) and how
/// far round it (`$a`), both fractions, and what it stands on (`$bottom`).
/// `check` knows these; no `let` or `override` may take one.
pub const POINT_NAMES: &[&str] = &["$x", "$y", "$d", "$a", "$bottom"];

/// `$` names reserved and not yet set by any element: none now.
pub const RESERVED_POINT_NAMES: &[&str] = &[];

/// The `$` names that are regions: `$tile`, the root, and `$region`, the
/// region of the thing being built (at a declaration, the enclosing one).
/// A region is its own kind: it goes to `parent()` and, in later steps, to
/// the coordinate functions, never to arithmetic.
pub const REGION_NAMES: &[&str] = &["$tile", "$region"];

/// What a body (dirt, water, floors, a cliff) sets at each node of its
/// region as it works its top and pins out.
pub const BODY_SETS: &[&str] = &["$x", "$y", "$d", "$a", "$bottom", "$region"];

/// What a tree sets at each branch tip, and a rock at its middle, as it works
/// its top out: the place, its place in the thing's own disc, the ground at
/// its foot and the disc.
pub const TREE_SETS: &[&str] = &["$x", "$y", "$d", "$a", "$bottom", "$region"];

/// What every element with a formula of its own sets at least: its region.
pub const REGION_SETS: &[&str] = &["$region"];

/// True when `name` is a function a value may call: a function of numbers
/// or a coordinate or point call.
#[must_use]
pub fn is_function(name: &str) -> bool {
    FUNCTIONS.iter().any(|(n, _)| *n == name) || COORD_CALLS.iter().any(|(n, _)| *n == name)
}

/// True when `name` is a weighting function.
#[must_use]
pub fn is_weighting(name: &str) -> bool {
    WEIGHTINGS.iter().any(|(n, _)| *n == name)
}

/// What kind of word `name` is in the language, for a message, if it is one:
/// a plain name (a `let`, a `param`) may not take any of them.
#[must_use]
pub fn kind_of(name: &str) -> Option<&'static str> {
    if COORDINATES.contains(&name) {
        Some("a coordinate")
    } else if is_function(name) {
        Some("a function")
    } else if is_weighting(name) {
        Some("a weighting")
    } else if UNITS.contains(&name) {
        Some("a unit")
    } else {
        None
    }
}

/// Every function word, for completion: the functions, the coordinate and
/// point calls and the coordinates (`parent` among them), each once.
#[must_use]
pub fn function_words() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for w in FUNCTIONS
        .iter()
        .map(|(n, _)| *n)
        .chain(
            COORD_CALLS
                .iter()
                .filter(|(n, _)| matches!(*n, "point" | "polar"))
                .map(|(n, _)| *n),
        )
        .chain(COORDINATES.iter().copied())
    {
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_has_one_kind() {
        for (n, _) in FUNCTIONS {
            assert_eq!(kind_of(n), Some("a function"));
        }
        assert_eq!(kind_of("parent"), Some("a coordinate"));
        assert_eq!(kind_of("low"), Some("a weighting"));
        assert_eq!(kind_of("dmm"), Some("a unit"));
        assert_eq!(kind_of("hillDirt"), None);
        assert_eq!(function_words().len(), 7 + 2 + 5);
    }
}
