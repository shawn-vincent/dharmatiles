//! The draft typed document model.
//!
//! One file is a [`Document`]: imports, file-level named heights and
//! definitions.  A tile is a definition with a root call attached
//! (`cols`, `rows`, `seed`).  Every name and value keeps its source span
//! so later passes (imports, evaluation) can still point at the text.

use miette::SourceSpan;

use crate::expr::Expr;
use crate::schema::Look;

/// A value with the source span it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    /// The value.
    pub value: T,
    /// Where it was written.
    pub span: SourceSpan,
}

/// One KDL file.
#[derive(Debug, Clone)]
pub struct Document {
    /// `use "lib:parts/grass"` lines, in order.
    pub uses: Vec<Spanned<String>>,
    /// A file-level `heights` table (library/parts/heights.kdl).
    pub heights: Vec<Height>,
    /// `define` and `tile` nodes, in order.
    pub definitions: Vec<Definition>,
}

/// Whether a definition is a tile (a root call) or a reusable part.
#[derive(Debug, Clone, PartialEq)]
pub enum DefKind {
    /// `define name { … }`
    Define,
    /// `tile name cols=… rows=… seed=… { … }`
    Tile {
        /// Squares across.
        cols: u32,
        /// Squares up the page.
        rows: u32,
        /// The root seed, if pinned.
        seed: Option<i64>,
    },
}

/// One definition: parameters with defaults, lines, regions and a stack.
#[derive(Debug, Clone)]
pub struct Definition {
    /// Its name.
    pub name: Spanned<String>,
    /// Tile or define.
    pub kind: DefKind,
    /// A description (`about "…"`), for library listings.
    pub about: Option<String>,
    /// `param` nodes.
    pub params: Vec<Param>,
    /// Named heights local to this definition.
    pub heights: Vec<Height>,
    /// Named lines.
    pub lines: Vec<Line>,
    /// Paint-bucket regions.
    pub regions: Vec<Region>,
    /// The stack, bottom to top.
    pub stack: Vec<Entry>,
    /// The whole node.
    pub span: SourceSpan,
}

/// `param name default`.  No default means the caller must give it.
#[derive(Debug, Clone)]
pub struct Param {
    /// Its name.
    pub name: Spanned<String>,
    /// Its default.
    pub default: Option<Spanned<Value>>,
}

/// A named height, in h.
#[derive(Debug, Clone)]
pub struct Height {
    /// Its name.
    pub name: Spanned<String>,
    /// Its value.
    pub value: Spanned<Value>,
}

/// A named Bézier line through points given in squares.
#[derive(Debug, Clone)]
pub struct Line {
    /// Its name.
    pub name: Spanned<String>,
    /// The points it passes through, (across, up the page), in squares.
    pub through: Vec<(Expr, Expr)>,
    /// Organic wobble, if any.
    pub organic: Option<Organic>,
    /// The whole node.
    pub span: SourceSpan,
}

/// Spatially coherent wobble applied once to a line.
#[derive(Debug, Clone)]
pub struct Organic {
    /// How far it wanders, in squares.
    pub amount: Expr,
    /// The wobble's feature size, in squares.
    pub scale: Expr,
}

/// A region: the lines it names, the tile perimeter, and fill points.
#[derive(Debug, Clone)]
pub struct Region {
    /// Its name.
    pub name: Spanned<String>,
    /// The lines that bound it.
    pub lines: Vec<Spanned<String>>,
    /// Paint-bucket fill points, in squares.
    pub fills: Vec<(Expr, Expr)>,
    /// The whole node.
    pub span: SourceSpan,
}

/// What a stack entry applies.
#[derive(Debug, Clone)]
pub enum Callee {
    /// A built-in look.
    Look(&'static Look),
    /// A definition, from this file or an import.
    Definition(String),
}

/// One stack entry: a look or a definition applied to a region.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The name as written.
    pub what: Spanned<String>,
    /// What it resolved to.
    pub callee: Callee,
    /// `self=region`; `None` means the caller's region.
    pub region: Option<Spanned<String>>,
    /// A pinned seed.
    pub seed: Option<i64>,
    /// A stable id among same-named siblings.
    pub id: Option<String>,
    /// Top-level controls (or a definition's parameters).
    pub settings: Vec<Setting>,
    /// Grouped controls (`leaf length=4.5`).
    pub groups: Vec<GroupSetting>,
    /// The whole node.
    pub span: SourceSpan,
}

/// One control set to a value.
#[derive(Debug, Clone)]
pub struct Setting {
    /// The control's name.
    pub name: Spanned<String>,
    /// Its value.
    pub value: Spanned<Value>,
}

/// A group node and its settings.
#[derive(Debug, Clone)]
pub struct GroupSetting {
    /// The group's name.
    pub name: Spanned<String>,
    /// Its settings.
    pub settings: Vec<Setting>,
}

/// A decoded value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number, range or arithmetic.
    Expr(Expr),
    /// `#true` / `#false`.
    Bool(bool),
    /// A word: a choice, or the name of a line, region or tag.
    Word(String),
    /// Free text.
    Text(String),
}

impl Document {
    /// A definition by name.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&Definition> {
        self.definitions.iter().find(|d| d.name.value == name)
    }
}

impl Entry {
    /// A top-level setting by name.
    #[must_use]
    pub fn setting(&self, name: &str) -> Option<&Value> {
        self.settings
            .iter()
            .find(|s| s.name.value == name)
            .map(|s| &s.value.value)
    }

    /// A grouped setting by group and name.
    #[must_use]
    pub fn group_setting(&self, group: &str, name: &str) -> Option<&Value> {
        self.groups
            .iter()
            .find(|g| g.name.value == group)?
            .settings
            .iter()
            .find(|s| s.name.value == name)
            .map(|s| &s.value.value)
    }
}
