//! The map of a tile for the editor's overlay: its lines as drawn, its
//! regions as the build fills them (on the draft build's lattice), and where each
//! line point and fill point is written, so the page can drag them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A point in squares.
pub type Xy = [f64; 2];
/// A byte span in a document: its start and its length.
pub type Span = [usize; 2];

/// The file a poster's map is kept in (`posters/KEY.map.json`): the tile's map and the build target's square, which the
/// marks are drawn on.  The square is the target's, so it travels with the map made for that target: a `TileMap` is
/// the same type in the editor, where the target arrives with every check (`Target.squareMm`), and holding the size
/// there too would be a second source of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PosterMap {
    /// The square's size on the target the poster was made for, mm.
    pub square_mm: f64,
    pub map: TileMap,
}

/// The map of a tile: what the editor's overlay and the site's cards draw.  One type for every reader (the page,
/// the site, the poster files); its TypeScript type, `TileMap`, is generated from this one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TileMap {
    pub language: Language,
    pub cols: u32,
    pub rows: u32,
    /// Lattice nodes per square of every region's bits.
    pub cells: u32,
    pub lines: Vec<MapLine>,
    pub regions: Vec<MapRegion>,
    /// The tile's own `point` declarations.
    pub points: Vec<NamedPoint>,
    pub places: Vec<Place>,
    /// The tile node itself; none when the document has no `tile` node.
    pub tile: Option<TileNode>,
}

/// The words a value is written from, for completion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub names: Vec<String>,
    pub units: Vec<String>,
    pub functions: Vec<String>,
}

/// A written value: its text as written, its byte span, and the value as words (quotes off, escapes undone).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    pub text: String,
    pub span: Span,
    pub plain: String,
}

impl Written {
    fn new(text: String, span: Span) -> Self {
        Self {
            plain: dt_doc::nodeid::plain_of(&text),
            text,
            span,
        }
    }
}

/// A control written on a node: its name, its value as written and the value's span, the value as words, and the whole
/// control (name through value) as written with its span.  The page edits at the spans and shows the words; it works out
/// neither.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MapControl {
    pub name: String,
    pub text: String,
    pub span: Span,
    /// The value with its quotes off and its escapes undone.
    pub plain: String,
    /// The whole control, `name=value` as written, and where.
    pub whole: Written,
}

impl MapControl {
    /// A control whose whole text (`name=value`, or the line `name value`) is at `whole`, in `text`.
    fn at(text: &str, name: &str, whole: [usize; 2], value: (String, [usize; 2])) -> Self {
        Self {
            name: name.to_owned(),
            plain: dt_doc::nodeid::plain_of(&value.0),
            text: value.0,
            span: value.1,
            whole: Written::new(text[whole[0]..whole[0] + whole[1]].to_owned(), whole),
        }
    }
}

/// The arguments of a node's child, as one run of text; the span is none when there are no arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub at: usize,
    pub text: String,
    /// `text` as words (one string's quotes off), as for a [`Written`].
    pub plain: String,
    pub span: Option<Span>,
}

/// A written point of a line's `through` or a region's `fill`, and where it lies when the document gave it a place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WrittenPoint {
    pub text: String,
    pub span: Span,
    pub at: Option<Xy>,
}

/// How a marker's text names its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    Literal,
    Name,
}

/// A control point a line or region refers to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// The source as written.
    pub label: String,
    pub kind: MarkerKind,
    pub at: Option<Xy>,
    pub span: Option<Span>,
    /// True when it lies somewhere in a range, so `at` is only the middle of where it may land.
    pub ranged: bool,
}

/// A line as drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MapLine {
    pub name: String,
    pub pts: Vec<Xy>,
    /// How far the line may wander, printed mm on this target.
    pub wobble_mm: Option<f64>,
    pub span: Option<Span>,
    pub markers: Vec<Marker>,
    pub points: Vec<WrittenPoint>,
    pub sheet: Option<LineSheet>,
}

/// What a line's node writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LineSheet {
    pub at: usize,
    pub controls: Vec<MapControl>,
    pub through: Option<Run>,
}

