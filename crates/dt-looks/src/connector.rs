//! `connector`: what is under z = 0. P1: the DungeonBlocks socket peg.
//!
//! Port of `src/dharmatiles/bases/dungeonblocks.py` and `core/logo.py`.
//! DungeonBlocks credit: the socket-peg geometry follows the DungeonBlocks
//! blank/floor tile standard on MyMiniFactory
//! (<https://www.myminifactory.com/object/3d-print-dungeonblocks-blank-floor-tile-177592>).
//!
//! One peg per square: the full square at z = 0 flares in to a 26 mm
//! column over 5.2 mm, runs down the peg height, and bevels 1.5 mm in to
//! the tip, into which the lotus logo is inset 0.8 mm. The peg height
//! depends on the content's maximum z (`select_peg_height`, :20–37), so
//! the connector reads every solid and is built last.

use dt_kit::{Material, Outline, Scene, Solid, Tag, carve, extrude, loft, union};
use rayon::prelude::*;

use crate::{Ctx, Look, LookError, Plan, Read, Write};

/// `BaseConfig`'s DungeonBlocks constants, mm.
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

    /// The four rings of one peg, top to bottom (`make_base`).
    fn rings(&self, x: f64, y: f64, s: f64, peg: f64) -> Vec<Vec<[f64; 3]>> {
        let col_inset = (s - self.column_mm) / 2.0;
        let bevel_inset = (s - (self.column_mm - 2.0 * self.bevel_mm)) / 2.0;
        let ring = |i: f64, z: f64| {
            vec![
                [x + i, y + i, z],
                [x + s - i, y + i, z],
                [x + s - i, y + s - i, z],
                [x + i, y + s - i, z],
            ]
        };
        vec![
            ring(0.0, 0.0),
            ring(col_inset, -self.flare_mm),
            ring(col_inset, -(peg - self.bevel_mm + self.flare_mm)),
            ring(bevel_inset, -(peg + self.flare_mm)),
        ]
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

impl Look for DungeonBlocks {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("connector"),
            reads: vec![Read::AllSolids],
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
                let body =
                    loft(slots.get(3 * k), &self.rings(x, y, s, peg)).map_err(LookError::build)?;
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
                kind: "connector",
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
    fn logo_has_a_groove_and_a_lotus() {
        let (g, l) = logo_parts(&Outline::svg_path(LOGO, LOGO_TOL).unwrap());
        assert!(!g.contours.is_empty() && !l.contours.is_empty());
    }
}
