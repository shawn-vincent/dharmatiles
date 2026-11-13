//! The stack mesher: the terrain's lift. An ordered list of layers (each a
//! top field over the same lattice, lowest first) becomes ONE closed mesh.
//! The outside is the visible top, each triangle carries its layer's run,
//! and the sides are panels painted by the layers they cross.
//!
//! - **One layer** is the adaptive lift of [`crate::lift`]: its leaves,
//!   conforming corners and fans, the rim, the walls down to z = 0 and the
//!   bottom fan. A tile's flat faces are [`Sides::flat`].
//! - **Visible layer.** A node shows layer `k` when `k` is the last layer
//!   is present there ([`Layer::present`]) and whose top exceeds the shown
//!   top below it by more than [`SHOW_OVER_MM`] (layer 0 is always shown
//!   otherwise). A layer that is
//!   absent somewhere says so; the mesher does not infer it from a gap.
//! - **Forced single cells.** A leaf whose nodes show more than one layer
//!   is split to single cells, so every boundary runs along cell edges and
//!   diagonals (the lift's 2:1 balance grades the rest).
//! - **Contour splits.** A triangle whose nodes show different layers is
//!   cut along straight segments between its edge crossings. A crossing is
//!   cached per grid edge and shared by both triangles of the edge; one
//!   within [`SNAP_MM`] of a node is that node.
//! - **Rounding.** With [`Round::F32`] every vertex is rounded to float32
//!   once and shared by index, so the mesh is watertight after an STL
//!   reload with no repair. [`Round::None`] keeps the lift's f64
//!   positions.
//! - **Sides.** Each rim node has a column of heights, one per layer
//!   (the layer's top, kept between the layer below's and the visible top),
//!   and the panel between two columns is split by layer: a layer's panel
//!   is painted with that layer's run and carries that layer's relief
//!   ([`Layer::relief`]). A rim edge a boundary crosses gets the crossing
//!   as a rim vertex, unless it snapped onto a node.

use dt_lang::smooth;
use std::collections::HashMap;

use crate::TriMesh;
use crate::grid::{Grid, LiftError};
use crate::lift::{Corners, Leaf, centre, plan_leaves};

/// A side's faces are held this far inside the overhang angle, degrees. The
/// pass down a column limits the lean from vertex to vertex along it, but a
/// triangle between two columns whose setbacks differ also leans across, so
/// its face is steeper than the column's slope: this is the room left for
/// that (the unit test below measures every face against the full angle).
const OVERHANG_MARGIN_DEG: f64 = 1.5;

/// How much a layer must stand over the shown top below it to be shown, mm:
/// a sliver thinner than this is not a layer (it would be a face of no
/// height). Where a layer is absent is [`Layer::present`]'s to say.
const SHOW_OVER_MM: f64 = 1e-4;

/// Side relief turns round a corner over this distance, mm: it is set back
/// behind both planes at the corner and behind its own alone beyond
/// ([`Sides::corner_blend_mm`], as the terrain's lift sets it).
pub const CORNER_BLEND_MM: f64 = 2.0;

/// A crossing this close to a node is the node, mm.
const SNAP_MM: f64 = 1e-4;

/// How vertices are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    /// Positions as the lift computes them (f64).
    None,
    /// Every position to float32, once; the mesh carries a tolerance of
    /// about 1e-7 of the tile's size, as a float32 import does.
    F32,
}

/// One layer: its top at every node, the original id of its run, where it
/// exists and the relief of its side panel.
#[derive(Clone, Copy)]
pub struct Layer<'a> {
    /// The layer's top, `nx * ny` values in mm. Where the layer is absent
    /// its top still sets where a boundary crosses an edge to a node that
    /// shows it: the boundary is interpolated to where this top would
    /// reach the layer below's.
    pub top: &'a [f32],
    /// The run's original id (a slot id).
    pub id: u32,
    /// Where the layer exists, `nx * ny` flags; `None` is everywhere. A
    /// layer shows only where it is present and its top is over the shown
    /// top below it.
    pub present: Option<&'a [bool]>,
    /// The relief of the layer's side panel (`None`: a flat face).
    pub relief: Option<LayerRelief<'a>>,
}

impl<'a> Layer<'a> {
    /// A layer present everywhere with flat sides.
    #[must_use]
    pub const fn new(top: &'a [f32], id: u32) -> Self {
        Self {
            top,
            id,
            present: None,
            relief: None,
        }
    }
}

/// The lattice and the knobs of the lift.
#[derive(Clone, Copy, Debug)]
pub struct Lattice {
    /// Nodes across.
    pub nx: usize,
    /// Nodes up.
    pub ny: usize,
    /// Node spacing, mm.
    pub step: f64,
    /// The adaptive lift's error bound, mm.
    pub tol: f64,
    /// The widest leaf, in cells.
    pub max_cells: usize,
    /// How positions are rounded.
    pub round: Round,
}