/// A region as the build fills it, on the draft lattice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MapRegion {
    pub name: String,
    pub bounded_by: Vec<String>,
    pub nx: usize,
    pub ny: usize,
    /// One `0` or `1` for each lattice node.
    pub bits: String,
    pub span: Option<Span>,
    pub markers: Vec<Marker>,
    pub fills: Vec<WrittenPoint>,
    pub sheet: Option<RegionSheet>,
    pub refs: Vec<PlaceRef>,
}

/// What a region's node writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RegionSheet {
    pub at: usize,
    pub bounded_by: Option<Run>,
    pub fill: Option<Run>,
    pub clauses: Vec<Clause>,
}

/// A `within`'s `of`: the place as named, as written, and its span.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OfPlace {
    pub text: String,
    pub written: String,
    pub span: Span,
}

/// One clause of a region's body, in the order written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Clause {
    /// All that lies within a distance of a place; `printed` is the distance's printed size on this target.
    Within {
        at: usize,
        distance: Option<Written>,
        of: Option<OfPlace>,
        printed: Option<Vec<String>>,
    },
    /// The regions taken out.
    Except { at: usize, regions: Run },
}

/// Where a place a region is made from lies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlaceRef {
    Point {
        name: String,
        at: Xy,
    },
    /// The whole tile, as its edge ring.
    Tile {
        name: String,
        pts: Option<Vec<Xy>>,
    },
    Edge {
        name: String,
        pts: Vec<Xy>,
    },
    Line {
        name: String,
        pts: Vec<Xy>,
    },
    /// Found by name among the map's regions.
    Region {
        name: String,
    },
    Unknown {
        name: String,
    },
}

/// What a place is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PlaceKind {
    Edge,
    Line,
    Region,
    Tile,
    Point,
}

/// A place a tile has, for a choice among them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub kind: PlaceKind,
}

/// A `point name "(x, y)"` declaration: where it is and where it is written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NamedPoint {
    pub name: String,
    pub at: Xy,
    pub span: Span,
}

/// The tile node itself, for the property sheet when nothing is selected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TileNode {
    pub at: usize,
    pub controls: Vec<MapControl>,
    pub about: Option<About>,
    pub heights: Vec<Height>,
}

/// `about "…"`: where it is and its string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct About {
    pub at: usize,
    pub value: Option<Written>,
}

/// Where a height constant comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum HeightFrom {
    Library,
    Tile,
    Override,
}

/// A height a value may name: its value in mm at the reference square where it is one number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Height {
    pub name: String,
    pub value: Option<f64>,
    pub text: String,
    pub from: HeightFrom,
}

/// Lattice nodes per square for the regions' areas: the draft build's
/// lattice, so a region's edge on the map steps under a millimetre (0.7 mm
/// on DungeonBlocks, 0.5 on OpenLOCK) and meets its lines.
const CELLS: u32 = crate::DRAFT_CELLS;

/// How the map reads a written point: as the document's own `"(0.5sq, 25%)"`
/// forms, which only the program's reader ([`dt_doc::decode::point_text`])
/// knows how to read.
#[derive(Clone, Copy)]
struct Reading {
    units: dt_doc::units::Units,
}

impl Reading {
    /// The point a text writes, if it writes one.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: parsing where None is the meaning (text that is not a written point is not a point; the checker reports the real diagnostic where the user wrote it)"
    )]
    fn point(self, t: &crate::Tile, text: &str) -> Option<dt_doc::place::Point> {
        dt_doc::decode::point_text(text.trim(), (t.cols, t.rows), self.units).ok()
    }
}

/// A written value: its text and its byte span.
type WrittenEntry = (String, [usize; 2]);

