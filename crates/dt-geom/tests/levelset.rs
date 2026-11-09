//! Manifold's level set sizes its hash table from the last vertex it
//! moved; when the table fills before any vertex has moved there is no such
//! vertex, and unpatched Manifold read one element before its buffer (an
//! intermittent fault in the wall and floor builds).  The vendored
//! `manifold-csg-sys` carries a guard for it
//! (`third_party/manifold-csg-sys/patches`; its build script stops the build
//! if the patch neither applies nor is already in the source).  This runs
//! many thin level sets, the shape that fills a small table early, and
//! wants each to be a sound solid of about the right volume.  It is a
//! stress, not a proof: the fault was intermittent.

#![allow(clippy::unwrap_used)]

use dt_geom::{Bounds, Solid};

#[test]
#[ignore = "a stress run of the vendored Manifold patch (about a minute)"]
fn thin_level_sets_never_fault_and_keep_their_volume() {
    // a slab 0.6 mm thick across a 30 mm box, meshed at 0.3 mm
    let sdf =
        |p: [f64; 3]| 0.3 - (p[2] - 4.0 - 0.8 * (0.4 * p[0]).sin() * (0.4 * p[1]).cos()).abs();
    let bounds = Bounds {
        min: [0.0, 0.0, 0.0],
        max: [30.0, 30.0, 9.0],
    };
    for run in 0..200 {
        let s = Solid::level_set(&sdf, bounds, 0.3);
        let v = s.volume();
        // 30 x 30 x 0.6, a little more for the slope, less at the box walls
        assert!((v - 540.0).abs() < 110.0, "run {run}: volume {v}");
    }
}
