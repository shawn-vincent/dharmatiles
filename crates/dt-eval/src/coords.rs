//! The coordinates of format 1: `x(region, unit)`, `y`, `d` and `a`, and the
//! shorthands `$d` and `$a`, worked out at a node of the lattice.
//!
//! Geometry keeps its own frame (squares from the south-west corner, `y`
//! running north); the coordinates are the language's side of the boundary:
//! `x` and `y` count from the region's bounding box's north-west corner, `y`
//! running south.
//!
//! * `d(region, %)` is 0% at the region's deepest point and 100% at its
//!   edge, scaled by the deepest part of each connected piece (a puddle
//!   beside a lake has a centre of its own); a piece no deeper than a cell
//!   counts as one cell deep, never a division by zero.  `d(region, mm)` is
//!   the distance in from the edge, 0 at the edge.
//! * `a(region, …)` runs round the piece's deepest point (the middle of the
//!   deepest ridge where there is one): 0 pointing east, turning towards the
//!   south the way the page reads.  Along a line's strip it runs from the
//!   line's first through point to its last.  A region with a hole has no
//!   "round": [`RegionField::hole`] says so, and `check` refuses `$a` there.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dt_doc::consts::ConstTable;
use dt_doc::expr::{Expr, UnitWord};
use dt_geom::Mask;

use crate::regions::{BBox, Kind, RegionTree};

/// The distance and angle fields of one region on the lattice.
#[derive(Debug)]
pub struct RegionField {
    bbox: BBox,
    per: f64,
    nx: usize,
    /// Per node: the distance in from the region's edge, in cells (0 at the
    /// edge).
    depth: Vec<f64>,
    /// Per node: its connected piece, 1-based; 0 outside the region.
    piece: Vec<u32>,
    /// Per piece: its deepest depth, cells.
    deepest: Vec<f64>,
    /// Per piece: the middle of its deepest nodes, in cells from the
    /// south-west corner.
    centre: Vec<[f64; 2]>,
    /// The line it runs along, with the cumulative length at each point.
    along: Option<(Vec<[f64; 2]>, Vec<f64>)>,
    /// True when the region has a hole: land round a pond, a ring.
    pub hole: bool,
}

