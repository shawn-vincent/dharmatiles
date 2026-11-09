//! `connector`: what is under z = 0: the DungeonBlocks socket peg.
//!
//! DungeonBlocks credit: the socket-peg geometry follows the DungeonBlocks
//! blank/floor tile standard on MyMiniFactory
//! (<https://www.myminifactory.com/object/3d-print-dungeonblocks-blank-floor-tile-177592>).
//!
//! One peg per square: the full square at z = 0 flares in to a 26 mm
//! column over 5.2 mm, runs down the peg height, and bevels 1.5 mm in to
//! the tip, into which the lotus logo is inset 0.8 mm. The peg height
//! depends on the content's maximum z (`select_peg_height`), so
//! the connector reads every solid and is built last.

use dt_kit::Kind;
use dt_kit::{Frame, Material, Outline, Scene, Solid, Tag, carve, extrude, loft, union};
use rayon::prelude::*;

use crate::base::{self, Box3, KeepOut};
use crate::{Ctx, Element, LookError, Plan, Read, Write};

/// The DungeonBlocks constants, mm.
#[derive(Clone, Debug, PartialEq)]
pub struct DungeonBlocks {
    /// Column height for low content.
    pub short_peg_mm: f64,
    /// Column height when the content rises above `threshold_mm`.
    pub tall_peg_mm: f64,
    /// Content taller than this takes the tall peg.
    pub threshold_mm: f64,
    /// The flare from the full square to the column (always 5.2 for DB).
    pub flare_mm: f64,
    /// Column cross-section.
    pub column_mm: f64,
    /// Chamfer at the peg tip.
    pub bevel_mm: f64,
    /// Logo inset depth.
    pub logo_depth_mm: f64,
    /// Inward shrink of the lotus contours (not the groove).
    pub logo_clearance_mm: f64,
}

impl Default for DungeonBlocks {
    fn default() -> Self {
        Self {
            short_peg_mm: 5.7,
            tall_peg_mm: 11.4,
            threshold_mm: 15.0,
            flare_mm: 5.2,
            column_mm: 26.0,
            bevel_mm: 1.5,
            logo_depth_mm: 0.8,
            logo_clearance_mm: 0.35,
        }
    }
}

/// The logo's SVG path (`src/dharmatiles/assets/dharmatiles-logo.svg`,
/// a 1024-unit viewBox).
const LOGO: &str = include_str!("logo.svgpath");
const LOGO_VIEWBOX: f64 = 1024.0;
/// Curve flattening, in SVG units (`_parse_svg_d`'s `tol_px`).
const LOGO_TOL: f64 = 1.5;
/// The cutter starts this far below the tip, so no face is coplanar.
const LOGO_EPS_MM: f64 = 0.01;

impl DungeonBlocks {
    /// `select_peg_height`: tall when the content's top is above the
    /// threshold.
    #[must_use]
    pub fn peg_mm(&self, content_top_mm: f64) -> f64 {
        if content_top_mm > self.threshold_mm {
            self.tall_peg_mm
        } else {
            self.short_peg_mm
        }
    }

    /// The four rings of one peg, top to bottom (`make_base`). The top ring
    /// is the square's part of the tile's footprint at zero, so a corner of
    /// the tile (`round`) is rounded by `arc`; the lower rings keep the same
    /// round, so the flare and the column turn the corner with it.
    fn rings(
        &self,
        at: [f64; 2],
        s: f64,
        peg: f64,
        arc: &[[f64; 2]],
        round: [bool; 4],
    ) -> Vec<Vec<[f64; 3]>> {
        let col_inset = (s - self.column_mm) / 2.0;
        let bevel_inset = (s - (self.column_mm - 2.0 * self.bevel_mm)) / 2.0;
        let ring = |i: f64, z: f64| {
            base::ring(
                [at[0] + i, at[1] + i],
                [at[0] + s - i, at[1] + s - i],
                arc,
                round,
            )
            .into_iter()
            .map(|p| [p[0], p[1], z])
            .collect()
        };
        vec![
            ring(0.0, 0.0),
            ring(col_inset, -self.flare_mm),
            ring(col_inset, -(peg - self.bevel_mm + self.flare_mm)),
            ring(bevel_inset, -(peg + self.flare_mm)),
        ]
    }