/// The written entries of each `name` child of the node `parent NAME`, in
/// order: (their text, their byte span), with the node's own span.
fn written(
    tile: &kdl::KdlNode,
    parent: &str,
    name: &str,
    child: &str,
) -> Option<([usize; 2], Vec<WrittenEntry>)> {
    let node = tile.children()?.nodes().iter().find(|n| {
        n.name().value() == parent
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(name))
    })?;
    let mut out = Vec::new();
    for c in node.children().map(|c| c.nodes()).unwrap_or_default() {
        if c.name().value() != child {
            continue;
        }
        for e in c.entries().iter().filter(|e| e.name().is_none()) {
            let v = e.value();
            let text = v.as_string().map_or_else(|| v.to_string(), str::to_owned);
            out.push((text, [e.span().offset(), e.span().len()]));
        }
    }
    Some(([node.span().offset(), node.span().len()], out))
}

/// An entry's value as written (not its `name=`), with the value's byte span.
fn entry_value(text: &str, e: &kdl::KdlEntry) -> (String, [usize; 2]) {
    let sp = e.span();
    let whole = &text[sp.offset()..sp.offset() + sp.len()];
    let from = if e.name().is_some() {
        whole.find('=').map_or(0, |i| i + 1)
    } else {
        0
    };
    let rest = &whole[from..];
    let lead = rest.len() - rest.trim_start().len();
    (
        rest.trim().to_owned(),
        [sp.offset() + from + lead, rest.trim().len()],
    )
}

/// A node's single-valued controls, both as `name=value` and as a
/// `name value` line inside it (not `skip`): each name, its value as
/// written and the value's byte span.
fn controls_of(text: &str, node: &kdl::KdlNode, skip: &[&str]) -> Vec<MapControl> {
    let mut out = entries_of(text, node);
    for c in node
        .children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
    {
        let name = c.name().value();
        if skip.contains(&name) || c.children().is_some() {
            continue;
        }
        if let [e] = c.entries()
            && e.name().is_none()
        {
            let sp = c.span();
            let raw = &text[sp.offset()..sp.offset() + sp.len()];
            let lead = raw.len() - raw.trim_start().len();
            let whole = [sp.offset() + lead, raw.trim().len()];
            out.push(MapControl::at(text, name, whole, entry_value(text, e)));
        }
    }
    out
}

/// A node's `name=value` properties, each as a control.
fn entries_of(text: &str, node: &kdl::KdlNode) -> Vec<MapControl> {
    node.entries()
        .iter()
        .filter_map(|e| {
            let name = e.name()?.value();
            let (start, end) = dt_doc::nodeid::entry_range(text, e);
            Some(MapControl::at(
                text,
                name,
                [start, end - start],
                entry_value(text, e),
            ))
        })
        .collect()
}

/// The arguments of `node`'s first `child` line, as one run of text: the
/// text and its span (from the first argument to the end of the last).
fn args_of(text: &str, node: &kdl::KdlNode, child: &str) -> Option<Run> {
    node.children()
        .and_then(|d| d.nodes().iter().find(|n| n.name().value() == child))
        .map(|c| run_of(text, c))
}

/// The positional arguments of `c`, as one run of text with its span.
fn run_of(text: &str, c: &kdl::KdlNode) -> Run {
    let args: Vec<_> = c.entries().iter().filter(|e| e.name().is_none()).collect();
    let (Some(a), Some(b)) = (args.first(), args.last()) else {
        return Run {
            at: c.name().span().offset(),
            text: String::new(),
            plain: String::new(),
            span: None,
        };
    };
    let lead = |e: &kdl::KdlEntry| {
        let sp = e.span();
        let w = &text[sp.offset()..sp.offset() + sp.len()];
        (
            sp.offset() + w.len() - w.trim_start().len(),
            sp.offset() + w.trim_end().len(),
        )
    };
    let (start, _) = lead(a);
    let (_, end) = lead(b);
    Run {
        at: c.name().span().offset(),
        plain: dt_doc::nodeid::plain_of(&text[start..end]),
        text: text[start..end].to_owned(),
        span: Some([start, end - start]),
    }
}

