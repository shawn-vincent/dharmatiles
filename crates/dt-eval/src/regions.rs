//! The region tree of a format 1 tile: every thing makes its region, and
//! regions nest.
//!
//! * `$tile` is the root; its parent is none.
//! * A named region's parent is the smallest region containing its fill
//!   point, else the tile.  Regions that only share a line (a pool and its
//!   bank) are siblings.
//! * A point- or line-placed thing makes a region from its size: a tree's
//!   crown disc, a rock's or flower's disc, a wall's strip.  Its parent is
//!   the region it is written in (its `at=` region, else the tile).
//! * A body (`dirt`, `water`, floors, `cliff`) and a coat have the region
//!   they act in: their `at=` region, else the tile.  A scatter's region is
//!   the same, or the strip of the line it runs along.
//! * Each thing a scatter places has its own region under the scatter's.  A
//!   scatter's things are not placed until the build, so the tree holds one
//!   node for each kind of thing the scatter lists (kind [`Kind::Placed`]),
//!   with the size a placed thing's disc will have; the build makes each
//!   placed thing's disc from [`disc`].
//!
//! Geometry is a raster on the build's lattice (the check's [`Frame`]):
//! each node with a footprint holds a [`Mask`], and its bounding box in
//! squares.  Coordinates are squares from the south-west corner, as
//! format 0 counts them (format 1's north-west move comes with the
//! coordinate functions).

use dt_geom::Mask;
use dt_kit::Frame;

use crate::diag::Diag;
use crate::doc::{Entry, Owner, Region, Setting, Tile, strip_name};
use crate::places::Places;

/// What a region is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The whole tile: the root.
    Tile,
    /// A region the tile names.
    Named,
    /// The strip a line's width makes.
    Strip,
    /// A disc round a point: a tree's crown, a rock, a flower.
    Disc,
    /// A wall's strip along its line.
    Run,
    /// One kind of thing a scatter places: each thing placed has a disc of
    /// this size under the scatter's region.
    Placed,
}

impl Kind {
    /// The word `check --explain` prints.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Kind::Tile => "tile",
            Kind::Named => "region",
            Kind::Strip => "strip",
            Kind::Disc => "disc",
            Kind::Run => "wall strip",
            Kind::Placed => "placed disc",
        }
    }
}

/// A bounding box in squares from the south-west corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    /// West edge.
    pub x0: f64,
    /// South edge.
    pub y0: f64,
    /// East edge.
    pub x1: f64,
    /// North edge.
    pub y1: f64,
}

/// One region.
#[derive(Clone, Debug)]
pub struct Node {
    /// What it is called in messages: a region's name, `tree (entry 3)`.
    pub label: String,
    /// What it is made from.
    pub kind: Kind,
    /// The region containing it (`None` for the tile).
    pub parent: Option<usize>,
    /// The stack entry that makes it, for a node a thing made.
    pub entry: Option<usize>,
    /// The nodes it covers on the check's lattice; `None` for the tile (all
    /// of them) and for a [`Kind::Placed`] node (made where each is placed).
    pub mask: Option<Mask>,
    /// Its bounding box, in squares; `None` where it has no footprint yet,
    /// or covers nothing.
    pub bbox: Option<BBox>,
    /// A disc's radius in squares: the size a [`Kind::Placed`] thing's disc
    /// will have, and the radius of a [`Kind::Disc`].
    pub radius: Option<f64>,
}

/// The region tree of one tile.
#[derive(Clone, Debug)]
pub struct RegionTree {
    /// Every region; the tile is [`RegionTree::TILE`].
    pub nodes: Vec<Node>,
    /// The region of each stack entry, by its place in [`Tile::stack`].
    pub of_entry: Vec<usize>,
    /// For a scatter's entry, the region of each thing it lists, in order.
    pub of_child: Vec<Vec<usize>>,
    /// The lattice the masks are on.
    pub frame: Frame,
}

/// What a size written as a range or a weighted range is taken as, before
/// anything is drawn: the middle of it (as `within` takes it).
fn middle(v: &dt_elements::Value) -> Option<f64> {
    match v {
        dt_elements::Value::Number(n) => Some(*n),
        dt_elements::Value::Range(a, b) => Some(0.5 * (a + b)),
        dt_elements::Value::Dist(d) => Some(0.5 * (d.lo + d.hi)),
        dt_elements::Value::Draw(d) => {
            let (a, b) = d.bounds();
            Some(0.5 * (a + b))
        }
        dt_elements::Value::Word(_) => None,
    }
}