impl RegionField {
    /// The fields of region `id`.
    #[must_use]
    pub fn new(tree: &RegionTree, id: usize) -> Self {
        let frame = &tree.frame;
        let (nx, ny) = frame.nodes();
        let per = f64::from(frame.cells_per_square);
        let node = &tree.nodes[id];
        let full;
        let mask: &Mask = match &node.mask {
            Some(m) => m,
            None => {
                full = Mask::new(nx, ny, true);
                &full
            }
        };
        let bbox = tree.bbox(id).unwrap_or(BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
        // distance in from the edge: to the nearest node outside, counting a
        // ring round the lattice as outside
        let (px, py) = (nx + 2, ny + 2);
        let mut outside = vec![true; px * py];
        for j in 0..ny {
            for i in 0..nx {
                outside[(j + 1) * px + i + 1] = !mask.bits[j * nx + i];
            }
        }
        let dist = dt_geom::distance(&outside, px, py);
        let mut depth = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                if mask.bits[j * nx + i] {
                    depth[j * nx + i] = (dist[(j + 1) * px + i + 1] - 1.0).max(0.0);
                }
            }
        }
        // connected pieces (4-connected)
        let mut piece = vec![0_u32; nx * ny];
        let (mut deepest, mut centre) = (Vec::new(), Vec::new());
        let mut count = 0_u32;
        for start in 0..nx * ny {
            if !mask.bits[start] || piece[start] != 0 {
                continue;
            }
            count += 1;
            let mut members = vec![start];
            piece[start] = count;
            let mut at = 0;
            while at < members.len() {
                let k = members[at];
                at += 1;
                let (i, j) = (k % nx, k / nx);
                let mut push = |n: usize| {
                    if mask.bits[n] && piece[n] == 0 {
                        piece[n] = count;
                        members.push(n);
                    }
                };
                if i > 0 {
                    push(k - 1);
                }
                if i + 1 < nx {
                    push(k + 1);
                }
                if j > 0 {
                    push(k - nx);
                }
                if j + 1 < ny {
                    push(k + nx);
                }
            }
            let top = members.iter().map(|&k| depth[k]).fold(0.0, f64::max);
            let ridge: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&k| depth[k] >= top - 1e-9)
                .collect();
            #[allow(clippy::cast_precision_loss)]
            let n = ridge.len() as f64;
            #[allow(clippy::cast_precision_loss)]
            let (sx, sy) = ridge.iter().fold((0.0, 0.0), |(a, b), &k| {
                (a + (k % nx) as f64, b + (k / nx) as f64)
            });
            deepest.push(top);
            centre.push([sx / n, sy / n]);
        }
        // a hole: a stretch of outside that does not reach the lattice's rim
        // (8-connected, as a ring's inside touches its outside only at a corner)
        let mut seen = vec![false; px * py];
        let mut queue = vec![0_usize];
        seen[0] = true;
        let mut at = 0;
        while at < queue.len() {
            let k = queue[at];
            at += 1;
            let (i, j) = (k % px, k / px);
            for nj in j.saturating_sub(1)..=(j + 1).min(py - 1) {
                for ni in i.saturating_sub(1)..=(i + 1).min(px - 1) {
                    let n = nj * px + ni;
                    if outside[n] && !seen[n] {
                        seen[n] = true;
                        queue.push(n);
                    }
                }
            }
        }
        let hole = (0..px * py).any(|k| outside[k] && !seen[k]);
        let along = node.along.as_ref().filter(|p| p.len() >= 2).map(|pts| {
            let mut acc = vec![0.0];
            for w in pts.windows(2) {
                let last = acc.last().copied().unwrap_or(0.0);
                acc.push(last + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
            }
            (pts.clone(), acc)
        });
        Self {
            bbox,
            per,
            nx,
            depth,
            piece,
            deepest,
            centre,
            along,
            hole: hole && node.kind != Kind::Tile && node.along.is_none(),
        }
    }

    /// The node's `x` from the region's north-west corner, squares.
    fn across(&self, k: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let x = (k % self.nx) as f64 / self.per;
        x - self.bbox.x0
    }

    /// The node's `y` from the region's north-west corner, running south.
    fn down(&self, k: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let y = (k / self.nx) as f64 / self.per;
        self.bbox.y1 - y
    }

    /// `d(region, %)` at a node: 0 deepest, 1 at the edge.
    fn depth_share(&self, k: usize) -> f64 {
        let p = self.piece[k] as usize;
        if p == 0 {
            return 1.0;
        }
        // a piece no deeper than a cell is one cell deep
        let deepest = self.deepest[p - 1].max(1.0);
        1.0 - self.depth[k] / deepest
    }

    /// `polar(region, d, a)`: out from the deepest point of the region's
    /// largest piece by `d` (a fraction of that piece's depth; 100% is 1),
    /// `a_deg` degrees round it (0 east, turning towards the south), in
    /// squares from the south-west corner.  `None` for a region with no
    /// ground.
    #[must_use]
    pub fn polar(&self, d: f64, a_deg: f64) -> Option<[f64; 2]> {
        let piece =
            (0..self.deepest.len()).max_by(|&a, &b| self.deepest[a].total_cmp(&self.deepest[b]))?;
        let c = self.centre[piece];
        let r = d * self.deepest[piece];
        let (sin, cos) = a_deg.to_radians().sin_cos();
        // east is +x, south is -y
        Some([(c[0] + r * cos) / self.per, (c[1] - r * sin) / self.per])
    }

    /// `a(region, …)` at a node as a fraction of a turn, 0 to 1.
    fn turn(&self, k: usize) -> f64 {
        if let Some((pts, acc)) = &self.along {
            return self.along_line(k, pts, acc);
        }
        let p = self.piece[k] as usize;
        if p == 0 {
            return 0.0;
        }
        let c = self.centre[p - 1];
        #[allow(clippy::cast_precision_loss)]
        let (i, j) = ((k % self.nx) as f64, (k / self.nx) as f64);
        // east is +x, south is -y: a quarter turn points straight down
        let angle = (c[1] - j).atan2(i - c[0]);
        let t = angle / std::f64::consts::TAU;
        if t < 0.0 { t + 1.0 } else { t }
    }

    fn along_line(&self, k: usize, pts: &[[f64; 2]], acc: &[f64]) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (
            (k % self.nx) as f64 / self.per,
            (k / self.nx) as f64 / self.per,
        );
        let total = acc.last().copied().unwrap_or(0.0);
        if total <= 0.0 {
            return 0.0;
        }
        let mut best = (f64::INFINITY, 0.0);
        for (w, a) in pts.windows(2).zip(acc) {
            let (ex, ey) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let len2 = ex * ex + ey * ey;
            let t = if len2 > 0.0 {
                (((x - w[0][0]) * ex + (y - w[0][1]) * ey) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (dx, dy) = (x - (w[0][0] + t * ex), y - (w[0][1] + t * ey));
            let d2 = dx * dx + dy * dy;
            if d2 < best.0 {
                best = (d2, a + t * len2.sqrt());
            }
        }
        best.1 / total
    }
}

