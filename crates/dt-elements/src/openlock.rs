//! `connector` for OpenLOCK: the clip-socket base under z = 0.
//!
//! OpenLOCK credit: the clip
//! socket dimensions and placement follow OpenSCAD-OpenLock by Caitlyn
//! Byrne (<https://github.com/caitlynb/OpenSCAD-OpenLock>); the OpenLOCK
//! tile system is by Printable Scenery.
//!
//! The base is an 8 mm slab under the whole tile. Every square on the tile
//! perimeter has a T-shaped clip socket in its outer edge (14 mm at the
//! mouth, stepping to 12 and 10 mm, 7 mm deep), with two 4.7 mm
//! retention slots 9 mm deep either side of it, all 4.2 mm tall from
//! 1.4 mm above the bottom. Interior square boundaries stay solid: one
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
    /// Retention slot centre from the socket centre, along the edge.
    pub retention_offset_mm: f64,
    /// Retention slot width along the edge.
    pub retention_width_mm: f64,
    /// Retention slot depth into the tile.
    pub retention_depth_mm: f64,
    /// Logo inset depth.
    pub logo_depth_mm: f64,
}

impl Default for OpenLock {
    fn default() -> Self {
        Self {
            depth_mm: 8.0,
            socket_mm: 4.2,
            socket_floor_mm: 1.4,
            retention_offset_mm: 8.35,
            retention_width_mm: 4.7,
            retention_depth_mm: 9.0,
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

impl OpenLock {
    /// Every cutter outline for a `w` × `h` mm tile of `cols` × `rows`
    /// squares of `s` mm: sockets and retention slots on the perimeter.
    fn sockets(&self, cols: u32, rows: u32, s: f64, w: f64, h: f64) -> Vec<Vec<[f64; 2]>> {
        let mut out = Vec::new();
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
        for (n, map) in edges {
            for i in 0..n {
                let c = (f64::from(i) + 0.5) * s;
                out.push(T_POLY.iter().map(|p| map(c, p[0], p[1])).collect());
                for sign in [-1.0, 1.0] {
                    let uc = sign * self.retention_offset_mm;
                    let (u0, u1) = (
                        uc - self.retention_width_mm / 2.0,
                        uc + self.retention_width_mm / 2.0,
                    );
                    let d = self.retention_depth_mm;
                    out.push(vec![
                        map(c, u0, 0.0),
                        map(c, u1, 0.0),
                        map(c, u1, d),
                        map(c, u0, d),
                    ]);
                }
            }
        }
        out
    }
}

impl OpenLock {
    /// What a cut into this base may not reach, keeping `wall_mm` (the
    /// printer's wall) from it: the base walls round the footprint, and
    /// every clip socket and retention slot with that wall round it. A cut
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
        let (z0, z1) = (
            bottom + self.socket_floor_mm - wall_mm,
            bottom + self.socket_floor_mm + self.socket_mm + wall_mm,
        );
        for c in self.sockets(frame.cols, frame.rows, frame.square_mm, w, h) {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in &c {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            keep(
                [lo[0] - wall_mm, lo[1] - wall_mm, z0],
                [hi[0] + wall_mm, hi[1] + wall_mm, z1],
                "a clip socket or retention slot",
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
        let (z0, z1) = (
            bottom + self.socket_floor_mm,
            bottom + self.socket_floor_mm + self.socket_mm,
        );
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
            let o = Outline { contours: vec![c] };
            cuts.push(extrude(slots.get(k), &o, 0.0, z0, z1).map_err(LookError::build)?);
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
    fn a_1x1_has_four_sockets_and_eight_retention_slots() {
        let ol = OpenLock::default();
        let c = ol.sockets(1, 1, 25.4, 25.4, 25.4);
        assert_eq!(c.len(), 12);
        // every cutter opens its edge: some point lies just outside the tile
        for p in &c {
            assert!(
                p.iter()
                    .any(|q| q[0] < 0.0 || q[1] < 0.0 || q[0] > 25.4 || q[1] > 25.4)
            );
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
            under: None,
            edge_mm: None,
            calm_mm: None,
            draft: false,
        };
        OpenLock::default().build(&mut scene, &ctx).unwrap();
        scene.parts()[0].solid.volume()
    }

    #[test]
    fn the_base_volume_is_unchanged() {
        // the base at 25.4 mm: 3018.506 mm³ (1×1) and
        // 15769.212 mm³ (2×2); the logo's curves are flattened a little
        // differently, so within 0.2%
        for (c, r, py) in [(1, 1, 3018.506), (2, 2, 15769.212)] {
            let v = base_volume(c, r);
            assert!((v - py).abs() / py < 0.002, "{c}x{r}: {v} vs {py}");
        }
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
        // (retention slots reach 9 mm in, the socket 7)
        assert!(base::refuse_cut(&keep, &cut(12.7, 12.7, 2.0, -8.0)).is_none());
        // into a base wall, and into a socket's mouth
        assert!(base::refuse_cut(&keep, &cut(0.5, 12.7, 1.0, -3.0)).is_some());
        assert!(base::refuse_cut(&keep, &cut(12.7, 4.0, 1.0, -6.0)).is_some());
        // a socket has its band of height: a wall's thickness above it a
        // shallow cut over it is clear
        assert!(base::refuse_cut(&keep, &cut(12.7, 4.0, 1.0, -1.0)).is_none());
    }

    #[test]
    fn the_slab_is_the_rounded_footprint() {
        // 3018.506 mm³ less the four corners' cut-off, 0.36² (1 - π/4)
        // times the 8 mm depth, 0.0279 mm³ each, to the polygon
        let v = base_volume(1, 1);
        assert!((v - 3018.506).abs() / 3018.506 < 0.002, "{v}");
    }

    #[test]
    fn interior_square_boundaries_stay_solid() {
        // a 2×1 tile: 2 sockets each on north and south, 1 each on east and west
        let c = OpenLock::default().sockets(2, 1, 25.4, 50.8, 25.4);
        assert_eq!(c.len(), 6 * 3);
    }
}
