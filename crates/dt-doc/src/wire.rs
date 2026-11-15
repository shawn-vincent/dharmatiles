//! The element schema and the language's word tables as they travel: the types of `dharmatiles elements --json`,
//! `GET /api/elements` and `GET /api/language`.  The program makes them from the schema tables (`schema`, `elements`)
//! and dt-lang's words; the preview's page and `cargo xtask docs` read them back as these types.  They live here, not
//! in dt-lang (which has no dependencies, so no serde), and carry no geometry, so xtask can read them cheaply.
//!
//! Every field is camelCase on the wire, like every other JSON the page reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One element in the list (`dharmatiles elements --json`) and the head of [`ElementJson`].
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementSummary {
    pub name: String,
    /// `surface`, `thing`, `run` or `scatter`.
    pub role: String,
    pub about: String,
    /// The element's page on the site.
    pub href: String,
    pub family: String,
    /// Whether this build builds it.
    pub built: bool,
}

/// An element with its controls, for the editor's lens, completion and the docs generator.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementJson {
    #[serde(flatten)]
    pub summary: ElementSummary,
    /// The place words it takes: `at`, `side`.
    pub place: Vec<String>,
    /// What `at=` may name.
    pub takes: Vec<String>,
    /// The place words it needs.
    pub needs: Vec<String>,
    pub holds_scatter: bool,
    /// Whether it is a body (fills a band over a region).
    pub body: bool,
    pub controls: Vec<ControlJson>,
    /// The weighting functions, for an element with a control that takes one; empty for the rest.
    pub weightings: Vec<WeightingJson>,
}

/// One control, the same shape for every element and node.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControlJson {
    pub name: String,
    /// What it measures; none for a group.
    pub unit: Option<String>,
    pub numeric: bool,
    /// `[min, max]`, max none for no high end.
    pub limits: Option<(f64, Option<f64>)>,
    /// `limits` refuses its minimum itself.
    pub above: bool,
    pub ranges: bool,
    pub weighted: bool,
    /// What it takes, in words.
    pub takes: Option<String>,
    pub default: Option<String>,
    /// The default as a format 1 document writes it (a length with its unit).
    pub default1: Option<String>,
    /// What leaving it out means, where there is no value to show for it.
    pub absent: Option<String>,
    pub required: bool,
    pub expert: bool,
    /// Whether this build takes it.
    pub taken: bool,
    pub about: String,
    pub words: Vec<WordJson>,
    pub subs: Vec<ControlJson>,
}

/// A word a control takes.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WordJson {
    pub word: String,
    pub about: String,
    /// The numbers the word sets, by control name.
    pub sets: BTreeMap<String, f64>,
    pub level: Option<String>,
}

/// One weighting function: how it is written and the numbers it takes.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeightingJson {
    pub name: String,
    pub call: String,
    pub about: String,
    pub args: Vec<WeightingArg>,
    /// The numbers it takes any number of.
    pub rest: Option<WeightingRest>,
}

/// A number after the range, with its default and limits.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeightingArg {
    pub name: String,
    pub default: Option<f64>,
    pub least: f64,
    /// `least` itself is refused.
    pub above: bool,
    /// None for no limit.
    pub most: Option<f64>,
}

/// The numbers a weighting takes any number of.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeightingRest {
    pub name: String,
    pub least: f64,
    pub at_least: usize,
    pub at_most: usize,
}

/// A node of the tile's own language (`tile`, `line`, `region`, `within`, `except`) with its controls.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSchema {
    pub name: String,
    pub about: String,
    pub href: String,
    pub controls: Vec<ControlJson>,
}

/// `GET /api/language`: the language's own tables, so the page keeps no copy of any.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageJson {
    /// The documentation site, with its trailing slash.
    pub docs: String,
    pub nodes: Vec<NodeSchema>,
    pub units: Vec<String>,
    /// What each unit word means, for the tooltip on a written unit.
    pub unit_about: BTreeMap<String, String>,
    pub functions: Vec<String>,
    pub weightings: Vec<String>,
    /// The `$` names.
    pub names: Vec<String>,
}

/// The edge contract's seams: what the tile-system page's table is made from.  A contract that does not read is
/// said in the answer, never left out.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SeamsJson {
    Read {
        seams: Vec<SeamRow>,
        corners: Vec<CornerRow>,
    },
    Failed {
        errors: Vec<String>,
    },
}

/// One seam: a position along a square's edge and the word on each side.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeamRow {
    pub at: f64,
    pub north: String,
    pub south: String,
    pub west: String,
    pub east: String,
}

/// A tile corner and its word.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CornerRow {
    pub corner: String,
    pub word: String,
}

/// `dharmatiles elements --json`: every element, and the seams.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementList {
    pub elements: Vec<ElementSummary>,
    pub seams: SeamsJson,
}