/// How far the face is drawn in at distance `t` mm from the edge of a
/// rounded rim of radius `round` mm: the quarter circle's drop, `round` at
/// `t = 0` and 0 from `t = round` on. The plan's rounded corners (set back
/// by it in [`Sides`]) and the top's edge-round (a drop of the top, see the
/// kit's `lay`) are this one profile.
#[must_use]
pub fn rim_profile(t: f64, round: f64) -> f64 {
    if t >= round {
        0.0
    } else {
        round - libm::sqrt((round * round - (round - t) * (round - t)).max(0.0))
    }
}

/// The relief of one layer's side panel.
#[derive(Clone, Copy)]
pub struct LayerRelief<'a> {
    /// The most the face is set back behind its plane, mm.
    pub depth: f64,
    /// The relief's shape, 0 (on the plane) to 1 (set back by `depth`), at
    /// a point of the tile's side (x, y, z mm). A function of the point
    /// alone, so it runs on round every corner without a seam.
    pub shape: &'a (dyn Fn([f64; 3]) -> f64 + Sync),
}

/// How the tile's four sides are made: each a height field over its
/// vertical plane (a displacement behind the plane and never past it), on
/// columns shared with the top's rim.
#[derive(Clone, Copy)]
pub struct Sides {
    /// Nodes per column along a side: the rim's leaves are no wider.
    pub column_nodes: usize,
    /// The rows a panel's relief is sampled at are about this far apart, mm.
    pub row_mm: f64,
    /// A layer's relief fades to its boundaries over this height, mm.
    pub fade_mm: f64,
    /// No face leans out over the one below it by more than this, degrees
    /// from vertical.
    pub overhang_deg: f64,
    /// The four vertical corners are rounded in plan by this, mm.
    pub corner_round_mm: f64,
    /// Near a corner a face is set back behind both planes: the setback
    /// turns from its own plane's to the diagonal's over this distance from
    /// the corner, mm, so the relief runs on round the corner as one surface.
    pub corner_blend_mm: f64,
}

impl Sides {
    /// Flat faces straight down to z = 0 and square corners: no relief, a
    /// rim of any leaf width.
    #[must_use]
    pub const fn flat() -> Self {
        Self {
            column_nodes: usize::MAX,
            row_mm: 1.0,
            fade_mm: 1.0,
            overhang_deg: 55.0,
            corner_round_mm: 0.0,
            corner_blend_mm: 1.0,
        }
    }
}

/// The shown layer at every node.
fn classes(layers: &[Layer], n: usize) -> Vec<u8> {
    (0..n)
        .map(|k| {
            let mut c = 0;
            for (l, layer) in layers.iter().enumerate().skip(1) {
                if layer.present.is_none_or(|p| p[k])
                    && f64::from(layer.top[k]) - f64::from(layers[c].top[k]) > SHOW_OVER_MM
                {
                    c = l;
                }
            }
            u8::try_from(c).unwrap_or(0)
        })
        .collect()
}

fn u(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The stack mesher's state: the shown layer at every node, the vertices
/// made so far (a node's vertex once, a boundary crossing once per grid
/// edge) and the triangles of each layer. Its steps run in the order
/// [`lift_stack`] names them, and each takes vertices in that order, so the
/// mesh is the same whatever the thread count.
struct Mesher<'l, 'a> {
    layers: &'l [Layer<'a>],
    nx: usize,
    ny: usize,
    step: f64,
    round: Round,
    /// The shown layer at every node.
    class: Vec<u8>,
    /// The shown top at every node.
    top: Vec<f32>,
    /// A node's vertex, `u32::MAX` until made.
    index: Vec<u32>,
    positions: Vec<[f64; 3]>,
    /// The vertex where the boundary crosses grid edge `(a, b)`, `a < b`.
    cuts: HashMap<(usize, usize), u32>,
    /// The triangles of each layer.
    tris: Vec<Vec<[u32; 3]>>,
    /// The corners' plan round, mm (0: square corners).
    corner_round: f64,
}

