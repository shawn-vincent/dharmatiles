//! The audit gate: the mesh as a slicer sees it.
//!
//! Pinch separation, the float32 reload, keeping the largest body, and the
//! layer-reach overhang rule (`Slices`, the 55° rule that wins over
//! the other three `audit_overhang` copies). Order, as `stl_safe`: separate
//! pinches, quantise to float32 and merge by position, split into bodies
//! and keep the largest (reporting the rest), then check watertightness,
//! overhang and see-through holes on what is written.
//!
//! [`hang`] asks the same overhang rule of one part while it is made.

use std::collections::HashMap;

use dt_geom::TriMesh;
use rayon::prelude::*;

use crate::form::vertex_normals;
use crate::scene::Material;
use crate::shape::signed_volume;
use dt_geom::vec3::{cross, dist, sub};

mod hang;
mod overhang;
mod repair;

pub use hang::{Hang, fit, hang};

use overhang::Slices;
use repair::{canonical, contains, keep_largest, quantise, separate_pinches};

/// The tolerance band: faces past the printer's overhang (55°, the
/// conservative target everything aims for by construction) but within
/// `max_deg` are reported as band, not overhang, up to `share` of the
/// downward-facing area. Past `max_deg` they are overhang and fail. Shawn,
/// 3 Nov 2026 for trees, generalised 4 Nov 2026 to every element: 55–65° in
/// small patches, 65° the hard cap (a 0.2 mm layer steps 0.2·tan 65° =
/// 0.43 mm, under a 0.45 mm line). The share is the trees' measured one.
/// Past the cap, small patches are allowed too (Shawn, 4 Nov 2026: "allow
/// small patches over 65°, like the band"): up to `over_share` of the same
/// downward area; more than that fails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HangBand {
    /// The materials the band covers; empty: every material.
    pub materials: &'static [Material],
    /// The hard cap, degrees from vertical.
    pub max_deg: f64,
    /// Band area allowed, as a share of those materials' downward-facing
    /// area.
    pub share: f64,
    /// Area past `max_deg` allowed, as a share of the same downward area.
    pub over_share: f64,
}

impl HangBand {
    /// The band every element gets: 55–65° up to 0.1% of the downward area,
    /// and over 65° up to another 0.1%.
    pub const DEFAULT: Self = Self {
        materials: &[],
        max_deg: 65.0,
        share: 0.001,
        over_share: 0.001,
    };
}

/// The printer profile the gate checks against: the numbers of a
/// `printer` in library/printers/*.dtile (dt-eval decodes it with dt-doc
/// and calls [`Printer::new`]), plus the gate's own tuning, which no
/// profile sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Printer {
    /// No layer may reach past this cone from vertical (the profile's
    /// `overhang`).
    pub overhang_deg: f64,
    /// Past this, degrees from vertical, overhang is red; between
    /// `overhang_deg` and this it is the band (the profile's `steepest`).
    pub steepest_deg: f64,
    /// Layer height, mm (the profile's `layer`).
    pub layer_mm: f64,
    /// One extruded line, mm (the profile's `line`).
    pub line_mm: f64,
    /// The narrowest gap that stays open, mm (the profile's `gap`).
    pub gap_mm: f64,
    /// Gate tuning: one layer may reach this far out over the one below,
    /// mm (0.6).
    pub reach_mm: f64,
    /// Gate tuning: how many mm of layers below are compared.
    pub span_mm: f64,
    /// Gate tuning: vertical-ray spacing for the slice, mm.
    pub grid_mm: f64,
    /// Gate tuning: overhang the gate accepts, mm².
    pub overhang_budget_mm2: f64,
}

impl Printer {
    /// A printer profile's numbers, with the gate's fixed tuning;
    /// `steepest_deg` is 65 until [`Self::steepest`] sets it.
    #[must_use]
    pub fn new(line_mm: f64, layer_mm: f64, overhang_deg: f64, gap_mm: f64) -> Self {
        Self {
            overhang_deg,
            steepest_deg: 65.0,
            layer_mm,
            line_mm,
            gap_mm,
            reach_mm: 0.6,
            span_mm: 3.0,
            grid_mm: 0.1,
            overhang_budget_mm2: 0.0,
        }
    }

    /// The same printer with the profile's `steepest`.
    #[must_use]
    pub const fn steepest(self, steepest_deg: f64) -> Self {
        Self {
            steepest_deg,
            ..self
        }
    }
}