    /// What a cut into this base may not reach, on `frame` with content up
    /// to `content_top_mm` (which picks the peg), keeping `wall_mm` (the
    /// printer's wall) from the connector's sides: for each square, the
    /// flare and the column's sides, and the column's floor (the logo's
    /// depth and a skin), so a pit stays inside the column, about 9 mm deep
    /// on the short peg and 15 mm on the tall.
    #[must_use]
    pub fn keep_out(&self, frame: &Frame, content_top_mm: f64, wall_mm: f64) -> Vec<KeepOut> {
        let s = frame.square_mm;
        let tip = self.peg_mm(content_top_mm) + self.flare_mm;
        let core = (s - self.column_mm) / 2.0 + wall_mm;
        let floor = -tip + self.logo_depth_mm * 2.0;
        let mut out = Vec::new();
        for c in 0..frame.cols {
            for r in 0..frame.rows {
                let (x, y) = (f64::from(c) * s, f64::from(r) * s);
                let (x1, y1) = (x + s, y + s);
                let mut keep = |min: [f64; 3], max: [f64; 3], what| {
                    out.push(KeepOut {
                        at: Box3 { min, max },
                        what,
                    });
                };
                let side = "the flare and the peg column's sides";
                keep([x, y, -tip], [x + core, y1, 0.0], side);
                keep([x1 - core, y, -tip], [x1, y1, 0.0], side);
                keep([x + core, y, -tip], [x1 - core, y + core, 0.0], side);
                keep([x + core, y1 - core, -tip], [x1 - core, y1, 0.0], side);
                keep(
                    [x + core, y + core, -tip],
                    [x1 - core, y1 - core, floor],
                    "the peg column's floor",
                );
            }
        }
        out
    }
}