impl<'l, 'a> Mesher<'l, 'a> {
    /// Step 0: the shown layer and top at every node, and the checks.
    fn new(lat: &Lattice, layers: &'l [Layer<'a>]) -> Result<Self, LiftError> {
        let (nx, ny) = (lat.nx, lat.ny);
        let n = nx * ny;
        if nx < 2 || ny < 2 || layers.is_empty() || layers.iter().any(|l| l.top.len() != n) {
            return Err(LiftError::TooSmall);
        }
        let nl = layers.len();
        if nl > usize::from(u8::MAX) {
            return Err(LiftError::TooManyLayers(nl));
        }
        let class = classes(layers, n);
        let top: Vec<f32> = (0..n).map(|k| layers[class[k] as usize].top[k]).collect();
        if nl > 1 {
            // the bottom layer carries the floor wall: it must be above 0 everywhere
            for (k, &z) in layers[0].top.iter().enumerate() {
                if !(z.is_finite() && z > 0.0) {
                    return Err(LiftError::BelowBase {
                        at: (k % nx, k / nx),
                        z,
                    });
                }
            }
        }
        Ok(Self {
            layers,
            nx,
            ny,
            step: lat.step,
            round: lat.round,
            class,
            top,
            index: vec![u32::MAX; n],
            positions: Vec::new(),
            cuts: HashMap::new(),
            tris: vec![Vec::new(); nl],
            corner_round: 0.0,
        })
    }

    /// A position rounded as the lattice says.
    #[allow(clippy::cast_possible_truncation)]
    fn rd(&self, v: f64) -> f64 {
        match self.round {
            Round::None => v,
            Round::F32 => f64::from(v as f32),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn node_xy(&self, k: usize) -> [f64; 2] {
        let (i, j) = (k % self.nx, k / self.nx);
        let p = [i as f64 * self.step, j as f64 * self.step];
        let r = self.corner_round;
        if r <= 0.0 || (i != 0 && i != self.nx - 1 && j != 0 && j != self.ny - 1) {
            return p;
        }
        // a rim node near a corner moves in along the corner's arc: the
        // corner itself to the arc's middle, its neighbours by the arc's
        // set-back at their distance (the rounded corner of a plan)
        let (ti, tj) = (
            i.min(self.nx - 1 - i) as f64 * self.step,
            j.min(self.ny - 1 - j) as f64 * self.step,
        );
        let sx = if i == 0 { 1.0 } else { -1.0 };
        let sy = if j == 0 { 1.0 } else { -1.0 };
        let on_x = i == 0 || i == self.nx - 1;
        let on_y = j == 0 || j == self.ny - 1;
        if on_x && on_y {
            let d = r * (1.0 - core::f64::consts::FRAC_1_SQRT_2);
            [p[0] + sx * d, p[1] + sy * d]
        } else if on_y {
            [p[0], p[1] + sy * rim_profile(ti, r)]
        } else {
            [p[0] + sx * rim_profile(tj, r), p[1]]
        }
    }

    /// Step 1: the leaves of the adaptive lift of the shown top, a leaf
    /// forced to a single cell where its nodes show several layers
    /// (summed-area tables of the nodes showing each layer above the first).
    #[allow(clippy::cast_possible_truncation)]
    fn plan(
        &self,
        tol: f64,
        max_cells: usize,
        sides: &Sides,
    ) -> Result<(Grid, Vec<Leaf>), LiftError> {
        let (nx, ny) = (self.nx, self.ny);
        let g = Grid {
            nx,
            ny,
            step: self.step,
            z: self.top.clone(),
        };
        let sw = nx + 1;
        let sats: Vec<Vec<u32>> = (1..self.layers.len())
            .map(|layer| {
                let mut sat = vec![0u32; sw * (ny + 1)];
                for j in 0..ny {
                    for i in 0..nx {
                        sat[(j + 1) * sw + i + 1] =
                            u32::from(usize::from(self.class[j * nx + i]) == layer)
                                + sat[j * sw + i + 1]
                                + sat[(j + 1) * sw + i]
                                - sat[j * sw + i];
                    }
                }
                sat
            })
            .collect();
        // the rim's leaves are no wider than a column, and the corner's
        // are single cells (its round is made of rim nodes)
        let column = sides.column_nodes.max(1);
        let round_nodes =
            usize::try_from(libm::ceil(sides.corner_round_mm / self.step) as i64).unwrap_or(0);
        let force = |l: Leaf| {
            let (south, north) = (l.j0 == 0, l.j1 == ny - 1);
            let (west, east) = (l.i0 == 0, l.i1 == nx - 1);
            if ((south || north) && l.i1 - l.i0 > column)
                || ((west || east) && l.j1 - l.j0 > column)
                || ((l.i0 < round_nodes || l.i1 + round_nodes >= nx)
                    && (l.j0 < round_nodes || l.j1 + round_nodes >= ny))
            {
                return true;
            }
            let area = ((l.i1 - l.i0 + 1) * (l.j1 - l.j0 + 1)) as u32;
            sats.iter().any(|sat| {
                let c = sat[(l.j1 + 1) * sw + l.i1 + 1] + sat[l.j0 * sw + l.i0]
                    - sat[l.j0 * sw + l.i1 + 1]
                    - sat[(l.j1 + 1) * sw + l.i0];
                c > 0 && c < area
            })
        };
        let leaves = plan_leaves(&g, tol, max_cells, &force)?;
        Ok((g, leaves))
    }

    /// The vertex of node `k`, made on first use.
    fn vid(&mut self, k: usize) -> u32 {
        if self.index[k] == u32::MAX {
            self.index[k] = u(self.positions.len());
            let p = self.node_xy(k);
            self.positions
                .push([self.rd(p[0]), self.rd(p[1]), f64::from(self.top[k])]);
        }
        self.index[k]
    }

    /// The vertex where the boundary crosses grid edge `(a, b)`, shared by
    /// both triangles of the edge; one within [`SNAP_MM`] of a node is that
    /// node.
    fn cut(&mut self, a: usize, b: usize) -> u32 {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        if let Some(&v) = self.cuts.get(&(a, b)) {
            return v;
        }
        let layers = self.layers;
        let (lo, hi) = (
            self.class[a].min(self.class[b]) as usize,
            self.class[a].max(self.class[b]) as usize,
        );
        let d = |k: usize| f64::from(layers[hi].top[k]) - f64::from(layers[lo].top[k]);
        let (da, db) = (d(a), d(b));
        let t = (da / (da - db)).clamp(0.0, 1.0);
        let t = if t.is_finite() { t } else { 0.5 };
        let (pa, pb) = (self.node_xy(a), self.node_xy(b));
        let len = libm::hypot(pb[0] - pa[0], pb[1] - pa[1]);
        let v = if t * len < SNAP_MM {
            self.vid(a)
        } else if (1.0 - t) * len < SNAP_MM {
            self.vid(b)
        } else {
            let z = f64::from(layers[lo].top[a])
                + t * (f64::from(layers[lo].top[b]) - f64::from(layers[lo].top[a]));
            let v = u(self.positions.len());
            let p = [
                self.rd(pa[0] + t * (pb[0] - pa[0])),
                self.rd(pa[1] + t * (pb[1] - pa[1])),
                self.rd(z),
            ];
            self.positions.push(p);
            v
        };
        self.cuts.insert((a, b), v);
        v
    }

    /// A triangle of `layer`, unless it is degenerate.
    fn push(&mut self, t: [u32; 3], layer: u8) {
        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
            self.tris[layer as usize].push(t);
        }
    }

    /// A triangle of grid nodes, cut along the boundary if it crosses it.
    fn emit(&mut self, nodes: [usize; 3]) {
        let c = nodes.map(|k| self.class[k]);
        let v: [u32; 3] = [self.vid(nodes[0]), self.vid(nodes[1]), self.vid(nodes[2])];
        if c[0] == c[1] && c[1] == c[2] {
            self.push(v, c[0]);
        } else if c[0] != c[1] && c[1] != c[2] && c[0] != c[2] {
            // three layers: a corner at each node, the middle to the highest
            let m01 = self.cut(nodes[0], nodes[1]);
            let m12 = self.cut(nodes[1], nodes[2]);
            let m20 = self.cut(nodes[2], nodes[0]);
            self.push([v[0], m01, m20], c[0]);
            self.push([v[1], m12, m01], c[1]);
            self.push([v[2], m20, m12], c[2]);
            self.push([m01, m12, m20], c[0].max(c[1]).max(c[2]));
        } else {
            // two layers: rotate so the lone node is first
            let lone = if c[1] == c[2] {
                0
            } else if c[0] == c[2] {
                1
            } else {
                2
            };
            let (x, y, z) = (lone, (lone + 1) % 3, (lone + 2) % 3);
            let cxy = self.cut(nodes[x], nodes[y]);
            let cxz = self.cut(nodes[x], nodes[z]);
            self.push([v[x], cxy, cxz], c[x]);
            self.push([cxy, v[y], v[z]], c[y]);
            self.push([cxy, v[z], cxz], c[y]);
        }
    }

    /// Step 2, the top: every leaf, a cell as two triangles and a larger
    /// uniform leaf as a fan from its centre.
    fn top_surface(&mut self, g: &Grid, leaves: &[Leaf], verts: &Corners) {
        let nx = self.nx;
        for &l in leaves {
            let ring = verts.ring(l);
            let nodes: Vec<usize> = ring.iter().map(|&(i, j)| j * nx + i).collect();
            if nodes.len() == 4 {
                self.emit([nodes[0], nodes[1], nodes[2]]);
                self.emit([nodes[0], nodes[2], nodes[3]]);
            } else {
                // a leaf with more is uniform (a mixed one is a cell): a fan
                let layer = self.class[nodes[0]];
                let ids: Vec<u32> = nodes.iter().map(|&k| self.vid(k)).collect();
                let c = centre(g, l);
                let cv = u(self.positions.len());
                let p = [self.rd(c[0]), self.rd(c[1]), self.rd(c[2])];
                self.positions.push(p);
                for k in 0..ids.len() {
                    self.tris[layer as usize].push([cv, ids[k], ids[(k + 1) % ids.len()]]);
                }
            }
        }
    }

    /// Steps 3 and 4 with side panels: the rim columns, each a vertex per
    /// layer boundary and rows between them where a layer has relief, the
    /// floor, and between two columns a panel per layer, zipped so the
    /// columns' different rows meet without a T-junction.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_cmp,
        clippy::too_many_lines,
        clippy::many_single_char_names
    )]
    fn side_panels(&mut self, verts: &Corners, sides: &Sides) {
        let nl = self.layers.len();
        let whole = Leaf {
            i0: 0,
            j0: 0,
            i1: self.nx - 1,
            j1: self.ny - 1,
        };
        let ring: Vec<usize> = verts
            .ring(whole)
            .iter()
            .map(|&(i, j)| j * self.nx + i)
            .collect();
        let slope = libm::tan((sides.overhang_deg - OVERHANG_MARGIN_DEG).to_radians());
        struct Col {
            /// Vertices of each layer's strip, from its lower boundary up.
            strips: Vec<Vec<u32>>,
            floor: u32,
        }
        let mut cols: Vec<Col> = Vec::new();
        let m = ring.len();
        for k in 0..m {
            let (a, b) = (ring[k], ring[(k + 1) % m]);
            let t_vertex = self.vid(a);
            let p = self.node_xy(a);
            let vis = f64::from(self.top[a]);
            let xy = [self.rd(p[0]), self.rd(p[1])];
            // heights of the layer boundaries at this column
            let mut hs = vec![0.0_f64];
            for (l, layer) in self.layers.iter().enumerate() {
                let hl = if l >= self.class[a] as usize {
                    vis
                } else {
                    hs[l].max(f64::from(layer.top[a]).min(vis))
                };
                hs.push(hl);
            }
            let floor = u(self.positions.len());
            self.positions.push([xy[0], xy[1], 0.0]);
            // how the face is set back at this column: behind its own plane
            // by the relief's depth, and near a corner behind the other
            // plane too (by the depth times a blend that is whole at the
            // corner), so the relief turns the corner as one surface
            let (ix, iy) = (a % self.nx, a / self.nx);
            let (on_x, on_y) = (ix == 0 || ix == self.nx - 1, iy == 0 || iy == self.ny - 1);
            let blend = |dist: f64| smooth(1.0 - dist / sides.corner_blend_mm.max(1e-9));
            let to_x = ix.min(self.nx - 1 - ix) as f64 * self.step;
            let to_y = iy.min(self.ny - 1 - iy) as f64 * self.step;
            let (gx, gy) = (
                if on_x { 1.0 } else { blend(to_x) },
                if on_y { 1.0 } else { blend(to_y) },
            );
            let (sx, sy) = (
                if ix == 0 || (!on_x && ix < self.nx / 2) {
                    1.0
                } else {
                    -1.0
                },
                if iy == 0 || (!on_y && iy < self.ny / 2) {
                    1.0
                } else {
                    -1.0
                },
            );
            // the setback is `u` along the face's own normal, turning to the
            // diagonal at a corner, where it is split over both planes
            // (`u / reach²` along each, `reach` = the hypotenuse of the two
            // blends = √2 there): its run is `u / reach`, never more than
            // `u`, and 1/√2 of it at the corner, so what stands over the
            // corner overhangs it no more than a side
            let reach = libm::hypot(gx, gy);
            let mut strips: Vec<Vec<u32>> = Vec::with_capacity(nl);
            // vertices top-down are needed for the slope pass: build the
            // heights and relief first, bottom-up
            struct Row {
                z: f64,
                u: f64,
            }
            let mut flat: Vec<(usize, Vec<Row>)> = Vec::new();
            for l in 0..nl {
                let (lo, hi) = (hs[l], hs[l + 1]);
                let rel = self.layers[l].relief;
                let n = match rel {
                    Some(_) if hi - lo > 1e-6 => {
                        ((hi - lo) / sides.row_mm).ceil().max(1.0) as usize
                    }
                    _ => 1,
                };
                let f = sides.fade_mm.min(0.5 * (hi - lo)).max(1e-9);
                let rows: Vec<Row> = (0..=n)
                    .map(|q| {
                        let z = lo + (hi - lo) * q as f64 / n as f64;
                        let u = match rel {
                            Some(r) if q > 0 && q < n => {
                                let s = (r.shape)([xy[0], xy[1], z]).clamp(0.0, 1.0);
                                let fz = smooth((z - lo) / f) * smooth((hi - z) / f);
                                r.depth * s * fz
                            }
                            _ => 0.0,
                        };
                        Row { z, u }
                    })
                    .collect();
                flat.push((l, rows));
            }
            // one pass down the column: no face leans out over the one
            // below by more than the overhang
            let mut next: Option<(f64, f64)> = None; // (z, u) of the vertex above
            for (_, rows) in flat.iter_mut().rev() {
                for row in rows.iter_mut().rev() {
                    if let Some((z, uu)) = next
                        && row.z < z
                    {
                        row.u = row.u.min(uu + slope * (z - row.z));
                    }
                    next = Some((row.z, row.u));
                }
            }
            for (l, rows) in &flat {
                let mut ids = Vec::with_capacity(rows.len());
                for (q, row) in rows.iter().enumerate() {
                    let id = if *l == 0 && q == 0 {
                        floor
                    } else if q == 0 {
                        // the layer's lower boundary is the one below's upper
                        strips[l - 1][strips[l - 1].len() - 1]
                    } else if row.z >= vis && q == rows.len() - 1 {
                        t_vertex
                    } else if q == rows.len() - 1 && row.z == rows[q - 1].z && *l > 0 {
                        ids[q - 1]
                    } else {
                        let v = u(self.positions.len());
                        self.positions.push([
                            self.rd(xy[0] + row.u * gx / (reach * reach) * sx),
                            self.rd(xy[1] + row.u * gy / (reach * reach) * sy),
                            self.rd(row.z),
                        ]);
                        v
                    };
                    ids.push(id);
                }
                strips.push(ids);
            }
            cols.push(Col { strips, floor });
            if self.class[a] != self.class[b] {
                let c = self.cut(a, b);
                if c != self.vid(a) && c != self.vid(b) {
                    let q = self.positions[c as usize];
                    let floor = u(self.positions.len());
                    self.positions.push([q[0], q[1], 0.0]);
                    let mut strips = vec![vec![floor, c]];
                    strips.extend((1..nl).map(|_| vec![c]));
                    cols.push(Col { strips, floor });
                }
            }
        }
        let mid = u(self.positions.len());
        let centre = [
            self.rd(0.5 * (self.nx - 1) as f64 * self.step),
            self.rd(0.5 * (self.ny - 1) as f64 * self.step),
            0.0,
        ];
        self.positions.push(centre);
        let mc = cols.len();
        for k in 0..mc {
            let k1 = (k + 1) % mc;
            for l in 0..nl {
                let (a, b) = (&cols[k].strips[l], &cols[k1].strips[l]);
                let layer = u8::try_from(l).unwrap_or(u8::MAX);
                let (n0, n1) = (a.len() - 1, b.len() - 1);
                let (mut i, mut j) = (0usize, 0usize);
                while i < n0 || j < n1 {
                    let advance_a = if i == n0 {
                        false
                    } else if j == n1 {
                        true
                    } else {
                        (i + 1) * n1 <= (j + 1) * n0
                    };
                    if advance_a {
                        self.push([a[i + 1], a[i], b[j]], layer);
                        i += 1;
                    } else {
                        self.push([a[i], b[j], b[j + 1]], layer);
                        j += 1;
                    }
                }
            }
            self.tris[0].push([mid, cols[k1].floor, cols[k].floor]);
        }
    }

    /// Step 5: one run per layer that made triangles, in layer order.
    #[allow(clippy::cast_precision_loss)]
    fn finish(self) -> TriMesh {
        let mut out = TriMesh {
            positions: self.positions,
            ..TriMesh::default()
        };
        out.run_index = vec![0];
        for (l, t) in self.tris.iter().enumerate() {
            out.triangles.extend(t.iter().copied());
            if !t.is_empty() {
                out.run_original_id.push(self.layers[l].id);
                out.run_index.push(u(out.triangles.len() * 3));
            }
        }
        if self.round == Round::F32 {
            // a float32 import carries about 1e-7 of the tile's size
            out.tolerance = 1e-7 * self.step * (self.nx.max(self.ny) - 1) as f64;
        }
        out
    }
}

