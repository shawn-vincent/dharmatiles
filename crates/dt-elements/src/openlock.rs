//! `connector` for OpenLOCK: the clip-socket base under z = 0.
//!
//! OpenLOCK credit: the clip socket's dimensions and placement are taken
//! from the geometry of OpenSCAD-OpenLock by Caitlyn Byrne
//! (<https://github.com/caitlynb/OpenSCAD-OpenLock>, CC BY-NC 4.0), measured
//! on its output; no code is copied. The OpenLOCK tile system is by
//! Printable Scenery.
//!
//! The base is an 8 mm slab under the whole tile. Sockets sit on the grid
//! lines between squares, as on OpenSCAD-OpenLock's parts: along a side
//! n squares long, one at each of its n - 1 interior lines, or one at the
//! middle when n is 1 (a 2×2 has one per side, a 4×2 three along its long
//! sides and one at the middle of its short ones). Each socket is a clip
//! socket in the tile's outer edge: a T-shaped slot (14 mm at
//! the mouth, stepping to 12 and 10 mm, 7 mm deep), 4.2 mm tall from 1.4 mm
//! above the bottom, and behind it a pocket for the clip's head, centred on
//! the socket: 18 mm along the edge, 4.7 mm deep (6.0 to 10.7 mm in from the
//! face), open through the bottom of the base up to the socket's top, with a
//! 14 mm-wide part of it reaching 0.3 mm higher, so its roof is bridged one
//! layer above the slot's. Interior square boundaries stay solid: one
//! merged N×M tile, not N×M separate ones. The lotus logo is inset 0.4 mm
//! into the bottom. The base never depends on the content, so unlike the
//! DungeonBlocks peg it has one height.

use dt_kit::Kind;
use dt_kit::{Frame, Material, Outline, Scene, Tag, carve, extrude};

use crate::base::{self, Box3, KeepOut};

use crate::{Ctx, Element, LookError, Plan, Read, Write};

/// The clip constants, mm. These are the system's own sizes and do
/// not scale: a clip is the same clip on every tile.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenLock {
    /// Slab thickness below z = 0 (`WALL_HEIGHT`).
    pub depth_mm: f64,
    /// Socket height (`CUTOUT_HEIGHT`).
    pub socket_mm: f64,
    /// Socket floor above the base bottom (`CUTOUT_START_Z`).
    pub socket_floor_mm: f64,
    /// Pocket start, in from the face.
    pub pocket_start_mm: f64,
    /// Pocket depth (how far it reaches into the tile from its start).
    pub pocket_depth_mm: f64,
    /// Pocket width along the edge.
    pub pocket_width_mm: f64,
    /// Width along the edge of the pocket's raised part.
    pub pocket_roof_width_mm: f64,
    /// How far the raised part reaches above the socket's top.
    pub pocket_roof_rise_mm: f64,
    /// Logo inset depth.
    pub logo_depth_mm: f64,
}

impl Default for OpenLock {
    fn default() -> Self {
        Self {
            depth_mm: 8.0,
            socket_mm: 4.2,
            socket_floor_mm: 1.4,
            pocket_start_mm: 6.0,
            pocket_depth_mm: 4.7,
            pocket_width_mm: 18.0,
            pocket_roof_width_mm: 14.0,
            pocket_roof_rise_mm: 0.3,
            logo_depth_mm: 0.4,
        }
    }
}

/// The T-shaped socket in `(along the edge from its centre, depth in from
/// the face)`: 14 mm wide for the first 2 mm, 12 mm to 5 mm deep, then
/// 10 mm to the back wall at 7 mm (`_T_POLY`).
const T_POLY: [[f64; 2]; 10] = [
    [-7.0, 0.0],
    [-7.0, 2.0],
    [-6.0, 2.0],
    [-5.0, 5.0],
    [-5.0, 7.0],
    [5.0, 7.0],
    [5.0, 5.0],
    [6.0, 2.0],
    [7.0, 2.0],
    [7.0, 0.0],
];

