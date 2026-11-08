//! The overhang test in the loop: the gate's own layer rule asked of one
//! part, in its final pose, on what it rests on, while it is being made.
//! An element meets 55° by construction; this lets it check a part it
//! cannot reason about exactly (a leaf on a curved crown, pieces lying
//! over each other) and shrink or drop it there, where the fix belongs,
//! instead of finding out at the gate.  The gate itself is untouched.

use dt_geom::{BoolOp, Solid, TriMesh};

use super::{Printer, face_areas, overhang::Slices};
use crate::KitError;

/// How much a part hangs, by the gate's layer rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hang {
    /// Its area past the printer's `steepest`, mm² (red at the gate).
    pub over_mm2: f64,
    /// Its area past the printer's `overhang` but within `steepest`, mm²
    /// (the band, orange at the gate).
    pub band_mm2: f64,
}

impl Hang {
    /// Nothing past the printer's overhang: the target.
    #[must_use]
    pub fn none(&self) -> bool {
        self.over_mm2 <= 0.0 && self.band_mm2 <= 0.0
    }
}

/// **hang**: how much `part` hangs standing on `on` (the meshes it rests
/// on or against; empty for a part on the bed), by the same layer rule,
/// at the same angles, the gate uses.  Only the part's own faces count.
///
/// # Errors
/// [`KitError::Mesh`] if a mesh is not a closed manifold.
pub fn hang(part: &TriMesh, on: &[TriMesh], printer: &Printer) -> Result<Hang, KitError> {
    // the part and its support under two ids taken together, the part's
    // first, so the union is the same whatever ran before
    let id = Solid::reserve_ids(2);
    let solid = |m: &TriMesh, id: u32| {
        Solid::from_tri_mesh(m)
            .and_then(|s| s.with_original_id(id))
            .map_err(KitError::Mesh)
    };
    let mut parts = vec![solid(part, id)?];
    for m in on {
        parts.push(solid(m, id + 1)?);
    }
    let all = Solid::batch(&parts, BoolOp::Union);
    all.status().map_err(KitError::Mesh)?;
    let mesh = all.to_tri_mesh();
    let Some(slices) = Slices::cut(&mesh, printer) else {
        return Ok(Hang::default());
    };
    let steep = Printer {
        overhang_deg: printer.steepest_deg,
        ..*printer
    };
    let (past, red) = (slices.flag(&mesh, printer), slices.flag(&mesh, &steep));
    let mine = mesh
        .triangle_original_ids()
        .ok_or_else(|| KitError::Lift("a union without sources".into()))?;
    let mut h = Hang::default();
    for (k, a) in face_areas(&mesh).into_iter().enumerate() {
        if mine[k] != id {
            continue;
        }
        if red[k] {
            h.over_mm2 += a;
        } else if past[k] {
            h.band_mm2 += a;
        }
    }
    Ok(h)
}

/// **fit**: the largest scale in `[lo, 1]` at which `make(scale)` hangs
/// nothing past the printer's overhang on `on`, found by bisection in
/// `steps` halvings, with the part made at it; `None` when even `lo`
/// hangs (drop the part).  Scale 1 is tried first, so a part that passes
/// as made is kept as made.
///
/// # Errors
/// As [`hang`].
pub fn fit(
    make: impl Fn(f64) -> TriMesh,
    on: &[TriMesh],
    printer: &Printer,
    lo: f64,
    steps: u32,
) -> Result<Option<(f64, TriMesh)>, KitError> {
    let full = make(1.0);
    if hang(&full, on, printer)?.none() {
        return Ok(Some((1.0, full)));
    }
    let low = make(lo);
    if !hang(&low, on, printer)?.none() {
        return Ok(None);
    }
    let (mut good, mut bad, mut kept) = (lo, 1.0, low);
    for _ in 0..steps {
        let mid = 0.5 * (good + bad);
        let m = make(mid);
        if hang(&m, on, printer)?.none() {
            (good, kept) = (mid, m);
        } else {
            bad = mid;
        }
    }
    Ok(Some((good, kept)))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cube, fdm};
    use super::*;

    /// A post 2 mm square and 5 mm tall.
    fn post() -> TriMesh {
        cube([0.0, 0.0, 0.0], [2.0, 2.0, 5.0])
    }

    /// A shelf on the post's top reaching `out` mm past it.
    fn shelf(out: f64) -> TriMesh {
        cube([0.0, 0.0, 4.9], [2.0 + out, 2.0, 6.0])
    }

    #[test]
    fn a_shelf_hangs_and_a_part_on_the_bed_does_not() {
        let h = hang(&shelf(4.0), &[post()], &fdm()).unwrap();
        // the 4 × 2 mm underside past the post, flat: red
        assert!(h.over_mm2 > 6.0, "{h:?}");
        assert!(hang(&post(), &[], &fdm()).unwrap().none());
        // only the part's faces count: the post itself hangs nothing
        let h = hang(&post(), &[shelf(4.0)], &fdm()).unwrap();
        assert!(h.none(), "{h:?}");
    }

    #[test]
    fn fit_shrinks_a_part_until_it_hangs_nothing_or_drops_it() {
        let (s, m) = fit(|s| shelf(4.0 * s), &[post()], &fdm(), 0.02, 10)
            .unwrap()
            .unwrap();
        assert!(s < 1.0 && s > 0.02, "{s}");
        assert!(hang(&m, &[post()], &fdm()).unwrap().none());
        // a little more and it hangs
        assert!(
            !hang(&shelf(4.0 * (s + 0.05)), &[post()], &fdm())
                .unwrap()
                .none()
        );
        // what passes as made is kept as made
        let (s, _) = fit(|_| post(), &[], &fdm(), 0.5, 4).unwrap().unwrap();
        assert!((s - 1.0).abs() < f64::EPSILON);
        // a floating shelf hangs at any size
        let float = |s: f64| cube([3.0, 0.0, 4.0], [3.0 + 4.0 * s, 2.0, 5.0]);
        assert!(fit(float, &[post()], &fdm(), 0.25, 4).unwrap().is_none());
    }
}