/// A left-out body smaller than this, mm³, is a boolean's sliver, not a
/// part (a printed bead, 0.45 × 0.45 × 0.2 mm, is 0.04; a slicer prints
/// nothing of it): an inward-wound one counts as a void wherever it lies,
/// an outward one is reported in `slivers`, not as a lost part.
const VOID_SLIVER_MM3: f64 = 1e-3;

/// A body keep-largest dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    /// Its triangles.
    pub triangles: usize,
    /// Its volume, mm³.
    pub volume_mm3: f64,
    /// Its bounding box, mm (low corner, high corner): where to element.
    pub bounds: [[f64; 3]; 2],
}

/// What the gate found.
#[derive(Clone, Debug, PartialEq)]
pub struct Audit {
    /// Triangles written.
    pub triangles: usize,
    /// Vertices written.
    pub vertices: usize,
    /// Volume written, mm³.
    pub volume_mm3: f64,
    /// Bodies before keep-largest.
    pub bodies: usize,
    /// Loose bodies keep-largest dropped, largest first.
    pub dropped: Vec<Dropped>,
    /// Sealed voids keep-largest left out (inward-wound shells, negative
    /// volume), so they print solid: a fill, not a lost part.
    pub voids: Vec<Dropped>,
    /// Loose boolean slivers keep-largest left out: outward bodies under
    /// a thousandth of a mm³, far below one printed bead. Not a lost part.
    pub slivers: Vec<Dropped>,
    /// Vertices pinch separation moved.
    pub pinches_moved: usize,
    /// Triangles that collapsed in the float32 merge (dropped).
    pub collapsed: usize,
    /// Closed and consistently oriented after the float32 reload.
    pub watertight: bool,
    /// Area of the faces past the overhang rule, mm².
    pub overhang_mm2: f64,
    /// Per written triangle: past the overhang rule (painted red).
    pub overhang: Vec<bool>,
    /// The overhang area allowed, mm²: the printer's budget, or the band's
    /// share past its cap, whichever is larger.
    pub overhang_budget_mm2: f64,
    /// Area of the faces in an element's tolerance band, mm² (past the
    /// printer's overhang, within the band's cap; not in `overhang_mm2`).
    pub band_mm2: f64,
    /// The band area allowed, mm² (0 without a band).
    pub band_budget_mm2: f64,
    /// Per written triangle: in the band (painted orange).
    pub band: Vec<bool>,
    /// Vertical rays inside the footprint that miss the mesh.
    pub holes: usize,
}

impl Audit {
    /// Why the gate fails, empty if it passes.
    #[must_use]
    pub fn failures(&self, printer: &Printer) -> Vec<String> {
        let mut f = Vec::new();
        if !self.watertight {
            f.push("not watertight after the float32 reload".to_owned());
        }
        if self.overhang_mm2 > self.overhang_budget_mm2.max(printer.overhang_budget_mm2) {
            f.push(format!(
                "{:.2} mm² of overhang (over {}°; budget {:.2} mm²)",
                self.overhang_mm2,
                HangBand::DEFAULT.max_deg,
                self.overhang_budget_mm2.max(printer.overhang_budget_mm2)
            ));
        }
        if self.band_mm2 > self.band_budget_mm2 {
            f.push(format!(
                "{:.2} mm² in the band ({}–{}°; budget {:.2} mm²)",
                self.band_mm2,
                printer.overhang_deg,
                HangBand::DEFAULT.max_deg,
                self.band_budget_mm2
            ));
        }
        if self.holes > 0 {
            f.push(format!("{} see-through holes", self.holes));
        }
        if self.triangles == 0 {
            f.push("nothing to print".to_owned());
        }
        // A dropped body is a part of the tile the print would lose (a
        // floating brick, a crumb): no library tile drops any, so there is
        // no volume floor.
        if let Some(big) = self.dropped.first() {
            let total: f64 = self.dropped.iter().map(|d| d.volume_mm3.abs()).sum();
            f.push(format!(
                "{} loose bodies dropped ({total:.2} mm³ in all, largest {} triangles)",
                self.dropped.len(),
                big.triangles
            ));
        }
        f
    }
}

/// The mesh the gate passes on: float32-exact positions, plain indices,
/// and for each triangle the index of the input triangle it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    /// The mesh as written.
    pub mesh: TriMesh,
    /// Input triangle of each written triangle.
    pub source: Vec<u32>,
}