/// The logo (the DungeonBlocks connector's).
const LOGO: &str = include_str!("logo.svgpath");
const LOGO_VIEWBOX: f64 = 1024.0;
const LOGO_TOL: f64 = 1.5;
/// Cutters overshoot the faces they open by this much, so no face is
/// coplanar with the slab's.
const EPS: f64 = 0.01;

/// One cutter: a plan outline cut between two heights above the base bottom.
struct Cutter {
    outline: Vec<[f64; 2]>,
    /// Bottom of the cut; below zero is through the bottom.
    z0: f64,
    z1: f64,
}

/// Where the sockets sit along a side `n` squares long of `s` mm squares,
/// from its start: on the `n - 1` interior grid lines (the squares' joins),
/// or at the middle when the side is one square. A tile of any size then
/// meets its neighbour socket to socket on the same 12.7 mm-grid lines
/// as the OpenSCAD-OpenLock parts E, R, S, SA and SB.
fn socket_centres(n: u32, s: f64) -> Vec<f64> {
    if n <= 1 {
        vec![s / 2.0]
    } else {
        (1..n).map(|i| f64::from(i) * s).collect()
    }
}

impl OpenLock {
    /// Every cutter for a `w` × `h` mm tile of `cols` × `rows` squares of
    /// `s` mm: on each perimeter square the T slot, the pocket behind it and
    /// the pocket's raised part.
    fn sockets(&self, cols: u32, rows: u32, s: f64, w: f64, h: f64) -> Vec<Cutter> {
        let mut out = Vec::new();
        let top = self.socket_floor_mm + self.socket_mm;
        // (u along the edge, v in from the face) → plan, for each edge
        #[allow(clippy::type_complexity)]
        let edges: [(u32, &dyn Fn(f64, f64, f64) -> [f64; 2]); 4] = [
            (cols, &|c, u, v| {
                [c + u, v - if v == 0.0 { EPS } else { 0.0 }]
            }),
            (cols, &|c, u, v| {
                [c + u, h - v + if v == 0.0 { EPS } else { 0.0 }]
            }),
            (rows, &|c, u, v| {
                [v - if v == 0.0 { EPS } else { 0.0 }, c + u]
            }),
            (rows, &|c, u, v| {
                [w - v + if v == 0.0 { EPS } else { 0.0 }, c + u]
            }),
        ];
        let (v0, v1) = (
            self.pocket_start_mm,
            self.pocket_start_mm + self.pocket_depth_mm,
        );
        for (n, map) in edges {
            for c in socket_centres(n, s) {
                out.push(Cutter {
                    outline: T_POLY.iter().map(|p| map(c, p[0], p[1])).collect(),
                    z0: self.socket_floor_mm,
                    z1: top,
                });
                for (width, z1) in [
                    (self.pocket_width_mm, top),
                    (self.pocket_roof_width_mm, top + self.pocket_roof_rise_mm),
                ] {
                    let u = width / 2.0;
                    out.push(Cutter {
                        outline: vec![map(c, -u, v0), map(c, u, v0), map(c, u, v1), map(c, -u, v1)],
                        z0: -EPS,
                        z1,
                    });
                }
            }
        }
        out
    }
}