/// What a region's body writes, clause by clause in the order written: its
/// `within` clauses (the distance and the place, each with its span) and
/// its `except` clauses (the regions, as one run).  Each `within` carries
/// what its distance prints on this target (`printed`, from
/// [`dt_doc::printed::printed_size`], the one place that works it out; null
/// where it prints as written): the page writes no unit word of its own.
fn clauses(
    text: &str,
    node: &kdl::KdlNode,
    units: dt_doc::units::Units,
    consts: &dt_doc::consts::ConstTable,
) -> Vec<Clause> {
    let mut out = Vec::new();
    for c in node
        .children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
    {
        let at = c.name().span().offset();
        match c.name().value() {
            "within" => {
                let dist = c
                    .entries()
                    .iter()
                    .find(|e| e.name().is_none())
                    .map(|e| entry_value(text, e));
                let of = c
                    .entries()
                    .iter()
                    .find(|e| e.name().is_some_and(|n| n.value() == "of"))
                    .map(|e| entry_value(text, e));
                let printed = dist.as_ref().and_then(|(t, _)| {
                    dt_doc::printed::printed_size(
                        dt_doc::schema::Axis::Dmm,
                        &dt_doc::nodeid::plain_of(t),
                        units,
                        Some(consts),
                    )
                });
                out.push(Clause::Within {
                    at,
                    distance: dist.map(|(text, span)| Written::new(text, span)),
                    of: of.map(|(t, span)| OfPlace {
                        text: dt_doc::nodeid::plain_of(&t),
                        written: t,
                        span,
                    }),
                    printed: printed.map(|p| p.ends),
                });
            }
            "except" => out.push(Clause::Except {
                at,
                regions: run_of(text, c),
            }),
            _ => {}
        }
    }
    out
}

/// Where a place a region is made from lies: its name, what kind of place
/// it is, and its geometry in squares (a line's points, a point; a region
/// is found by name among the map's regions).
fn place_ref(t: &crate::Tile, name: &str, rd: Reading) -> PlaceRef {
    use dt_doc::place::{
        BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION, builtin_line, builtin_point,
    };
    let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
    if (name.starts_with('(') || name.starts_with("point(") || name.starts_with("polar("))
        && let Some(p) = rd.point(t, name)
        && let Ok(((x0, x1), (y0, y1))) = p.bounds(cols, rows)
    {
        let at = [f64::midpoint(x0, x1), f64::midpoint(y0, y1)];
        return PlaceRef::Point {
            name: name.to_owned(),
            at,
        };
    }
    if name == BUILTIN_REGION {
        let ring = builtin_line("edges", cols, rows);
        return PlaceRef::Tile {
            name: name.to_owned(),
            pts: ring,
        };
    }
    if let Some(xy) = t.heights.points.get(name) {
        return PlaceRef::Point {
            name: name.to_owned(),
            at: *xy,
        };
    }
    if BUILTIN_POINTS.contains(&name)
        && let Some((x, y)) = builtin_point(name, cols, rows)
    {
        return PlaceRef::Point {
            name: name.to_owned(),
            at: [x, y],
        };
    }
    if BUILTIN_LINES.contains(&name)
        && let Some(pts) = builtin_line(name, cols, rows)
    {
        return PlaceRef::Edge {
            name: name.to_owned(),
            pts,
        };
    }
    if let Some(pts) = t.lines.get(name) {
        return PlaceRef::Line {
            name: name.to_owned(),
            pts: pts.clone(),
        };
    }
    if t.regions.iter().any(|r| r.name == name) {
        return PlaceRef::Region {
            name: name.to_owned(),
        };
    }
    PlaceRef::Unknown {
        name: name.to_owned(),
    }
}

