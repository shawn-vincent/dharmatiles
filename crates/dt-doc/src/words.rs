//! The language's own words, which a plain name (a `let`, a `param`) may not
//! take in format 1, and the `$` names an element sets while it builds.  The
//! tables are [`dt_lang`]'s; this holds the questions the reader asks of
//! them and of the element schema.

pub use dt_lang::{
    BODY_SETS, COORDINATES, POINT_NAMES, REGION_NAMES, RESERVED_POINT_NAMES, TREE_SETS, UNITS,
};

/// The things whose `top=` is a cloud worked out where the thing stands: a
/// tree (at each branch tip) and a rock, stone, boulder or pebble (at its
/// middle).
#[must_use]
pub fn takes_cloud_top(element: &str) -> bool {
    crate::schema::element(element).is_some_and(|e| e.cloud_top)
}

/// True when `element` is a body of the stack (dirt, water, a floor, a
/// cliff): it paints the surface things stand on and takes a `top=` and
/// pins of its own.
#[must_use]
pub fn is_body(element: &str) -> bool {
    crate::schema::element(element).is_some_and(|e| e.is_body())
}

/// The `$` names `element` sets when it calls a formula it was given (its
/// `top=` or a pin's): `check` refuses a read of any other that no caller
/// installed.  An element with no formula of its own still has a region.
#[must_use]
pub fn sets(element: &str) -> &'static [&'static str] {
    if is_body(element) {
        BODY_SETS
    } else if takes_cloud_top(element) {
        TREE_SETS
    } else {
        dt_lang::REGION_SETS
    }
}

/// True when `name` is a word of the language (a plain name may not be it).
#[must_use]
pub fn is_reserved(name: &str) -> bool {
    dt_lang::kind_of(name).is_some()
}

/// What kind of word it is, for a message.
#[must_use]
pub fn kind(name: &str) -> &'static str {
    dt_lang::kind_of(name).unwrap_or("a unit")
}

/// True when an element sets `name` itself while it builds (so a document
/// may read it but never `let` or `override` it).
#[must_use]
pub fn is_element_set(name: &str) -> bool {
    POINT_NAMES.contains(&name)
        || RESERVED_POINT_NAMES.contains(&name)
        || REGION_NAMES.contains(&name)
}