/// The regions a tile's formulas read from, with each region's fields made
/// once, on demand.
pub struct Coords<'a> {
    tree: &'a RegionTree,
    square_mm: f64,
    layer_mm: f64,
    fields: RefCell<HashMap<usize, Rc<RegionField>>>,
}

impl<'a> Coords<'a> {
    /// Coordinates over `tree`, for the build target's square and layer.
    #[must_use]
    pub fn new(tree: &'a RegionTree, square_mm: f64, layer_mm: f64) -> Self {
        Self {
            tree,
            square_mm,
            layer_mm,
            fields: RefCell::new(HashMap::new()),
        }
    }

    /// The fields of region `id`.
    #[must_use]
    pub fn field(&self, id: usize) -> Rc<RegionField> {
        Rc::clone(
            self.fields
                .borrow_mut()
                .entry(id)
                .or_insert_with(|| Rc::new(RegionField::new(self.tree, id))),
        )
    }

    /// The region a region-valued expression names, where `own` is
    /// `$region`: `$tile`, `$region`, `parent(…)`, or a name that holds one.
    ///
    /// # Errors
    /// What is not a region, or a `parent` past the tile.
    pub fn region(&self, e: &Expr, own: usize, table: &ConstTable) -> Result<usize, String> {
        self.region_at(e, own, table, 0)
    }

    fn region_at(
        &self,
        e: &Expr,
        own: usize,
        table: &ConstTable,
        depth: usize,
    ) -> Result<usize, String> {
        if depth > 64 {
            return Err("names read each other in a loop".to_owned());
        }
        match e {
            Expr::Name(n) if n == "$region" => Ok(own),
            Expr::Name(n) if n == "$tile" => Ok(RegionTree::TILE),
            Expr::Name(n) => table
                .get(n)
                .ok_or_else(|| format!("`{n}` is not a region"))
                .and_then(|c| self.region_at(&c.expr, own, table, depth + 1)),
            Expr::Call { name, args } if name == "parent" && args.len() == 1 => {
                let inner = self.region_at(&args[0], own, table, depth)?;
                self.tree
                    .parent(inner)
                    .ok_or_else(|| "the tile has no parent".to_owned())
            }
            other => Err(format!("`{other}` is not a region")),
        }
    }

    /// `x(region, unit)` at node `k`.
    #[must_use]
    pub fn x(&self, id: usize, k: usize, unit: UnitWord) -> f64 {
        let f = self.field(id);
        let sq = f.across(k);
        self.in_unit(sq, f.bbox.x1 - f.bbox.x0, unit)
    }

    /// `y(region, unit)` at node `k`, counted south from the north-west corner.
    #[must_use]
    pub fn y(&self, id: usize, k: usize, unit: UnitWord) -> f64 {
        let f = self.field(id);
        let sq = f.down(k);
        self.in_unit(sq, f.bbox.y1 - f.bbox.y0, unit)
    }

    /// A length in squares counted in `unit`; `%` is a share of `extent`.
    fn in_unit(&self, sq: f64, extent: f64, unit: UnitWord) -> f64 {
        match unit {
            UnitWord::Sq => sq,
            UnitWord::Dmm => sq * dt_doc::schema::SQUARE_MM,
            UnitWord::Mm => sq * self.square_mm,
            UnitWord::In => sq * self.square_mm / 25.4,
            UnitWord::Layer => sq * self.square_mm / self.layer_mm,
            _ => {
                if extent > 0.0 {
                    sq / extent
                } else {
                    0.0
                }
            }
        }
    }

    /// `d(region, unit)` at node `k`.
    #[must_use]
    pub fn d(&self, id: usize, k: usize, unit: UnitWord) -> f64 {
        let f = self.field(id);
        if unit == UnitWord::Pct {
            return f.depth_share(k);
        }
        self.in_unit(f.depth[k] / f.per, 0.0, unit)
    }

    /// `a(region, unit)` at node `k`.
    #[must_use]
    pub fn a(&self, id: usize, k: usize, unit: UnitWord) -> f64 {
        let turn = self.field(id).turn(k);
        match unit {
            UnitWord::Deg => turn * 360.0,
            UnitWord::Rad => turn * std::f64::consts::TAU,
            _ => turn,
        }
    }
}