fn setting<'a>(controls: &'a [Setting], name: &str) -> Option<&'a Setting> {
    controls.iter().find(|s| s.name == name)
}

/// A tree's crown diameter, mm at the reference square: its `crown/width`,
/// which the tree element draws its crown radius from (half of it), else the
/// element's own default.
const CROWN_WIDTH_MM: f64 = dt_elements::tree::DEFAULT_CROWN_WIDTH_MM;

/// A wall's thickness when none is written (the schema's default).
const WALL_THICKNESS_MM: f64 = 7.0;

/// The width of a thing's footprint in mm at the reference square, and
/// whether it is a disc (else a strip along a line), by what it is: a
/// tree's crown diameter, a rock's or flower's size, a wall's thickness.
fn footprint(what: &str, controls: &[Setting]) -> Option<(f64, bool)> {
    let width = |name: &str, default: f64| {
        setting(controls, name)
            .and_then(|s| middle(&s.value))
            .unwrap_or(default)
    };
    match what {
        "tree" => Some((width("crown/width", CROWN_WIDTH_MM), true)),
        "rock" | "stone" | "boulder" | "pebble" => setting(controls, "size")
            .and_then(|s| middle(&s.value))
            .map(|w| (w, true)),
        "flower" => Some((width("size", 5.0), true)),
        "wall" => Some((width("thickness", WALL_THICKNESS_MM), false)),
        _ => None,
    }
}

/// Mm at the reference square, in squares.
fn squares(mm: f64) -> f64 {
    mm / dt_kit::REFERENCE_SQUARE_MM
}

/// The nodes within `radius` squares of `centre`, on `frame`'s lattice: the
/// disc of a thing placed there.
#[must_use]
pub fn disc(frame: &Frame, centre: [f64; 2], radius: f64) -> Mask {
    let per = f64::from(frame.cells_per_square);
    let (nx, ny) = frame.nodes();
    let mut m = Mask::new(nx, ny, false);
    let (cx, cy, r) = (centre[0] * per, centre[1] * per, radius * per);
    let range = |c: f64, n: usize| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let lo = (c - r).floor().max(0.0) as usize;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hi = ((c + r).ceil().max(0.0) as usize).min(n - 1);
        lo..=hi
    };
    for j in range(cy, ny) {
        for i in range(cx, nx) {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = (i as f64 - cx, j as f64 - cy);
            if dx * dx + dy * dy <= r * r {
                m.bits[j * nx + i] = true;
            }
        }
    }
    m
}