/// Where a written point lies, in squares, and whether it is a `literal`
/// pair or a `name` (a corner or edge word, a declared or built-in point).
/// A coordinate drawn from a range or written as an expression is placed
/// where its range middles; a name nothing declares has no place.  The
/// reading is the program's own (the decoder's), not the page's.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where None means not a form this reads; the checker reports the real diagnostic where the user wrote it"
)]
fn locate(t: &crate::Tile, text: &str, rd: Reading) -> (MarkerKind, Option<[f64; 2]>) {
    use dt_doc::place::{BUILTIN_POINTS, Point, builtin_point};
    let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
    if let Some(p) = rd.point(t, text) {
        let kind = if matches!(p, Point::Pair { .. }) {
            MarkerKind::Literal
        } else {
            MarkerKind::Name
        };
        let at = p
            .bounds(cols, rows)
            .ok()
            .map(|((x0, x1), (y0, y1))| [f64::midpoint(x0, x1), f64::midpoint(y0, y1)]);
        return (kind, at);
    }
    let name = text.trim();
    let at = t.heights.points.get(name).copied().or_else(|| {
        BUILTIN_POINTS
            .contains(&name)
            .then(|| builtin_point(name, cols, rows))
            .flatten()
            .map(|(x, y)| [x, y])
    });
    (MarkerKind::Name, at)
}

/// A control point a line or region refers to: its label (the source as
/// written: the coordinates, or the point's name), where it is, and the
/// written value's byte span.  `ranged` is true when it lies somewhere in a
/// range, so `at` is only the middle of where it may land.
fn marker(t: &crate::Tile, text: &str, span: Option<[usize; 2]>, rd: Reading) -> Marker {
    let (kind, at) = locate(t, text, rd);
    let ranged = rd.point(t, text).is_some_and(|p| p.is_range());
    Marker {
        label: text.trim().to_owned(),
        kind,
        at,
        span,
        ranged,
    }
}

/// The `of` points of a region's `within` clauses: the ones written as a
/// literal pair or as the name of a point (a line or region is drawn as
/// itself, not as a point).
fn within_markers(t: &crate::Tile, node: &kdl::KdlNode, text: &str, rd: Reading) -> Vec<Marker> {
    use dt_doc::place::{BUILTIN_POINTS, Point};
    node.children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
        .iter()
        .filter(|c| c.name().value() == "within")
        .filter_map(|c| {
            let e = c
                .entries()
                .iter()
                .find(|e| e.name().is_some_and(|n| n.value() == "of"))?;
            let (w, span) = entry_value(text, e);
            let w = dt_doc::nodeid::plain_of(&w);
            let name = w.trim();
            let is_point = t.heights.points.contains_key(name)
                || BUILTIN_POINTS.contains(&name)
                || matches!(rd.point(t, &w), Some(Point::Pair { .. }));
            is_point.then(|| marker(t, &w, Some(span), rd))
        })
        .collect()
}

/// Every control point a region refers to: each `fill` point, then each
/// `within … of=` centre that is a point.
fn markers_of(
    t: &crate::Tile,
    tile: Option<&kdl::KdlNode>,
    r: &crate::Region,
    fills: Option<&[WrittenEntry]>,
    text: &str,
    rd: Reading,
) -> Vec<Marker> {
    let mut out: Vec<Marker> = fills
        .unwrap_or_default()
        .iter()
        .map(|(x, sp)| marker(t, x, Some(*sp), rd))
        .collect();
    if let Some(node) = tile.and_then(|n| mark_node(n, "region", &r.name)) {
        out.extend(within_markers(t, node, text, rd));
    }
    out
}

/// The places a region is made from, each once: the lines that bound it,
/// the places its `within`s are round, the regions it excepts.
fn region_refs(t: &crate::Tile, r: &crate::Region, rd: Reading) -> Vec<PlaceRef> {
    let mut names: Vec<String> = r.bounded_by.iter().map(|b| b.value.clone()).collect();
    names.extend(r.within.iter().map(|w| w.of.to_string()));
    names.extend(r.except.iter().cloned());
    let mut seen = std::collections::BTreeSet::new();
    names.retain(|n| *n != r.name && seen.insert(n.clone()));
    names.iter().map(|n| place_ref(t, n, rd)).collect()
}