/// The logo split as `make_logo_manifold` does: the square groove (any
/// contour whose bounding box is at least half the largest) keeps its
/// width; the lotus contours are shrunk by the clearance.
fn logo_parts(logo: &Outline) -> (Outline, Outline) {
    let area = |c: &Vec<[f64; 2]>| {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in c {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (hi[0] - lo[0]) * (hi[1] - lo[1])
    };
    let max = logo.contours.iter().map(area).fold(0.0, f64::max);
    let (groove, lotus): (Vec<_>, Vec<_>) = logo
        .contours
        .iter()
        .cloned()
        .partition(|c| area(c) / max >= 0.5);
    (Outline { contours: groove }, Outline { contours: lotus })
}

impl Element for DungeonBlocks {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Connector),
            reads: vec![Read::AllSolids],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let f = *scene.frame();
        let s = f.square_mm;
        let peg = self.peg_mm(scene.top());
        let tip = -(peg + self.flare_mm);
        let logo = Outline::svg_path(LOGO, LOGO_TOL).map_err(LookError::build)?;
        let (groove, lotus) = logo_parts(&logo);
        let logo_side = (self.column_mm - 2.0 * self.bevel_mm) * 0.80;
        let arc = base::arc(&f);
        let squares: Vec<(u32, u32)> = (0..f.cols)
            .flat_map(|c| (0..f.rows).map(move |r| (c, r)))
            .collect();
        // Three leaves per square (peg, groove, lotus), slots fixed by index
        // before the parallel work; all connector material, so the cut
        // faces are too.
        let n = u32::try_from(squares.len()).unwrap_or(u32::MAX);
        let slots = scene
            .slots(3 * n, Material::Connector)
            .map_err(LookError::build)?;
        let pegs: Vec<Result<Solid, LookError>> = squares
            .par_iter()
            .enumerate()
            .map(|(k, &(c, r))| {
                let k = u32::try_from(k).unwrap_or(u32::MAX);
                let (x, y) = (f64::from(c) * s, f64::from(r) * s);
                let round = base::tile_corners(&f, c, r);
                let rings = self.rings([x, y], s, peg, &arc, round);
                let body = loft(slots.get(3 * k), &rings).map_err(LookError::build)?;
                let (cx, cy) = (x + s / 2.0, y + s / 2.0);
                let (z0, z1) = (tip - LOGO_EPS_MM, tip + self.logo_depth_mm);
                let g = groove.placed(LOGO_VIEWBOX, cx, cy, logo_side);
                let l = lotus.placed(LOGO_VIEWBOX, cx, cy, logo_side);
                let cuts = [
                    extrude(slots.get(3 * k + 1), &g, 0.0, z0, z1).map_err(LookError::build)?,
                    extrude(slots.get(3 * k + 2), &l, self.logo_clearance_mm, z0, z1)
                        .map_err(LookError::build)?,
                ];
                carve(&body, &cuts).map_err(LookError::build)
            })
            .collect();
        let pegs: Vec<Solid> = pegs.into_iter().collect::<Result<_, _>>()?;
        let base = union(&pegs).map_err(LookError::build)?;
        scene.add(
            Tag {
                kind: Kind::Connector,
                owner: ctx.path.to_owned(),
            },
            base,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    #[test]
    fn peg_height_follows_content() {
        let db = DungeonBlocks::default();
        assert_eq!(db.peg_mm(5.0), 5.7);
        assert_eq!(db.peg_mm(15.0), 5.7);
        assert_eq!(db.peg_mm(15.1), 11.4);
    }

    #[test]
    fn a_pit_stays_inside_the_column() {
        let db = DungeonBlocks::default();
        let f = Frame::dungeonblocks(1, 1);
        let wall = 0.9;
        let pit = |half: f64, z: f64| base::Box3 {
            min: [17.5 - half, 17.5 - half, z],
            max: [17.5 + half, 17.5 + half, 3.0],
        };
        for (top, deep, too_deep) in [(5.0, 9.0, 10.0), (20.0, 15.0, 16.0)] {
            let keep = db.keep_out(&f, top, wall);
            // a pit well inside the column, about 9 mm (short) or 15 mm
            // (tall) deep, is allowed; deeper reaches the floor
            assert!(base::refuse_cut(&keep, &pit(8.0, -deep)).is_none(), "{top}");
            assert!(
                base::refuse_cut(&keep, &pit(8.0, -too_deep)).is_some(),
                "{top}"
            );
            // a pit as wide as the column reaches its sides, and one the
            // width of the flare its outside
            assert!(base::refuse_cut(&keep, &pit(12.5, -3.0)).is_some());
            assert!(base::refuse_cut(&keep, &pit(17.4, -1.0)).is_some());
        }
        // a cut above zero is never refused
        let keep = db.keep_out(&f, 5.0, wall);
        let above = base::Box3 {
            min: [0.0; 3],
            max: [35.0, 35.0, 4.0],
        };
        assert!(base::refuse_cut(&keep, &above).is_none());
    }

    #[test]
    fn the_top_ring_follows_the_tiles_round_corners() {
        let db = DungeonBlocks::default();
        let rings = db.rings(
            [0.0, 0.0],
            35.0,
            5.7,
            &base::arc(&Frame::dungeonblocks(1, 1)),
            [true, false, false, false],
        );
        assert!(rings.iter().all(|r| r.len() == rings[0].len()));
        assert!(!rings[0].iter().any(|p| p[0] == 0.0 && p[1] == 0.0));
        assert!(rings[0].iter().any(|p| p[0] == 35.0 && p[1] == 35.0));
    }

    #[test]
    fn logo_has_a_groove_and_a_lotus() {
        let (g, l) = logo_parts(&Outline::svg_path(LOGO, LOGO_TOL).unwrap());
        assert!(!g.contours.is_empty() && !l.contours.is_empty());
    }
}