/// The nodes within `half` squares of the polyline `pts`: the strip of a
/// wall along its line.
#[must_use]
pub fn strip(frame: &Frame, pts: &[[f64; 2]], half: f64) -> Mask {
    let per = f64::from(frame.cells_per_square);
    let (nx, ny) = frame.nodes();
    let mut m = Mask::new(nx, ny, false);
    let r = half * per;
    for w in pts.windows(2).chain(
        // a lone point is a disc
        (pts.len() == 1).then_some(&[pts[0], pts[0]][..]),
    ) {
        let (a, b) = (
            [w[0][0] * per, w[0][1] * per],
            [w[1][0] * per, w[1][1] * per],
        );
        let range = |lo: f64, hi: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let l = (lo.min(hi) - r).floor().max(0.0) as usize;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let h = ((lo.max(hi) + r).ceil().max(0.0) as usize).min(n - 1);
            l..=h
        };
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let len2 = ex * ex + ey * ey;
        for j in range(a[1], b[1], ny) {
            for i in range(a[0], b[0], nx) {
                #[allow(clippy::cast_precision_loss)]
                let (px, py) = (i as f64 - a[0], j as f64 - a[1]);
                let t = if len2 > 0.0 {
                    ((px * ex + py * ey) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (dx, dy) = (px - t * ex, py - t * ey);
                if dx * dx + dy * dy <= r * r {
                    m.bits[j * nx + i] = true;
                }
            }
        }
    }
    m
}

/// A mask's bounding box in squares, `None` for an empty one.
fn bbox_of(frame: &Frame, m: &Mask) -> Option<BBox> {
    let per = f64::from(frame.cells_per_square);
    let (mut i0, mut j0, mut i1, mut j1) = (usize::MAX, usize::MAX, 0, 0);
    let mut any = false;
    for j in 0..m.ny {
        let row = &m.bits[j * m.nx..(j + 1) * m.nx];
        if let Some(a) = row.iter().position(|&b| b) {
            let b = row.iter().rposition(|&b| b).unwrap_or(a);
            any = true;
            i0 = i0.min(a);
            i1 = i1.max(b);
            j0 = j0.min(j);
            j1 = j1.max(j);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    any.then(|| BBox {
        x0: i0 as f64 / per,
        y0: j0 as f64 / per,
        x1: i1 as f64 / per,
        y1: j1 as f64 / per,
    })
}

/// A node's index for a point in squares, clamped to the lattice.
fn node_at(frame: &Frame, xy: [f64; 2]) -> (usize, usize) {
    let per = f64::from(frame.cells_per_square);
    let (nx, ny) = frame.nodes();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let k = |v: f64, n: usize| (v * per).round().clamp(0.0, (n - 1) as f64) as usize;
    (k(xy[0], nx), k(xy[1], ny))
}

impl RegionTree {
    /// The tile's node.
    pub const TILE: usize = 0;

    /// The regions of `tile` on `frame`'s lattice.
    #[must_use]
    pub fn build(tile: &Tile, frame: &Frame) -> Self {
        let mut nodes = vec![Node {
            label: "$tile".to_owned(),
            kind: Kind::Tile,
            parent: None,
            entry: None,
            mask: None,
            bbox: Some(BBox {
                x0: 0.0,
                y0: 0.0,
                x1: f64::from(tile.cols),
                y1: f64::from(tile.rows),
            }),
            radius: None,
        }];
        // the tile's named regions, then their parents by fill point
        let places = Places::new(tile, frame);
        let (nx, ny) = frame.nodes();
        let first = nodes.len();
        for r in &tile.regions {
            let mask = places
                .region(r)
                .unwrap_or_else(|_| Mask::new(nx, ny, false));
            nodes.push(Node {
                label: r.name.clone(),
                kind: if r.strip { Kind::Strip } else { Kind::Named },
                parent: Some(Self::TILE),
                entry: None,
                bbox: bbox_of(frame, &mask),
                mask: Some(mask),
                radius: None,
            });
        }
        let counts: Vec<usize> = nodes[first..]
            .iter()
            .map(|n| n.mask.as_ref().map_or(0, Mask::count))
            .collect();
        for (k, r) in tile.regions.iter().enumerate() {
            if r.strip {
                continue;
            }
            let Some(fill) = r.fills.first() else {
                continue;
            };
            let (i, j) = node_at(frame, *fill);
            // the smallest region that holds the fill point and is larger
            // than this one (equal regions are siblings: neither holds the
            // other, so no two can hold each other)
            let parent = (0..counts.len())
                .filter(|&o| o != k && counts[o] > counts[k])
                .filter(|&o| {
                    nodes[first + o]
                        .mask
                        .as_ref()
                        .is_some_and(|m| m.bits[j * m.nx + i])
                })
                .min_by_key(|&o| (counts[o], o));
            if let Some(o) = parent {
                nodes[first + k].parent = Some(first + o);
            }
        }
        let region_node = |name: &str| {
            tile.regions
                .iter()
                .position(|r: &Region| r.name == name)
                .map(|k| first + k)
        };
        let mut tree = Self {
            nodes,
            of_entry: Vec::new(),
            of_child: Vec::new(),
            frame: *frame,
        };
        for (i, e) in tile.stack.iter().enumerate() {
            let base = e
                .area
                .as_ref()
                .and_then(|a| region_node(&a.value))
                .unwrap_or(Self::TILE);
            let own = tree.entry_node(tile, i, e, base, &region_node);
            tree.of_entry.push(own);
            let kids = e
                .children
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let radius = footprint(&c.what.value, &c.controls)
                        .filter(|(_, disc)| *disc)
                        .map(|(w, _)| squares(w) / 2.0);
                    tree.push(Node {
                        label: format!("{} (entry {}, thing {})", c.what.value, i + 1, k + 1),
                        kind: Kind::Placed,
                        parent: Some(own),
                        entry: Some(i),
                        mask: None,
                        bbox: None,
                        radius,
                    })
                })
                .collect();
            tree.of_child.push(kids);
        }
        tree
    }

    fn push(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// The region entry `i` acts in or makes: a thing at a point makes its
    /// disc, a wall its strip; a scatter along a line has the line's strip;
    /// the rest have the region they are written in.
    fn entry_node(
        &mut self,
        tile: &Tile,
        i: usize,
        e: &Entry,
        base: usize,
        region_node: &dyn Fn(&str) -> Option<usize>,
    ) -> usize {
        let frame = self.frame;
        let what = e.what.value.as_str();
        if what == "scatter" {
            return e
                .line
                .as_deref()
                .and_then(|l| region_node(&strip_name(l)))
                .unwrap_or(base);
        }
        let Some((width, is_disc)) = footprint(what, &e.controls) else {
            return base;
        };
        let radius = squares(width) / 2.0;
        let label = format!("{what} (entry {})", i + 1);
        if is_disc {
            let Some(at) = &e.at else {
                return base;
            };
            let mid = |d: &dt_elements::Draw| 0.5 * (d.bounds().0 + d.bounds().1);
            let mask = disc(&frame, [mid(&at[0]), mid(&at[1])], radius);
            return self.push(Node {
                label,
                kind: Kind::Disc,
                parent: Some(base),
                entry: Some(i),
                bbox: bbox_of(&frame, &mask),
                mask: Some(mask),
                radius: Some(radius),
            });
        }
        let pts = e
            .along
            .clone()
            .or_else(|| e.line.as_ref().and_then(|l| tile.lines.get(l).cloned()));
        let Some(pts) = pts else {
            return base;
        };
        let mask = strip(&frame, &pts, radius);
        self.push(Node {
            label,
            kind: Kind::Run,
            parent: Some(base),
            entry: Some(i),
            bbox: bbox_of(&frame, &mask),
            mask: Some(mask),
            radius: Some(radius),
        })
    }

    /// The region containing `id`; `None` for the tile.
    #[must_use]
    pub fn parent(&self, id: usize) -> Option<usize> {
        self.nodes[id].parent
    }

    /// How many regions lie above `id` before the tile ends the chain: 0 for
    /// the tile, 1 for a region whose parent is the tile.  This is how many
    /// times `parent(…)` can be taken from `id`.
    #[must_use]
    pub fn parents(&self, id: usize) -> usize {
        let mut n = 0;
        let mut at = id;
        while let Some(p) = self.nodes[at].parent {
            n += 1;
            at = p;
        }
        n
    }

    /// Whether the point (squares from the south-west corner) lies in `id`.
    #[must_use]
    pub fn contains(&self, id: usize, xy: [f64; 2]) -> bool {
        match &self.nodes[id].mask {
            Some(m) => {
                let (i, j) = node_at(&self.frame, xy);
                m.bits[j * m.nx + i]
            }
            None => self.nodes[id].kind == Kind::Tile,
        }
    }

    /// The region's nodes on the lattice (`None` for the tile, which is all
    /// of them, and for a placed thing's, made where it is placed).
    #[must_use]
    pub fn mask(&self, id: usize) -> Option<&Mask> {
        self.nodes[id].mask.as_ref()
    }

    /// The region's bounding box in squares.
    #[must_use]
    pub fn bbox(&self, id: usize) -> Option<BBox> {
        self.nodes[id].bbox
    }

    /// The nodes under `id`'s parent, for the tree printout, in order.
    fn children(&self, id: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.nodes.len()).filter(move |&k| self.nodes[k].parent == Some(id))
    }

    /// The tree as `check --explain` prints it: each region, what it is made
    /// from and its parent, then the region each entry acts in.
    #[must_use]
    pub fn explain(&self, tile: &Tile) -> Vec<String> {
        let mut out = vec!["regions".to_owned()];
        let mut stack = vec![(Self::TILE, 0_usize)];
        while let Some((id, depth)) = stack.pop() {
            let n = &self.nodes[id];
            let parent = n
                .parent
                .map_or("none".to_owned(), |p| self.nodes[p].label.clone());
            let size = n
                .radius
                .map(|r| format!(", {:.1} mm across", 2.0 * r * dt_kit::REFERENCE_SQUARE_MM))
                .unwrap_or_default();
            out.push(format!(
                "  {}{}: {}{size}, parent {parent}",
                "  ".repeat(depth),
                n.label,
                n.kind.word(),
            ));
            let kids: Vec<usize> = self.children(id).collect();
            stack.extend(kids.into_iter().rev().map(|k| (k, depth + 1)));
        }
        for (i, e) in tile.stack.iter().enumerate() {
            if let Some(&id) = self.of_entry.get(i) {
                out.push(format!(
                    "  entry {} `{}` acts in {}",
                    i + 1,
                    e.what.value,
                    self.nodes[id].label
                ));
            }
        }
        out
    }
}

impl RegionTree {
    /// The region `$region` is for a formula's owner.
    #[must_use]
    pub fn owner(&self, o: Owner) -> usize {
        match o.child {
            Some(k) => self.of_child[o.entry][k],
            None => self.of_entry[o.entry],
        }
    }

    /// Where `$region` and its parents lead from `id`, for a message.
    fn chain(&self, id: usize) -> String {
        let mut names = vec![format!("`{}`", self.nodes[id].label)];
        let mut at = id;
        while let Some(p) = self.nodes[at].parent {
            names.push(format!("`{}`", self.nodes[p].label));
            at = p;
        }
        names.join(" in ")
    }

    /// Every `parent(…)` a formula calls that reaches past the tile, at the
    /// formula, before any build: what each placement can read is the
    /// parents it has (a tree in a scatter in a region has two below the
    /// tile's root; a thing at tile level has none).
    #[must_use]
    pub fn parent_flaws(&self, tile: &Tile) -> Vec<Diag> {
        use dt_doc::consts::ParentFlaw;
        let mut out = Vec::new();
        for r in &tile.region_reads {
            let own = self.owner(r.owner);
            let parents = self.parents(own);
            let Some(flaw) = r.table.parent_flaw(&r.expr, parents) else {
                continue;
            };
            let at = narrow(r, parents).unwrap_or(r.span);
            let here = self.chain(own);
            let (message, label, help) = match flaw {
                ParentFlaw::Tile => (
                    "the tile has no parent".to_owned(),
                    "the tile is the root",
                    format!(
                        "`$tile` is the root of the regions; `$region` here is {here}, and `parent($region)` is the region containing it"
                    ),
                ),
                ParentFlaw::Past { parents: 0, .. } => (
                    "the tile has no parent".to_owned(),
                    "here `$region` is the tile",
                    format!(
                        "this is called with `$region` the tile itself ({here}), which has no parent; put the thing in a scatter or a named region to give it one"
                    ),
                ),
                ParentFlaw::Past { hops, parents } => (
                    format!("`parent` reaches past the tile: {hops} up, and there are {parents}"),
                    "no region above the tile",
                    format!(
                        "this is called with `$region` {here}: {parents} parent{} before the tile, and the tile has no parent",
                        if parents == 1 { "" } else { "s" }
                    ),
                ),
            };
            out.push(Diag::at(&r.src, at, "dt::region", message, label).help(help));
        }
        out
    }
}

/// The text of the first `parent(…)` in a formula that has no parent where
/// it is called, as a span inside the formula's own, when it is written
/// there (not reached through a name).
fn narrow(r: &crate::doc::RegionRead, parents: usize) -> Option<miette::SourceSpan> {
    let text = r.src.inner();
    let (off, len) = (r.span.offset(), r.span.len());
    let whole = text.get(off..off + len)?;
    let mut from = 0;
    while let Some(i) = whole[from..].find("parent(") {
        let start = from + i;
        let mut depth = 0_usize;
        let mut end = None;
        for (k, c) in whole[start..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + k + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end?;
        if let Ok(e) = dt_doc::expr::parse_v2(&whole[start..end])
            && r.table.parent_flaw(&e, parents).is_some()
        {
            return Some(miette::SourceSpan::new((off + start).into(), end - start));
        }
        from = start + "parent(".len();
    }
    None
}