/// Every place a tile has, for a choice among them: the built-in edges and
/// points, the tile's own points, lines and regions.
fn places(t: &crate::Tile) -> Vec<Place> {
    use dt_doc::place::{BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION};
    let mut out = Vec::new();
    let mut add = |name: &str, kind: PlaceKind| {
        out.push(Place {
            name: name.to_owned(),
            kind,
        });
    };
    for n in BUILTIN_LINES {
        add(n, PlaceKind::Edge);
    }
    for l in t.lines.keys() {
        add(l, PlaceKind::Line);
    }
    for r in &t.regions {
        add(&r.name, PlaceKind::Region);
    }
    add(BUILTIN_REGION, PlaceKind::Tile);
    for n in BUILTIN_POINTS {
        add(n, PlaceKind::Point);
    }
    for p in t.heights.points.keys() {
        if !BUILTIN_POINTS.contains(&p.as_str()) {
            add(p, PlaceKind::Point);
        }
    }
    out
}

/// The `line` or `region` node of the tile called `name`.
fn mark_node<'a>(tile: &'a kdl::KdlNode, kind: &str, name: &str) -> Option<&'a kdl::KdlNode> {
    tile.children()?.nodes().iter().find(|n| {
        n.name().value() == kind
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(name))
    })
}

fn points(w: &[WrittenEntry], at: Option<&Vec<[f64; 2]>>) -> Vec<WrittenPoint> {
    w.iter()
        .enumerate()
        .map(|(k, (text, span))| {
            let at = at.filter(|a| a.len() == w.len()).map(|a| a[k]);
            WrittenPoint {
                text: text.clone(),
                span: *span,
                at,
            }
        })
        .collect()
}

/// The map of checked tile `c`, its spans from `text` (the tile's own document as checked); none when the
/// document has no tile.  The one constructor of [`TileMap`].
///
/// # Errors
/// The regions' masks could not be made.
pub fn json(
    c: &crate::Checked,
    text: &str,
    units: dt_doc::units::Units,
) -> Result<Option<TileMap>, Vec<crate::Diag>> {
    let Some(t) = &c.tile else {
        return Ok(None);
    };
    let rd = Reading { units };
    let doc = kdl::KdlDocument::parse(text).map_err(|e| {
        vec![crate::Diag::plain(
            dt_lang::codes::SYNTAX,
            format!("the tile's document does not parse: {e}"),
        )]
    })?;
    let node = doc.nodes().iter().find(|n| n.name().value() == "tile");
    let masks = c.region_masks(CELLS)?;
    let lines: Vec<MapLine> = t
        .lines
        .iter()
        .map(|(name, pts)| {
            let w = node.and_then(|n| written(n, "line", name, "through"));
            let control = t.controls.get(name).or(Some(pts));
            MapLine {
                name: name.clone(),
                pts: pts.clone(),
                // how far the line may wander, printed mm on this target (the
                // page draws it as it is and does no unit arithmetic)
                wobble_mm: t
                    .wobbles
                    .get(name)
                    .map(|(_, w)| w * units.square_mm / dt_doc::schema::SQUARE_MM),
                span: w.as_ref().map(|w| w.0),
                markers: w
                    .as_ref()
                    .map(|(_, w)| {
                        w.iter()
                            .map(|(x, sp)| marker(t, x, Some(*sp), rd))
                            .collect()
                    })
                    .unwrap_or_default(),
                points: w.map(|(_, w)| points(&w, control)).unwrap_or_default(),
                sheet: node
                    .and_then(|n| mark_node(n, "line", name))
                    .map(|l| LineSheet {
                        at: l.name().span().offset(),
                        controls: controls_of(text, l, &["through"]),
                        through: args_of(text, l, "through"),
                    }),
            }
        })
        .collect();
    let regions: Vec<MapRegion> = t
        .regions
        .iter()
        .zip(&masks)
        // a line's width makes a strip no document names: the map has the line
        .filter(|(r, _)| !r.strip)
        .map(|(r, m)| {
            let w = node.and_then(|n| written(n, "region", &r.name, "fill"));
            let bits: String = m.bits.iter().map(|&b| if b { '1' } else { '0' }).collect();
            MapRegion {
                name: r.name.clone(),
                bounded_by: r.bounded_by.iter().map(|b| b.value.clone()).collect(),
                nx: m.nx,
                ny: m.ny,
                bits,
                span: w.as_ref().map(|w| w.0),
                markers: markers_of(t, node, r, w.as_ref().map(|(_, w)| w.as_slice()), text, rd),
                fills: w
                    .map(|(_, w)| points(&w, Some(&r.fills)))
                    .unwrap_or_default(),
                sheet: node
                    .and_then(|n| mark_node(n, "region", &r.name))
                    .map(|g| RegionSheet {
                        at: g.name().span().offset(),
                        bounded_by: args_of(text, g, "boundedBy"),
                        fill: args_of(text, g, "fill"),
                        clauses: clauses(text, g, units, &t.heights.consts),
                    }),
                refs: region_refs(t, r, rd),
            }
        })
        .collect();
    // the tile's own `point name "(x, y)"` declarations: where each is and where it is written
    let named: Vec<NamedPoint> = node
        .and_then(|n| n.children())
        .map(|c| c.nodes())
        .unwrap_or_default()
        .iter()
        .filter(|n| n.name().value() == "point")
        .filter_map(|n| {
            let name = n.entries().first()?.value().as_string()?;
            let at = *t.heights.points.get(name)?;
            Some(NamedPoint {
                name: name.to_owned(),
                at,
                span: [n.span().offset(), n.span().len()],
            })
        })
        .collect();
    // the tile node itself, for the property sheet when nothing is selected
    let own = node.map(|n| {
        let at = n.name().span().offset();
        let controls = entries_of(text, n);
        let child = |name: &str| {
            n.children()
                .and_then(|c| c.nodes().iter().find(|k| k.name().value() == name))
        };
        // `about "…"`: its string, and where the string is written
        let about = child("about").map(|a| {
            let value = a.entries().first().filter(|e| e.name().is_none()).map(|e| {
                let sp = e.span();
                let whole = &text[sp.offset()..sp.offset() + sp.len()];
                let lead = whole.len() - whole.trim_start().len();
                let v = whole.trim();
                Written::new(v.to_owned(), [sp.offset() + lead, v.len()])
            });
            About {
                at: a.name().span().offset(),
                value,
            }
        });
        TileNode {
            at,
            controls,
            about,
            heights: heights(&t.heights.consts),
        }
    });
    Ok(Some(TileMap {
        language: language(t),
        cols: t.cols,
        rows: t.rows,
        cells: CELLS,
        lines,
        regions,
        points: named,
        places: places(t),
        tile: own,
    }))
}