impl OpenLock {
    /// What a cut into this base may not reach, keeping `wall_mm` (the
    /// printer's wall) from it: the base walls round the footprint, and
    /// every clip socket and pocket with that wall round it. A cut
    /// may go through the slab to the table anywhere else.
    #[must_use]
    pub fn keep_out(&self, frame: &Frame, wall_mm: f64) -> Vec<KeepOut> {
        let [w, h] = frame.size_mm();
        let bottom = -self.depth_mm;
        let mut out = Vec::new();
        let mut keep = |min: [f64; 3], max: [f64; 3], what| {
            out.push(KeepOut {
                at: Box3 { min, max },
                what,
            });
        };
        let wall = "the base walls";
        keep([0.0, 0.0, bottom], [wall_mm, h, 0.0], wall);
        keep([w - wall_mm, 0.0, bottom], [w, h, 0.0], wall);
        keep([0.0, 0.0, bottom], [w, wall_mm, 0.0], wall);
        keep([0.0, h - wall_mm, bottom], [w, h, 0.0], wall);
        for c in self.sockets(frame.cols, frame.rows, frame.square_mm, w, h) {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in &c.outline {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            // a pocket is open to the bottom: its keep-out reaches the table
            let z0 = if c.z0 < 0.0 {
                bottom
            } else {
                bottom + c.z0 - wall_mm
            };
            keep(
                [lo[0] - wall_mm, lo[1] - wall_mm, z0],
                [hi[0] + wall_mm, hi[1] + wall_mm, bottom + c.z1 + wall_mm],
                "a clip socket or its pocket",
            );
        }
        out
    }
}

impl Element for OpenLock {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Connector),
            reads: vec![Read::AllSolids],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let f = *scene.frame();
        let [w, h] = f.size_mm();
        let bottom = -self.depth_mm;
        let cutters = self.sockets(f.cols, f.rows, f.square_mm, w, h);
        let logo = Outline::svg_path(LOGO, LOGO_TOL).map_err(LookError::build)?;
        let side = 22.0_f64.min(w.min(h) * 0.75);
        let logo = logo.placed(LOGO_VIEWBOX, w / 2.0, h / 2.0, side);
        let n = u32::try_from(cutters.len() + 2).unwrap_or(u32::MAX);
        let slots = scene
            .slots(n, Material::Connector)
            .map_err(LookError::build)?;
        // the slab is the tile's footprint at zero, corners rounded
        let slab = base::footprint(&f);
        let body = extrude(slots.get(0), &slab, 0.0, bottom, 0.0).map_err(LookError::build)?;
        let mut cuts = Vec::with_capacity(cutters.len() + 1);
        for (k, c) in cutters.into_iter().enumerate() {
            let k = u32::try_from(k + 1).unwrap_or(u32::MAX);
            let o = Outline {
                contours: vec![c.outline],
            };
            cuts.push(
                extrude(slots.get(k), &o, 0.0, bottom + c.z0, bottom + c.z1)
                    .map_err(LookError::build)?,
            );
        }
        cuts.push(
            extrude(
                slots.get(n - 1),
                &logo,
                0.0,
                bottom - EPS,
                bottom + self.logo_depth_mm,
            )
            .map_err(LookError::build)?,
        );
        let slab = carve(&body, &cuts).map_err(LookError::build)?;
        scene.add(
            Tag {
                kind: Kind::Connector,
                owner: ctx.path.to_owned(),
            },
            slab,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_1x1_has_four_sockets_each_with_a_pocket_and_its_raised_part() {
        let ol = OpenLock::default();
        let c = ol.sockets(1, 1, 25.4, 25.4, 25.4);
        assert_eq!(c.len(), 12);
        // each socket's T opens its edge: some point lies just outside the tile
        for t in c.iter().step_by(3) {
            assert!(
                t.outline
                    .iter()
                    .any(|q| q[0] < 0.0 || q[1] < 0.0 || q[0] > 25.4 || q[1] > 25.4)
            );
        }
        // the pockets sit 6.0 to 10.7 mm in, 18 and 14 mm wide, the pocket
        // through the bottom and its raised part 0.3 mm over the socket top
        let south = &c[0..3];
        assert_eq!((south[0].z0, south[0].z1), (1.4, 5.6));
        for (p, wide, top) in [(&south[1], 18.0, 5.6), (&south[2], 14.0, 5.9)] {
            let ys: Vec<f64> = p.outline.iter().map(|q| q[1]).collect();
            let xs: Vec<f64> = p.outline.iter().map(|q| q[0]).collect();
            assert!((ys.iter().cloned().fold(f64::MIN, f64::max) - 10.7).abs() < 1e-9);
            assert!((ys.iter().cloned().fold(f64::MAX, f64::min) - 6.0).abs() < 1e-9);
            let span = xs.iter().cloned().fold(f64::MIN, f64::max)
                - xs.iter().cloned().fold(f64::MAX, f64::min);
            assert!((span - wide).abs() < 1e-9);
            assert!(p.z0 < 0.0 && (p.z1 - top).abs() < 1e-9);
        }
    }

    /// The base alone, as built into an empty scene.
    fn base_volume(cols: u32, rows: u32) -> f64 {
        let frame = dt_kit::Frame::openlock(cols, rows);
        let mut scene = Scene::new(frame, dt_kit::Seed::new(1));
        let area = scene.whole();
        let ctx = Ctx {
            area: &area,
            path: "tile/t/connector",
            seed: dt_kit::Seed::new(1),
            along: None,
            at: None,
            printer: dt_kit::Printer::new(0.45, 0.2, 55.0, 0.2),
            against: None,
            foots: &[],
            band: None,
            below: None,
            land_mm: 0.0,
            under: None,
            calm_mm: None,
            draft: false,
        };
        OpenLock::default().build(&mut scene, &ctx).unwrap();
        scene.parts()[0].solid.volume()
    }

    #[test]
    fn the_base_volume_is_the_slab_less_its_cuts() {
        // measured on the built base (T slots, pockets, raised parts and
        // the logo cut from the rounded slab): 2747.328 mm³ (1×1)
        let v = base_volume(1, 1);
        assert!((v - 2747.328).abs() / 2747.328 < 0.002, "1x1: {v}");
        let v = base_volume(2, 2);
        assert!((v - 17404.475).abs() / 17404.475 < 0.002, "2x2: {v}");
    }

    #[test]
    fn a_cut_clears_the_clips_and_the_walls() {
        let f = Frame::openlock(1, 1);
        let keep = OpenLock::default().keep_out(&f, 0.9);
        let cut = |x: f64, y: f64, half: f64, z0: f64| base::Box3 {
            min: [x - half, y - half, z0],
            max: [x + half, y + half, 1.0],
        };
        // the middle, through to the table (z = -8): clear of everything
        // (pockets reach 10.7 mm in, plus the wall)
        assert!(base::refuse_cut(&keep, &cut(12.7, 12.7, 1.0, -8.0)).is_none());
        // into a pocket, which is open to the bottom
        assert!(base::refuse_cut(&keep, &cut(12.7, 8.0, 1.0, -8.0)).is_some());
        assert!(base::refuse_cut(&keep, &cut(12.7, 8.0, 1.0, -3.0)).is_some());
        // into a base wall, and into a socket's mouth
        assert!(base::refuse_cut(&keep, &cut(0.5, 12.7, 1.0, -3.0)).is_some());
        assert!(base::refuse_cut(&keep, &cut(12.7, 4.0, 1.0, -6.0)).is_some());
        // a socket has its band of height: a wall's thickness above it a
        // shallow cut over it is clear
        assert!(base::refuse_cut(&keep, &cut(12.7, 4.0, 1.0, -1.0)).is_none());
    }

    #[test]
    fn interior_square_boundaries_stay_solid() {
        // a 2×1 tile: one socket each on north and south (on the join), one
        // each on east and west (the middle of the one-square side)
        let c = OpenLock::default().sockets(2, 1, 25.4, 50.8, 25.4);
        assert_eq!(c.len(), 4 * 3);
    }

    #[test]
    fn sockets_sit_on_the_interior_grid_lines() {
        assert_eq!(socket_centres(1, 25.4), vec![12.7]);
        assert_eq!(socket_centres(2, 25.4), vec![25.4]);
        let four = socket_centres(4, 25.4);
        assert!(
            four.iter()
                .zip([25.4, 50.8, 76.2])
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
        let ol = OpenLock::default();
        // sockets per tile: (cols, rows) -> sides of cols-1 (or 1) twice and rows-1 (or 1) twice
        for (cols, rows, n) in [
            (1, 1, 4),
            (2, 1, 4),
            (2, 2, 4),
            (4, 2, 8),
            (1, 3, 6),
            (3, 3, 8),
        ] {
            let s = 25.4;
            let c = ol.sockets(cols, rows, s, f64::from(cols) * s, f64::from(rows) * s);
            assert_eq!(c.len(), n * 3, "{cols}x{rows}");
        }
    }
}
