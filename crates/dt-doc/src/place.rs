//! Where things go: tile edges, corners, points and edge paths.
//!
//! Positions are in squares from the south-west corner, x running east
//! and y running north.  A point is written `"(0.65sq, 1sq)"` and read by
//! the decoder (`decode::Reader::point`); either coordinate may be a range,
//! drawn once from the tile seed.  A corner word (`north-west`) or an edge
//! word (`north`, the middle of that edge) stands in for a point.  An edge path is one or
//! more edge words, quoted when there are several (`at="north west"`).

use std::fmt;

use crate::expr::{Expr, ExprError};

/// A tile edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    /// y = rows.
    North,
    /// y = 0.
    South,
    /// x = cols.
    East,
    /// x = 0.
    West,
}

impl Edge {
    /// Every edge, in the order words are listed in help text.
    pub const ALL: [Edge; 4] = [Edge::North, Edge::South, Edge::East, Edge::West];

    /// Parse an edge word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == s)
    }

    /// The word as written.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Edge::North => "north",
            Edge::South => "south",
            Edge::East => "east",
            Edge::West => "west",
        }
    }

    /// The edge across the tile from this one.
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Edge::North => Edge::South,
            Edge::South => Edge::North,
            Edge::East => Edge::West,
            Edge::West => Edge::East,
        }
    }

    /// The middle of the edge, in squares, on a `cols` × `rows` tile.
    #[must_use]
    pub fn middle(self, cols: f64, rows: f64) -> (f64, f64) {
        match self {
            Edge::North => (cols / 2.0, rows),
            Edge::South => (cols / 2.0, 0.0),
            Edge::East => (cols, rows / 2.0),
            Edge::West => (0.0, rows / 2.0),
        }
    }
}

/// A tile corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corner {
    /// (0, rows).
    NorthWest,
    /// (cols, rows).
    NorthEast,
    /// (0, 0).
    SouthWest,
    /// (cols, 0).
    SouthEast,
}

impl Corner {
    /// Every corner.
    pub const ALL: [Corner; 4] = [
        Corner::NorthWest,
        Corner::NorthEast,
        Corner::SouthWest,
        Corner::SouthEast,
    ];

    /// Parse a corner word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// The word as written.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Corner::NorthWest => "north-west",
            Corner::NorthEast => "north-east",
            Corner::SouthWest => "south-west",
            Corner::SouthEast => "south-east",
        }
    }

    /// The corner two edges share, if they meet.
    #[must_use]
    pub fn between(a: Edge, b: Edge) -> Option<Self> {
        use Edge::{East, North, South, West};
        match (a, b) {
            (North, West) | (West, North) => Some(Corner::NorthWest),
            (North, East) | (East, North) => Some(Corner::NorthEast),
            (South, West) | (West, South) => Some(Corner::SouthWest),
            (South, East) | (East, South) => Some(Corner::SouthEast),
            _ => None,
        }
    }

    /// The corner, in squares, on a `cols` × `rows` tile.
    #[must_use]
    pub fn position(self, cols: f64, rows: f64) -> (f64, f64) {
        match self {
            Corner::NorthWest => (0.0, rows),
            Corner::NorthEast => (cols, rows),
            Corner::SouthWest => (0.0, 0.0),
            Corner::SouthEast => (cols, 0.0),
        }
    }
}

/// Where a point may land: `((x_lo, x_hi), (y_lo, y_hi))` in squares.
pub type Bounds = ((f64, f64), (f64, f64));

/// A point, in squares from the south-west corner.
#[derive(Debug, Clone, PartialEq)]
pub enum Point {
    /// A quoted pair.  Either coordinate may be a range, drawn once.
    Pair {
        /// Across, east from the west edge.
        x: Expr,
        /// Up the page, north from the south edge.
        y: Expr,
    },
    /// A corner word.
    Corner(Corner),
    /// An edge word: the middle of that edge.
    Edge(Edge),
    /// A built-in point (`northCenter`, `center`) or a declared `point`,
    /// by name; the evaluator knows where it is.
    Named(String),
    /// A point in a region's frame, found once the tile's regions exist
    /// (format 1; see [`crate::framed`]).
    Framed(Box<crate::framed::Framed>),
}

impl Point {
    /// The ranges the point may land in on a `cols` × `rows` tile, as
    /// `((x_lo, x_hi), (y_lo, y_hi))`.  A fixed point has `lo == hi`.
    ///
    /// # Errors
    /// If a coordinate refers to a name (points are plain numbers today).
    pub fn bounds(&self, cols: f64, rows: f64) -> Result<Bounds, String> {
        let none = |_: &str| None;
        Ok(match self {
            Point::Pair { x, y } => (x.bounds(&none)?, y.bounds(&none)?),
            Point::Corner(c) => {
                let (x, y) = c.position(cols, rows);
                ((x, x), (y, y))
            }
            Point::Edge(e) => {
                let (x, y) = e.middle(cols, rows);
                ((x, x), (y, y))
            }
            Point::Named(n) => return Err(format!("the point `{n}` has no fixed place yet")),
            Point::Framed(f) => {
                return Err(format!(
                    "the point `{f}` is in a region's frame, found when the tile's regions are known"
                ));
            }
        })
    }

    /// True when either coordinate is a range.
    #[must_use]
    pub fn is_range(&self) -> bool {
        matches!(self, Point::Pair { x, y } if x.is_range() || y.is_range())
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Point::Pair { x, y } => write!(f, "{x},{y}"),
            Point::Corner(c) => f.write_str(c.as_str()),
            Point::Edge(e) => f.write_str(e.as_str()),
            Point::Named(n) => f.write_str(n),
            Point::Framed(p) => write!(f, "{p}"),
        }
    }
}