/// Run the gate on a kernel export. `footprint` is the tile's width and
/// height in mm (for see-through holes).
#[must_use]
pub fn audit(input: &TriMesh, footprint: [f64; 2], printer: &Printer) -> (Export, Audit) {
    audit_banded(input, footprint, printer, &[], Some(&HangBand::DEFAULT))
}

/// [`audit`] with an element's tolerance [`HangBand`]; `materials` holds each
/// input triangle's material.
#[must_use]
pub fn audit_banded(
    input: &TriMesh,
    footprint: [f64; 2],
    printer: &Printer,
    materials: &[Material],
    band: Option<&HangBand>,
) -> (Export, Audit) {
    let mut m = canonical(input);
    let pinches_moved = separate_pinches(&mut m);
    let (m, source, collapsed) = quantise(&m);
    let (m, source, bodies, left_out) = keep_largest(&m, &source);
    // a void is an inward-wound shell sealed inside the kept body; an
    // inward-wound shell anywhere else is a lost part like any other.
    // Under VOID_SLIVER_MM3 an inward shell is a boolean's sliver lying
    // on the kept surface (its inside is ambiguous, and it holds no
    // material): filled, not lost.
    let inside: Vec<bool> = left_out
        .par_iter()
        .map(|(d, probe)| {
            d.volume_mm3 < 0.0 && (d.volume_mm3 > -VOID_SLIVER_MM3 || contains(&m, *probe))
        })
        .collect();
    let (voids, dropped): (Vec<_>, Vec<_>) = left_out
        .into_iter()
        .zip(inside)
        .map(|((d, _), i)| (d, i))
        .partition(|(_, i)| *i);
    let voids = voids.into_iter().map(|(d, _)| d).collect();
    let (slivers, dropped): (Vec<_>, Vec<_>) = dropped
        .into_iter()
        .map(|(d, _)| d)
        .partition(|d| d.volume_mm3.abs() < VOID_SLIVER_MM3);
    let watertight = m.is_watertight();
    let slices = Slices::cut(&m, printer);
    let (mut overhang, holes) = slices.as_ref().map_or_else(
        || (vec![false; m.triangles.len()], 0),
        |sl| (sl.flag(&m, printer), sl.holes(footprint)),
    );
    let areas = face_areas(&m);
    let mut in_band = vec![false; overhang.len()];
    let mut band_budget_mm2 = 0.0;
    let mut overhang_budget_mm2 = printer.overhang_budget_mm2;
    if let Some(b) = band {
        let tolerant: Vec<bool> = source
            .iter()
            .map(|&t| {
                b.materials.is_empty()
                    || materials
                        .get(t as usize)
                        .is_some_and(|mat| b.materials.contains(mat))
            })
            .collect();
        if overhang.iter().zip(&tolerant).any(|(o, t)| *o && *t) {
            let cap = Printer {
                overhang_deg: b.max_deg,
                ..*printer
            };
            let hard = slices
                .as_ref()
                .map_or_else(|| vec![false; m.triangles.len()], |sl| sl.flag(&m, &cap));
            for k in 0..overhang.len() {
                if overhang[k] && tolerant[k] && !hard[k] {
                    overhang[k] = false;
                    in_band[k] = true;
                }
            }
        }
        let down: f64 = m
            .triangles
            .iter()
            .zip(&tolerant)
            .zip(&areas)
            .filter(|((t, tol), _)| {
                **tol && {
                    let [a, b2, c] = t.map(|i| m.positions[i as usize]);
                    cross(sub(b2, a), sub(c, a))[2] < 0.0
                }
            })
            .map(|(_, a)| a)
            .sum();
        band_budget_mm2 = b.share * down;
        overhang_budget_mm2 = overhang_budget_mm2.max(b.over_share * down);
    }
    let band_mm2 = 0.0
        + in_band
            .iter()
            .zip(&areas)
            .filter(|(o, _)| **o)
            .map(|(_, a)| a)
            .sum::<f64>();
    // (an empty f64 sum is -0.0)
    let overhang_mm2 = 0.0
        + overhang
            .iter()
            .zip(&areas)
            .filter(|(o, _)| **o)
            .map(|(_, a)| a)
            .sum::<f64>();
    let a = Audit {
        triangles: m.triangles.len(),
        vertices: m.positions.len(),
        volume_mm3: signed_volume(&m),
        bodies,
        dropped,
        voids,
        slivers,
        pinches_moved,
        collapsed,
        watertight,
        overhang_mm2,
        overhang,
        overhang_budget_mm2,
        band_mm2,
        band_budget_mm2,
        band: in_band,
        holes,
    };
    (Export { mesh: m, source }, a)
}