/// The words a value is written from, for completion: the `$` names (what
/// an element sets, the regions, the tile's heights), the units and the
/// functions.
fn language(t: &crate::Tile) -> Language {
    let mut names: Vec<String> = dt_lang::POINT_NAMES
        .iter()
        .chain(dt_lang::REGION_NAMES)
        .map(|n| (*n).to_owned())
        .collect();
    names.extend(
        t.heights
            .consts
            .entries()
            .iter()
            .filter(|e| e.name.starts_with('$'))
            .map(|e| e.name.clone()),
    );
    Language {
        names,
        units: dt_lang::UNITS.iter().map(|u| (*u).to_owned()).collect(),
        functions: dt_lang::function_words()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }
}

/// The heights a value may name in this tile: its constants (the library's
/// first, then its own `let`s), each with its value in mm at the reference
/// square where it is one number, and where it comes from.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where None means not a form this reads; the checker reports the real diagnostic where the user wrote it"
)]
fn heights(table: &dt_doc::consts::ConstTable) -> Vec<Height> {
    use dt_doc::consts::Origin;
    table
        .entries()
        .iter()
        .map(|e| {
            let value = table.scalar(&e.expr, 0.5, &|_| None).ok();
            let from = match e.origin {
                Origin::Library => HeightFrom::Library,
                Origin::Tile => HeightFrom::Tile,
                Origin::Override => HeightFrom::Override,
            };
            Height {
                name: e.name.clone(),
                value,
                text: e.text.clone(),
                from,
            }
        })
        .collect()
}