/// The nine built-in points of a tile, camelCase, west to east and south
/// to north: `southWestCorner southCenter southEastCorner westCenter
/// center eastCenter northWestCorner northCenter northEastCorner`.
pub const BUILTIN_POINTS: [&str; 9] = [
    "southWestCorner",
    "southCenter",
    "southEastCorner",
    "westCenter",
    "center",
    "eastCenter",
    "northWestCorner",
    "northCenter",
    "northEastCorner",
];

/// The built-in lines: the four edges, and all four together.
pub const BUILTIN_LINES: [&str; 5] = ["northEdge", "eastEdge", "southEdge", "westEdge", "edges"];

/// The built-in region: the whole tile.
pub const BUILTIN_REGION: &str = "tile";

/// A built-in point, in squares on a `cols` × `rows` tile.
#[must_use]
pub fn builtin_point(name: &str, cols: f64, rows: f64) -> Option<(f64, f64)> {
    let ix = BUILTIN_POINTS.iter().position(|n| *n == name)?;
    let x = [0.0, cols / 2.0, cols][ix % 3];
    let y = [0.0, rows / 2.0, rows][ix / 3];
    Some((x, y))
}

/// A built-in line as a polyline in squares on a `cols` × `rows` tile; the
/// four edges together (`edges`) close the loop.
#[must_use]
pub fn builtin_line(name: &str, cols: f64, rows: f64) -> Option<Vec<[f64; 2]>> {
    Some(match name {
        "northEdge" => vec![[0.0, rows], [cols, rows]],
        "eastEdge" => vec![[cols, 0.0], [cols, rows]],
        "southEdge" => vec![[0.0, 0.0], [cols, 0.0]],
        "westEdge" => vec![[0.0, 0.0], [0.0, rows]],
        "edges" => vec![
            [0.0, 0.0],
            [cols, 0.0],
            [cols, rows],
            [0.0, rows],
            [0.0, 0.0],
        ],
        _ => return None,
    })
}

/// Which side of a drawn line a thing stands on, walking the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// To the left as you walk the line.
    Left,
    /// To the right.
    Right,
    /// Centred on the line (the default).
    Centre,
}

impl dt_value::Canonical for Side {
    fn canon(&self, c: &mut dt_value::Canon) {
        c.tag(match self {
            Side::Left => b'l',
            Side::Right => b'r',
            Side::Centre => b'c',
        });
    }
}

impl Side {
    /// The words, in help order.
    pub const WORDS: [&'static str; 3] = ["left", "right", "centre"];

    /// Parse a side word.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            "centre" => Some(Side::Centre),
            _ => None,
        }
    }
}

/// What a line `at=` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Along {
    /// One tile edge, or a path of edges that meet at corners
    /// (`"north west"`).  A wall or cliff on an edge stands inside the
    /// tile, its outer face flush with the edge.
    Edges(Vec<Edge>),
    /// A named `line` of this definition.
    Line(String),
}

/// Why a point or edge path couldn't be read: a message and the byte
/// range inside the text.
pub type PlaceError = ExprError;

fn perr(message: impl Into<String>, start: usize, end: usize) -> PlaceError {
    ExprError {
        message: message.into(),
        code: None,
        help: None,
        start,
        end,
    }
}

/// True when the `(` that starts `t` closes at its very end.
pub fn encloses(t: &str) -> bool {
    let mut depth = 0usize;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1 == t.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// The commas of `s` outside every parenthesis and bracket.
pub fn top_commas(s: &str) -> Vec<usize> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => out.push(i),
            _ => {}
        }
    }
    out
}

/// Parse an edge path: one or more edge words, each meeting the next at
/// a corner (`"north west"`).  Anything else is returned as `None`, to be
/// read as a line name.
///
/// # Errors
/// If the text is edge words that don't make a path.
pub fn parse_edges(s: &str) -> Result<Option<Vec<Edge>>, PlaceError> {
    let words: Vec<(usize, &str)> = s
        .split_whitespace()
        .map(|w| (w.as_ptr() as usize - s.as_ptr() as usize, w))
        .collect();
    if words.is_empty() || !words.iter().all(|(_, w)| Edge::parse(w).is_some()) {
        if words.len() > 1
            && let Some((at, w)) = words.iter().find(|(_, w)| Edge::parse(w).is_none())
        {
            return Err(perr(
                format!("`{w}` is not an edge: a path of edges uses north, south, east and west"),
                *at,
                at + w.len(),
            ));
        }
        return Ok(None);
    }
    let edges: Vec<Edge> = words.iter().filter_map(|(_, w)| Edge::parse(w)).collect();
    for (i, pair) in edges.windows(2).enumerate() {
        let (at, w) = words[i + 1];
        if pair[0] == pair[1] || edges[..=i].contains(&pair[1]) {
            return Err(perr(
                format!("`{w}` is already on this path"),
                at,
                at + w.len(),
            ));
        }
        if Corner::between(pair[0], pair[1]).is_none() {
            return Err(perr(
                format!(
                    "`{}` and `{w}` don't meet at a corner: a path walks round the tile edge by edge",
                    pair[0].as_str()
                ),
                at,
                at + w.len(),
            ));
        }
    }
    Ok(Some(edges))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_paths() {
        assert_eq!(parse_edges("north").unwrap(), Some(vec![Edge::North]));
        assert_eq!(
            parse_edges("north west").unwrap(),
            Some(vec![Edge::North, Edge::West])
        );
        assert_eq!(parse_edges("trail").unwrap(), None);
        let e = parse_edges("north south").unwrap_err();
        assert!(e.message.contains("don't meet"), "{e:?}");
        assert_eq!((e.start, e.end), (6, 11));
        assert!(parse_edges("north north").is_err());
        assert!(parse_edges("north up").is_err());
    }
}
