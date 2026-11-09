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
//! into the bottom, as large as fits at the middle without touching a pocket
//! (at most 22 mm), and left out where that is under 6 mm (a 1×1 or a 2×1). The base never depends on the content, so unlike the
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

/// A side of the tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    South,
    North,
    West,
    East,
}

impl Side {
    const ALL: [Self; 4] = [Self::South, Self::North, Self::West, Self::East];

    /// The squares along this side.
    fn squares(self, frame: &Frame) -> u32 {
        match self {
            Self::South | Self::North => frame.cols,
            Self::West | Self::East => frame.rows,
        }
    }

    /// Plan position of `u` mm along the side from its start and `v` mm in
    /// from its face; at the face itself (`v == 0`) it overshoots by [`EPS`].
    fn at(self, [w, h]: [f64; 2], u: f64, v: f64) -> [f64; 2] {
        let v_out = if v == 0.0 { EPS } else { 0.0 };
        match self {
            Self::South => [u, v - v_out],
            Self::North => [u, h - v + v_out],
            Self::West => [v - v_out, u],
            Self::East => [w - v + v_out, u],
        }
    }
}

/// One clip socket: the T slot in a side, the pocket behind it and the
/// pocket's raised part.
struct Socket {
    /// Which side it is in (the tests ask for sockets by side, not by the
    /// order they are made in).
    #[cfg_attr(not(test), allow(dead_code))]
    side: Side,
    slot: Cutter,
    pocket: Cutter,
    roof: Cutter,
}

impl Socket {
    fn cutters(&self) -> [&Cutter; 3] {
        [&self.slot, &self.pocket, &self.roof]
    }
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

/// The least a lotus is drawn at, mm: smaller, its petals are thinner than
/// the printer's line and it is left out.
const LOGO_MIN_MM: f64 = 6.0;
/// The largest lotus, mm.
const LOGO_MAX_MM: f64 = 22.0;

impl OpenLock {
    /// Every socket of a tile: on each side the T slot, the pocket behind it
    /// and the pocket's raised part.
    fn sockets(&self, frame: &Frame) -> Vec<Socket> {
        let size = frame.size_mm();
        let top = self.socket_floor_mm + self.socket_mm;
        let (v0, v1) = (
            self.pocket_start_mm,
            self.pocket_start_mm + self.pocket_depth_mm,
        );
        let mut out = Vec::new();
        for side in Side::ALL {
            for c in socket_centres(side.squares(frame), frame.square_mm) {
                let at = |u: f64, v: f64| side.at(size, c + u, v);
                let pocket = |width: f64, z1: f64| {
                    let u = width / 2.0;
                    Cutter {
                        outline: vec![at(-u, v0), at(u, v0), at(u, v1), at(-u, v1)],
                        z0: -EPS,
                        z1,
                    }
                };
                out.push(Socket {
                    side,
                    slot: Cutter {
                        outline: T_POLY.iter().map(|p| at(p[0], p[1])).collect(),
                        z0: self.socket_floor_mm,
                        z1: top,
                    },
                    pocket: pocket(self.pocket_width_mm, top),
                    roof: pocket(self.pocket_roof_width_mm, top + self.pocket_roof_rise_mm),
                });
            }
        }
        out
    }