/// A draft's stand-in for the gate (the editor's view while a value is
/// dragged): the parts as they are, not joined, nothing dropped or
/// measured, and each face flagged by its own angle against the printer's
/// overhang and steepest.  Faces on the bed (the tile's underside) and
/// those `skip` marks (the connector, which a draft does not change) are
/// not flagged.  The gate's verdict only ever comes from [`audit_banded`].
#[must_use]
pub fn draft(mesh: TriMesh, printer: &Printer, skip: &[bool]) -> (Export, Audit) {
    let lo = bounds(&mesh).map_or(0.0, |(lo, _)| lo[2]);
    let sin = |d: f64| d.to_radians().sin();
    let (s_over, s_steep) = (sin(printer.overhang_deg), sin(printer.steepest_deg));
    let n = mesh.triangles.len();
    let (mut overhang, mut band) = (vec![false; n], vec![false; n]);
    for (k, t) in mesh.triangles.iter().enumerate() {
        if skip.get(k).copied().unwrap_or(false) {
            continue;
        }
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        if a[2].max(b[2]).max(c[2]) < lo + 0.05 {
            continue;
        }
        let nrm = cross(sub(b, a), sub(c, a));
        let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if len <= 0.0 {
            continue;
        }
        // how far the face leans past vertical: 90 degrees is a ceiling
        let lean = -nrm[2] / len;
        if lean > s_steep {
            overhang[k] = true;
        } else if lean > s_over {
            band[k] = true;
        }
    }
    let audit = Audit {
        triangles: n,
        vertices: mesh.positions.len(),
        volume_mm3: 0.0,
        bodies: 0,
        dropped: Vec::new(),
        voids: Vec::new(),
        slivers: Vec::new(),
        pinches_moved: 0,
        collapsed: 0,
        watertight: false,
        overhang_mm2: 0.0,
        overhang,
        overhang_budget_mm2: 0.0,
        band_mm2: 0.0,
        band_budget_mm2: 0.0,
        band,
        holes: 0,
    };
    let source = (0..u32::try_from(n).unwrap_or(u32::MAX)).collect();
    (Export { mesh, source }, audit)
}

fn face_areas(m: &TriMesh) -> Vec<f64> {
    m.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            let f = cross(sub(b, a), sub(c, a));
            0.5 * (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt()
        })
        .collect()
}