/// The stack lift: the leaves of the shown top ([`Mesher::plan`]), the top
/// surface, then the floor and the side panels ([`Sides`]: a height field
/// over each vertical plane, on columns shared with the top's rim; flat
/// faces straight down to z = 0 are [`Sides::flat`]), one run per layer.
///
/// # Errors
/// [`LiftError`] when the lattice is smaller than 2 × 2, a field has the
/// wrong length, or a shown node (or a bottom layer's node) is not above 0,
/// or there are more than 255 layers.
pub fn lift_stack(lat: &Lattice, layers: &[Layer], sides: &Sides) -> Result<TriMesh, LiftError> {
    let mut m = Mesher::new(lat, layers)?;
    m.corner_round = sides.corner_round_mm;
    let (g, leaves) = m.plan(lat.tol, lat.max_cells, sides)?;
    let verts = Corners::new(lat.nx, lat.ny, &leaves);
    m.top_surface(&g, &leaves, &verts);
    m.side_panels(&verts, sides);
    Ok(m.finish())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::float_cmp
    )]
    use super::*;
    use crate::Solid;
    use std::collections::HashSet;

    fn lat(nx: usize, ny: usize, round: Round) -> Lattice {
        Lattice {
            nx,
            ny,
            step: 0.25,
            tol: 0.02,
            max_cells: 16,
            round,
        }
    }

    fn field(nx: usize, ny: usize, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
        (0..nx * ny)
            .map(|k| f((k % nx) as f32 * 0.25, (k / nx) as f32 * 0.25))
            .collect()
    }

    fn reload(m: &TriMesh) -> TriMesh {
        let mut f = m.clone();
        for p in &mut f.positions {
            *p = p.map(|v| f64::from(v as f32));
        }
        f
    }

    #[test]
    fn one_layer_rejects_a_node_not_above_zero() {
        let mut e = vec![1.0f32; 16];
        e[5] = 0.0;
        let l = Lattice {
            nx: 4,
            ny: 4,
            ..lat(4, 4, Round::None)
        };
        assert!(matches!(
            lift_stack(&l, &[Layer::new(&e, 1)], &Sides::flat()),
            Err(LiftError::BelowBase { at: (1, 1), .. })
        ));
    }

    fn two_layers() -> (usize, usize, Vec<f32>, Vec<f32>) {
        let (nx, ny) = (81, 61);
        let e = field(nx, ny, |x, y| 3.0 + 0.5 * (x * 0.4).sin() * (y * 0.3).cos());
        let w = field(nx, ny, |x, _| 3.2 + 0.05 * (x * 2.0).sin() - 0.09 * x);
        (nx, ny, e, w)
    }

    #[test]
    fn two_layers_are_watertight_painted_and_survive_the_float32_reload() {
        let (nx, ny, e, w) = two_layers();
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer::new(&e, 7), Layer::new(&w, 9)],
            &Sides::flat(),
        )
        .unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![7, 9]);
        assert_eq!(reload(&m).positions, m.positions);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 2);
    }

    #[test]
    fn three_layers_meet_watertight() {
        let (nx, ny) = (81, 61);
        let a = field(nx, ny, |x, y| 3.0 + 0.4 * (x * 0.5).sin() * (y * 0.4).cos());
        let b = field(nx, ny, |x, _| 3.3 + 0.05 * (x * 2.0).sin() - 0.1 * x);
        let c = field(nx, ny, |_, y| 2.2 + 0.12 * y);
        let layers = [Layer::new(&a, 1), Layer::new(&b, 2), Layer::new(&c, 3)];
        let m = lift_stack(&lat(nx, ny, Round::F32), &layers, &Sides::flat()).unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![1, 2, 3]);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 3);
    }

    /// Every cell a boundary crosses is a single cell.
    #[test]
    fn boundary_cells_are_single_cells() {
        let (nx, ny, e, w) = two_layers();
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer::new(&e, 7), Layer::new(&w, 9)],
            &Sides::flat(),
        )
        .unwrap();
        let (w, h) = (
            f64::from(nx as u32 - 1) * 0.25,
            f64::from(ny as u32 - 1) * 0.25,
        );
        let inside =
            |p: &[f64; 3]| p[0] > 1e-9 && p[0] < w - 1e-9 && p[1] > 1e-9 && p[1] < h - 1e-9;
        // the vertices both runs use are the boundary's
        let run_of = |t: usize| usize::from(t * 3 >= m.run_index[1] as usize);
        let mut used = [HashSet::new(), HashSet::new()];
        for (t, tri) in m.triangles.iter().enumerate() {
            used[run_of(t)].extend(tri.iter().copied());
        }
        let mut crossings = 0;
        for tri in &m.triangles {
            let p = tri.map(|i| m.positions[i as usize]);
            if p.iter().all(inside)
                && tri
                    .iter()
                    .any(|i| used[0].contains(i) && used[1].contains(i))
            {
                crossings += 1;
                // a boundary triangle is of single cells: no edge is longer
                // than a cell's diagonal
                for k in 0..3 {
                    let (a, b) = (p[k], p[(k + 1) % 3]);
                    let len = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
                    assert!(len <= 0.25 * 2.0_f64.sqrt() + 1e-4, "{p:?}");
                }
            }
        }
        assert!(crossings > 20);
    }

    /// Runs stay apart across coplanar faces through a union.
    #[test]
    fn runs_survive_a_union() {
        let (nx, ny) = (41, 41);
        let w = vec![3.2f32; nx * ny];
        let mut e = vec![3.2f32; nx * ny];
        for j in 0..ny {
            for i in 21..nx {
                e[j * nx + i] = 2.0;
            }
        }
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer::new(&e, 7), Layer::new(&w, 9)],
            &Sides::flat(),
        )
        .unwrap();
        assert!(m.is_watertight());
        let t = Solid::from_tri_mesh(&m).unwrap();
        let boxm = |x0: f64, x1: f64, z1: f64| {
            let b = Solid(manifold_csg::Manifold::cube(x1 - x0, 2.0, z1 - 1.0, false));
            let b = Solid(b.0.translate(x0, 3.0, 1.0));
            Solid::from_tri_mesh(&b.to_tri_mesh())
                .unwrap()
                .with_original_id(1)
                .unwrap()
        };
        let o = t
            .union(&boxm(1.0, 3.0, 5.0))
            .union(&boxm(4.0, 6.5, 6.0))
            .to_tri_mesh();
        assert!(o.is_watertight());
        let ids = o.triangle_original_ids().unwrap();
        assert!(ids.contains(&7) && ids.contains(&9));
    }

    fn lumps(p: [f64; 3]) -> f64 {
        // sharp, deep, uneven lumps: the worst case for the one pass
        let a = (p[0] * 2.7 + p[1] * 1.9).sin() * (p[2] * 3.9).cos();
        if a > 0.0 { 1.0 } else { 0.0 }
    }

    fn sides() -> Sides {
        Sides {
            column_nodes: 4,
            row_mm: 0.3,
            fade_mm: 0.4,
            overhang_deg: 55.0,
            corner_round_mm: 0.5,
            corner_blend_mm: CORNER_BLEND_MM,
        }
    }

    fn with_sides(round: Round) -> (TriMesh, usize, usize) {
        let (nx, ny, e, w) = two_layers();
        let relief = LayerRelief {
            depth: 0.6,
            shape: &lumps,
        };
        let m = lift_stack(
            &lat(nx, ny, round),
            &[
                Layer {
                    relief: Some(relief),
                    ..Layer::new(&e, 7)
                },
                Layer::new(&w, 9),
            ],
            &sides(),
        )
        .unwrap();
        (m, nx, ny)
    }

    /// Side panels close the mesh, keep the layers' runs and survive the
    /// float32 reload and a union.
    #[test]
    fn side_panels_are_watertight_painted_and_survive_the_float32_reload() {
        let (m, ..) = with_sides(Round::F32);
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![7, 9]);
        assert_eq!(reload(&m).positions, m.positions);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 2);
    }

    /// The corners are one surface: with relief running round all four, the
    /// closed mesh is a single sphere (Euler characteristic 2 over its
    /// vertices, with every edge shared by two triangles), before and after a
    /// trip through the kernel.
    #[test]
    fn the_corners_are_one_watertight_surface() {
        let (m, ..) = with_sides(Round::F32);
        let sphere = |m: &TriMesh| {
            assert!(m.is_watertight());
            let mut used = HashSet::new();
            for t in &m.triangles {
                used.extend(t.iter().copied());
            }
            // V - E + F with E = 3F / 2 on a closed mesh
            used.len() as i64 - (m.triangles.len() as i64) / 2
        };
        assert_eq!(sphere(&m), 2);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert_eq!(sphere(&out), 2);
    }

    /// Relief only sets a face back: nothing is past a plane, the floor and
    /// the corner columns are where the base needs them, and no face leans
    /// out over the one below past the overhang.
    #[test]
    fn side_relief_stays_behind_the_plane_and_under_the_overhang() {
        let (m, nx, ny) = with_sides(Round::F32);
        let (w, h) = ((nx - 1) as f64 * 0.25, (ny - 1) as f64 * 0.25);
        for p in &m.positions {
            assert!(
                p[0] >= 0.0 && p[0] <= w && p[1] >= 0.0 && p[1] <= h,
                "{p:?}"
            );
            if p[2] == 0.0 && (p[0], p[1]) != (0.5 * w, 0.5 * h) {
                // the floor's ring is the plane (the corners rounded in)
                assert!(
                    p[0] < 0.51
                        || p[0] > w - 0.51
                        || p[1] == 0.0
                        || p[1] == h
                        || p[0] == 0.0
                        || p[0] == w,
                    "{p:?}"
                );
            }
        }
        // some of each face is set back, so the relief is really there
        let set_back = m
            .positions
            .iter()
            .filter(|p| p[2] > 0.3 && p[1] > 0.05 && p[1] < 0.7 && p[0] > 2.0 && p[0] < w - 2.0)
            .count();
        assert!(set_back > 20);
        // no triangle whose three vertices are above the floor faces down
        // more than the overhang allows
        let limit = -libm::sin(55.0_f64.to_radians());
        for t in &m.triangles {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            if a[2].min(b[2]).min(c[2]) <= 0.0 {
                continue;
            }
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-9 {
                assert!(
                    n[2] / len >= limit - 1e-6,
                    "{a:?} {b:?} {c:?}: {}",
                    n[2] / len
                );
            }
        }
    }

    /// A shore that lands exactly on nodes adds no second rim vertex and no
    /// duplicate vertex: the mesh stays closed with every vertex used.
    #[test]
    fn a_crossing_snapped_onto_a_node_adds_no_rim_vertex() {
        let (nx, ny) = (17, 9);
        // earth falls from 2 to 1 across x = 1 mm; the water sits flat at
        // 1.5: the shore is where earth = 1.5 (a node column at x = 2 since
        // earth is 2 left of it, exactly 1.5 on the node, 1 right of it)
        let e = field(nx, ny, |x, _| {
            if x < 2.0 {
                2.0
            } else if (x - 2.0).abs() < 1e-6 {
                1.5
            } else {
                1.0
            }
        });
        let w = vec![1.5f32; nx * ny];
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer::new(&e, 1), Layer::new(&w, 2)],
            &Sides::flat(),
        )
        .unwrap();
        assert!(m.is_watertight());
        // no two vertices are one point (a second rim vertex at the shore
        // would sit on the node's), and the shore is on the node column
        let mut seen = std::collections::HashSet::new();
        for p in &m.positions {
            assert!(seen.insert(p.map(f64::to_bits)), "duplicate vertex {p:?}");
        }
        assert!(
            m.positions
                .iter()
                .any(|p| p[0] == 2.0 && p[1] == 0.0 && p[2] == 1.5)
        );
        // every vertex is used
        let mut used = vec![false; m.positions.len()];
        for t in &m.triangles {
            for &i in t {
                used[i as usize] = true;
            }
        }
        assert!(used.iter().all(|&u| u));
    }
}