    /// The side of the lotus on the underside, mm: the largest square about
    /// the tile's middle that no pocket reaches (the pockets are open through
    /// the bottom, where the logo is inset), at most [`LOGO_MAX_MM`]; `None`
    /// when that is under [`LOGO_MIN_MM`] (a 1×1 or a 2×1: the pockets leave
    /// only a few millimetres at the middle).
    fn logo_side(&self, frame: &Frame) -> Option<f64> {
        let [w, h] = frame.size_mm();
        let (cx, cy) = (w / 2.0, h / 2.0);
        // half the side that clears each pocket, which a square about the
        // middle reaches along x and y at once
        let mut half = LOGO_MAX_MM / 2.0;
        for s in self.sockets(frame) {
            let (lo, hi) = bounds(&s.pocket.outline);
            let dx = (lo[0] - cx).max(cx - hi[0]).max(0.0);
            let dy = (lo[1] - cy).max(cy - hi[1]).max(0.0);
            // the square overlaps the pocket unless it stops short on one axis
            half = half.min(dx.max(dy));
        }
        let side = (2.0 * half).min(w.min(h) * 0.75);
        (side >= LOGO_MIN_MM).then_some(side)
    }
}

/// The low and high corners of a plan outline.
fn bounds(outline: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in outline {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
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
        for c in self.sockets(frame).iter().flat_map(Socket::cutters) {
            let (lo, hi) = bounds(&c.outline);
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
        let sockets = self.sockets(&f);
        let cutters: Vec<&Cutter> = sockets.iter().flat_map(Socket::cutters).collect();
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
                contours: vec![c.outline.clone()],
            };
            cuts.push(
                extrude(slots.get(k), &o, 0.0, bottom + c.z0, bottom + c.z1)
                    .map_err(LookError::build)?,
            );
        }
        if let Some(side) = self.logo_side(&f) {
            let logo = Outline::svg_path(LOGO, LOGO_TOL).map_err(LookError::build)?;
            let logo = logo.placed(LOGO_VIEWBOX, w / 2.0, h / 2.0, side);
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
        }
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

    fn on(sockets: &[Socket], side: Side) -> Vec<&Socket> {
        sockets.iter().filter(|s| s.side == side).collect()
    }

    fn span(outline: &[[f64; 2]], axis: usize) -> f64 {
        let (lo, hi) = bounds(outline);
        hi[axis] - lo[axis]
    }

    #[test]
    fn a_1x1_has_four_sockets_each_with_a_pocket_and_its_raised_part() {
        let ol = OpenLock::default();
        let f = Frame::openlock(1, 1);
        let c = ol.sockets(&f);
        assert_eq!(c.len(), 4);
        // each socket's T opens its edge: some point lies just outside the tile
        for t in &c {
            assert!(
                t.slot
                    .outline
                    .iter()
                    .any(|q| q[0] < 0.0 || q[1] < 0.0 || q[0] > 25.4 || q[1] > 25.4),
                "{:?}",
                t.side
            );
        }
        // the pockets sit 6.0 to 10.7 mm in, 18 and 14 mm wide, the pocket
        // through the bottom and its raised part 0.3 mm over the socket top
        let south = on(&c, Side::South);
        assert_eq!(south.len(), 1);
        let south = south[0];
        assert_eq!((south.slot.z0, south.slot.z1), (1.4, 5.6));
        for (p, wide, top) in [(&south.pocket, 18.0, 5.6), (&south.roof, 14.0, 5.9)] {
            let (lo, hi) = bounds(&p.outline);
            assert!((hi[1] - 10.7).abs() < 1e-9);
            assert!((lo[1] - 6.0).abs() < 1e-9);
            assert!((span(&p.outline, 0) - wide).abs() < 1e-9);
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
        // snapshots of the built base (T slots, pockets, raised parts and
        // the logo cut from the rounded slab), not derived from the sizes:
        // 1x1 has no logo (the pockets leave it no room), 2x2 has one
        let v = base_volume(1, 1);
        assert!((v - 2756.218).abs() / 2756.218 < 0.002, "1x1: {v:.3}");
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
    fn a_pocket_leaves_the_tiles_other_faces_at_least_the_printers_wall() {
        // the pockets of two sides meet at a 1x1's corners (the south and
        // west pockets overlap, x and y 6.0 to 10.7 mm); what stays between
        // each pocket and the faces it does not open on is the base's wall
        let wall = 2.0 * 0.45;
        let ol = OpenLock::default();
        for (cols, rows) in [(1, 1), (2, 1), (2, 2), (4, 2)] {
            let f = Frame::openlock(cols, rows);
            let [w, h] = f.size_mm();
            for s in ol.sockets(&f) {
                let (lo, hi) = bounds(&s.pocket.outline);
                let to = |face: Side| match face {
                    Side::South => lo[1],
                    Side::North => h - hi[1],
                    Side::West => lo[0],
                    Side::East => w - hi[0],
                };
                for face in Side::ALL.into_iter().filter(|&d| d != s.side) {
                    // the opposite face is the other side's pocket, not wall
                    let across = matches!(
                        (s.side, face),
                        (Side::South, Side::North)
                            | (Side::North, Side::South)
                            | (Side::West, Side::East)
                            | (Side::East, Side::West)
                    );
                    if !across {
                        assert!(
                            to(face) >= wall,
                            "{cols}x{rows}: the {:?} pocket is {} mm from the {face:?} face",
                            s.side,
                            to(face)
                        );
                    }
                }
            }
        }
        // on a 1x1 it is 3.7 mm: the pockets are 18 mm wide on 25.4 mm
        let f = Frame::openlock(1, 1);
        let south = &ol.sockets(&f)[0];
        let (lo, _) = bounds(&south.pocket.outline);
        assert!((lo[0] - 3.7).abs() < 1e-9, "{lo:?}");
    }

    #[test]
    fn the_logo_touches_no_pocket_on_any_tile() {
        let ol = OpenLock::default();
        for (cols, rows, side) in [
            (1, 1, None),
            (2, 1, None),
            (2, 2, Some(22.0)),
            (4, 2, Some(22.0)),
            (3, 3, Some(22.0)),
        ] {
            let f = Frame::openlock(cols, rows);
            let got = ol.logo_side(&f);
            assert_eq!(got.is_some(), side.is_some(), "{cols}x{rows}: {got:?}");
            let Some(got) = got else { continue };
            assert!((got - side.unwrap()).abs() < 1e-9, "{cols}x{rows}: {got}");
            let [w, h] = f.size_mm();
            let (lo, hi) = (
                [w / 2.0 - got / 2.0, h / 2.0 - got / 2.0],
                [w / 2.0 + got / 2.0, h / 2.0 + got / 2.0],
            );
            for s in ol.sockets(&f) {
                let (plo, phi) = bounds(&s.pocket.outline);
                let overlap = (0..2).all(|k| lo[k] < phi[k] && plo[k] < hi[k]);
                assert!(
                    !overlap,
                    "{cols}x{rows}: the logo reaches the {:?} pocket",
                    s.side
                );
            }
        }
        // and where it fits, the logo is in the build: it changes the volume
        // by about its inset's worth
        let with = base_volume(2, 2);
        assert!(with < 17420.0 && with > 17380.0, "{with}");
    }

    #[test]
    fn interior_square_boundaries_stay_solid() {
        // a 2×1 tile: one socket each on north and south (on the join), one
        // each on east and west (the middle of the one-square side)
        let c = OpenLock::default().sockets(&Frame::openlock(2, 1));
        assert_eq!(c.len(), 4);
        for side in [Side::South, Side::North] {
            let at = on(&c, side);
            assert_eq!(at.len(), 1);
            let (lo, hi) = bounds(&at[0].slot.outline);
            assert!((lo[0] + hi[0]) / 2.0 - 25.4 < 1e-9);
        }
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
            let c = ol.sockets(&Frame::openlock(cols, rows));
            assert_eq!(c.len(), n, "{cols}x{rows}");
            assert_eq!(on(&c, Side::South).len(), (cols as usize - 1).max(1));
            assert_eq!(on(&c, Side::East).len(), (rows as usize - 1).max(1));
        }
    }
}