pub(crate) fn bounds(m: &TriMesh) -> Option<([f64; 3], [f64; 3])> {
    let first = *m.positions.first()?;
    Some(m.positions.iter().fold((first, first), |(lo, hi), p| {
        (
            [0, 1, 2].map(|k| lo[k].min(p[k])),
            [0, 1, 2].map(|k| hi[k].max(p[k])),
        )
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use dt_geom::{BOX_TRIANGLES, Grid, lift};

    #[test]
    fn pinches_are_found_across_cell_borders() {
        // two cubes touching at a corner, the corners 0.1 µm apart, placed
        // so the pair straddles a hash cell border in every axis
        for off in [0.0, 1.5e-4, 3.9e-4] {
            let a = cube([0.0, 0.0, 0.0], [1.0 + off, 1.0 + off, 1.0 + off]);
            let b = cube([1.0 + off + 1e-4; 3], [2.0 + off, 2.0 + off, 2.0 + off]);
            let mut m = a.clone();
            let n = u32::try_from(a.positions.len()).unwrap();
            m.positions.extend(b.positions);
            m.triangles
                .extend(b.triangles.iter().map(|t| t.map(|v| v + n)));
            assert_eq!(separate_pinches(&mut m), 2, "offset {off}");
        }
    }

    pub(super) fn cube(lo: [f64; 3], hi: [f64; 3]) -> TriMesh {
        TriMesh {
            positions: (0..8)
                .map(|i| {
                    [
                        if i & 1 == 0 { lo[0] } else { hi[0] },
                        if i & 2 == 0 { lo[1] } else { hi[1] },
                        if i & 4 == 0 { lo[2] } else { hi[2] },
                    ]
                })
                .collect(),
            triangles: BOX_TRIANGLES.to_vec(),
            ..TriMesh::default()
        }
    }

    /// library/printers/default.dtile's numbers.
    pub(super) fn fdm() -> Printer {
        Printer::new(0.45, 0.2, 55.0, 0.2)
    }

    #[test]
    fn a_flat_slab_passes() {
        let m = lift(&Grid::<f32>::filled(21, 21, 0.5, 2.0)).unwrap();
        let (e, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.watertight);
        assert_eq!((a.bodies, a.holes), (1, 0));
        assert!(a.overhang_mm2 == 0.0, "{}", a.overhang_mm2);
        assert_eq!(e.mesh.triangles.len(), m.triangles.len());
        assert!(a.failures(&fdm()).is_empty());
    }

    #[test]
    fn a_sealed_void_is_filled_not_dropped() {
        // a 6 mm block with a 1 mm cavity inside: an inward-wound shell
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let cavity = cube([2.0, 2.0, 2.0], [3.0, 3.0, 3.0]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(cavity.positions);
        m.triangles.extend(
            cavity
                .triangles
                .iter()
                .map(|t| [t[0] + base, t[2] + base, t[1] + base]),
        );
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert_eq!(a.bodies, 2);
        assert!(a.dropped.is_empty(), "{:?}", a.dropped);
        assert_eq!(a.voids.len(), 1);
        assert!((a.voids[0].volume_mm3 + 1.0).abs() < 1e-9);
        assert!(a.failures(&fdm()).is_empty());
        // the same inward shell outside the block is a lost part, not a fill
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let stray = cube([7.0, 7.0, 2.0], [8.0, 8.0, 3.0]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(stray.positions);
        m.triangles.extend(
            stray
                .triangles
                .iter()
                .map(|t| [t[0] + base, t[2] + base, t[1] + base]),
        );
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.voids.is_empty(), "{:?}", a.voids);
        assert_eq!(a.dropped.len(), 1);
        assert!(!a.failures(&fdm()).is_empty());
    }

    #[test]
    fn a_loose_sliver_is_not_a_lost_part() {
        // a block and, apart from it, a 0.05 mm speck (1.25e-4 mm³)
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let speck = cube([8.0, 8.0, 1.0], [8.05, 8.05, 1.05]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(speck.positions);
        m.triangles
            .extend(speck.triangles.iter().map(|t| t.map(|i| i + base)));
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.dropped.is_empty(), "{:?}", a.dropped);
        assert_eq!(a.slivers.len(), 1);
        assert!(a.failures(&fdm()).is_empty());
    }

    #[test]
    fn a_floating_shelf_is_red_and_a_stray_body_is_dropped() {
        // a post with a 4 mm shelf sticking out at the top, plus a far crumb
        let mut m = cube([0.0, 0.0, 0.0], [2.0, 2.0, 6.0]);
        let shelf = cube([2.0, 0.0, 5.0], [6.0, 2.0, 6.0]);
        let crumb = cube([9.0, 9.0, 0.0], [9.5, 9.5, 0.5]);
        for extra in [shelf, crumb] {
            let base = u32::try_from(m.positions.len()).unwrap();
            m.positions.extend(extra.positions);
            m.triangles
                .extend(extra.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        // post and shelf touch only face to face, so they are two bodies here
        assert_eq!(a.bodies, 3);
        assert_eq!(a.dropped.len(), 2);
        let fails = a.failures(&fdm());
        assert!(
            fails.iter().any(|f| f.contains("2 loose bodies")),
            "{fails:?}"
        );
        let (_, b) = audit(&shelf_on_post(), [10.0, 10.0], &fdm());
        // the layer-reach rule gives 8.0 mm² on this shelf (4 × 2 mm underside)
        assert!(
            (b.overhang_mm2 - 8.0).abs() < 1e-9,
            "shelf underside: {}",
            b.overhang_mm2
        );
    }

    /// One closed L-shaped body: a post with a shelf at the top.
    fn shelf_on_post() -> TriMesh {
        let prof = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 5.0],
            [6.0, 5.0],
            [6.0, 6.0],
            [0.0, 6.0],
        ];
        prism(&prof, &[[0, 1, 2], [0, 2, 5], [2, 3, 4], [2, 4, 5]])
    }

    /// A post whose underside leans out at `deg` from vertical.
    fn leaning(deg: f64) -> TriMesh {
        let rise = 4.0 / deg.to_radians().tan();
        let prof = [[0.0, 0.0], [2.0, 0.0], [6.0, rise], [6.0, 5.0], [0.0, 5.0]];
        prism(&prof, &[[0, 1, 2], [0, 2, 3], [0, 3, 4]])
    }

    #[test]
    fn a_band_tolerates_its_materials_up_to_its_cap() {
        use crate::scene::Material;
        let band = HangBand {
            materials: &[Material::Leaf],
            max_deg: 65.0,
            share: 1.0,
            over_share: 0.0,
        };
        let at = |deg: f64, mat: Material, band: Option<&HangBand>| {
            let m = leaning(deg);
            let mats = vec![mat; m.triangles.len()];
            audit_banded(&m, [10.0, 10.0], &fdm(), &mats, band).1
        };
        // 60°: past 55, within 65: band for leaves, overhang for anything else
        let a = at(60.0, Material::Leaf, Some(&band));
        assert!(a.overhang_mm2 == 0.0 && a.band_mm2 > 1.0, "{a:?}");
        assert!(a.failures(&fdm()).is_empty(), "{:?}", a.failures(&fdm()));
        assert!(at(60.0, Material::Masonry, Some(&band)).overhang_mm2 > 1.0);
        assert!(at(60.0, Material::Leaf, None).overhang_mm2 > 1.0);
        // 70°: past the cap fails whatever the material
        let b = at(70.0, Material::Leaf, Some(&band));
        assert!(b.overhang_mm2 > 1.0 && b.band_mm2 == 0.0, "{b:?}");
        assert!(!b.failures(&fdm()).is_empty());
        // over the budget fails
        let tight = HangBand { share: 0.0, ..band };
        let c = at(60.0, Material::Leaf, Some(&tight));
        assert!(!c.failures(&fdm()).is_empty());
        // past the cap within `over_share` passes, red but allowed
        let loose = HangBand {
            over_share: 1.0,
            ..band
        };
        let d = at(70.0, Material::Leaf, Some(&loose));
        assert!(
            d.overhang_mm2 > 1.0 && d.overhang_budget_mm2 >= d.overhang_mm2,
            "{d:?}"
        );
        assert!(d.failures(&fdm()).is_empty(), "{:?}", d.failures(&fdm()));
    }

    /// A profile in x–z, extruded 2 mm in y; `fan` triangulates it.
    fn prism(prof: &[[f64; 2]], fan: &[[u32; 3]]) -> TriMesh {
        let n = u32::try_from(prof.len()).unwrap();
        let mut positions = Vec::new();
        for y in [0.0, 2.0] {
            positions.extend(prof.iter().map(|p| [p[0], y, p[1]]));
        }
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for f in fan {
            triangles.push([f[0], f[2], f[1]]);
            triangles.push([f[0] + n, f[1] + n, f[2] + n]);
        }
        for k in 0..n {
            let k1 = (k + 1) % n;
            triangles.push([k, k1, k1 + n]);
            triangles.push([k, k1 + n, k + n]);
        }
        let mut m = TriMesh {
            positions,
            triangles,
            ..TriMesh::default()
        };
        if signed_volume(&m) < 0.0 {
            m.triangles.iter_mut().for_each(|t| t.swap(1, 2));
        }
        assert!(m.is_watertight());
        m
    }

    /// A block on a slab, held by a post at one end, with a bed joint of
    /// `gap` mm between them: a C in profile.
    fn block_on_joint(gap: f64) -> TriMesh {
        let g = 2.0 + gap;
        let prof = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [1.0, 2.0],
            [1.0, g],
            [6.0, g],
            [6.0, 4.0],
            [0.0, 4.0],
        ];
        prism(
            &prof,
            &[
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [0, 4, 7],
                [4, 5, 6],
                [4, 6, 7],
            ],
        )
    }

    #[test]
    fn a_bed_joint_thinner_than_the_gap_is_no_overhang() {
        // 0.11 mm: the wall's bed joint; the layer above lands on the stone below
        let (_, a) = audit(&block_on_joint(0.11), [10.0, 10.0], &fdm());
        assert!(a.overhang_mm2 == 0.0, "{}", a.overhang_mm2);
        // the plain rule (every void open) flags its roof
        let open = Printer {
            gap_mm: 0.0,
            ..fdm()
        };
        let (_, was) = audit(&block_on_joint(0.11), [10.0, 10.0], &open);
        assert!(was.overhang_mm2 > 1.0, "{}", was.overhang_mm2);
        // 1 mm: a real slot, whose roof is a 5 mm shelf
        let (_, b) = audit(&block_on_joint(1.0), [10.0, 10.0], &fdm());
        assert!(b.overhang_mm2 > 5.0, "{}", b.overhang_mm2);
    }
}
