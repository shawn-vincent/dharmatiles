//! The floors, flagstones and planks: pieces bodies made to fit their band.
//!
//! A single height is a flat floor, a wide band a heaved one; a floor
//! meets the tile edge flush and square at `floorEdgeHeight`; a band that
//! jumps across a piece is refused (`dt::band`); a floor stands on the land
//! under it, or on the base with none.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Built, Library, Memory, Options, build};

fn mem(text: &str) -> Memory {
    common::modern(&[("t.dtile", text)])
}

fn tile(stack: &str) -> String {
    format!(
        "tile size=\"1x1\" seed=3 {{\n region mid {{ within 5 of=center }}\n stack {{\n{stack}\n}}\n}}\n"
    )
}

fn built(stack: &str) -> Built {
    let text = tile(stack);
    build(&mem(&text), "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// The highest vertex of the written mesh above the base, over the whole
/// tile (`inside`) or on its edges only.
fn tops(b: &Built, edges: bool) -> f64 {
    let [w, h] = b.footprint;
    let mut top = f64::MIN;
    for p in &b.export.mesh.positions {
        let on_edge = p[0] < 1e-6 || p[1] < 1e-6 || p[0] > w - 1e-4 || p[1] > h - 1e-4;
        if p[2] > 0.0 && (on_edge || !edges) {
            top = top.max(p[2]);
        }
    }
    top
}

#[test]
fn a_single_height_is_a_flat_floor_flush_at_the_edge() {
    // (the stack, how far under floorEdgeHeight its edge may top out: the
    // grain's depth, and a drystone flag's rounded shoulder)
    for (stack, under) in [
        ("base\n dirt\n flagstones", 0.4),
        ("base\n dirt\n flagstones style=fieldstone", 1.0),
        ("base\n dirt\n planks", 0.4),
    ] {
        let b = built(stack);
        assert!(b.audit.watertight, "{stack}");
        assert_eq!(b.audit.overhang_mm2, 0.0, "{stack}");
        // nothing over floorEdgeHeight anywhere, edge or not: the grain is
        // set in, never raised
        assert!(
            tops(&b, false) <= 7.4 + 1e-5,
            "{stack}: {}",
            tops(&b, false)
        );
        // and the edge reaches it, to within the grain's depth: the faces at the plane keep their relief
        assert!(tops(&b, true) >= 7.4 - under, "{stack}: {}", tops(&b, true));
    }
}

#[test]
fn a_wide_band_is_a_heaved_floor_held_flush_at_the_edge() {
    let b =
        built("base\n dirt\n flagstones slabs=4 top=\"[floorHeight - 1.5 .. floorHeight + 1.5]\"");
    assert!(b.audit.watertight);
    assert_eq!(b.audit.overhang_mm2, 0.0);
    // the slabs inside rise over the edge height, the band's top is never passed
    assert!(tops(&b, false) > 7.4 + 0.3, "{}", tops(&b, false));
    assert!(tops(&b, false) <= 7.4 + 1.5 + 1e-5, "{}", tops(&b, false));
    // the edge pin holds the slabs on the edge at floorEdgeHeight
    assert!(tops(&b, true) <= 7.4 + 1e-5, "{}", tops(&b, true));
}

#[test]
fn a_band_that_jumps_across_a_slab_is_refused_as_a_bug() {
    // one slab over the whole tile, a pin 9 mm high at its middle
    let text = tile("base\n dirt\n flagstones { pin top=9 at=mid }");
    let e = build(&mem(&text), "t.dtile", &Options::default())
        .err()
        .unwrap();
    assert_eq!(e[0].code, "dt::band", "{e:#?}");
}

#[test]
fn a_floor_with_no_land_stands_on_the_base() {
    let b = built("base\n planks");
    assert!(b.audit.watertight);
    assert!(b.audit.failures(&Options::default().printer).is_empty());
    assert!(tops(&b, false) <= 7.4 + 1e-5);
}

#[test]
fn the_library_floors_pass_the_gate() {
    for path in [
        "lib:tests/floors/1x1-floor-stone.dtile",
        "lib:tests/floors/1x1-planks.dtile",
        "lib:tests/floors/1x1-flagstones-heaved.dtile",
    ] {
        let b = build(&Library, path, &Options::default()).unwrap();
        assert!(
            b.audit.failures(&Options::default().printer).is_empty(),
            "{path}"
        );
    }
}

#[test]
fn the_faces_at_the_plane_keep_the_materials_grain_and_never_pass_it() {
    for stack in [
        "base\n dirt\n flagstones",
        "base\n dirt\n flagstones style=fieldstone",
        "base\n dirt\n planks",
    ] {
        let b = built(stack);
        let [w, h] = b.footprint;
        let (mut near, mut on) = (0_u32, 0_u32);
        for p in &b.export.mesh.positions {
            // nothing past the plane
            assert!(p[0] >= -1e-4 && p[1] >= -1e-4, "{stack}");
            assert!(p[0] <= w + 1e-4 && p[1] <= h + 1e-4, "{stack}");
            if p[2] < 4.5 {
                continue;
            }
            let d = p[0].min(w - p[0]).min(p[1]).min(h - p[1]);
            if d < 1.0 {
                near += 1;
                if d < 0.1 {
                    on += 1;
                }
            }
        }
        // at least 15% of a face is within 0.1 mm of the plane, so neighbours
        // touch; and the rest is set back, carrying the grain
        let share = f64::from(on) / f64::from(near.max(1_u32));
        assert!(share >= 0.15, "{stack}: {share}");
        assert!(share <= 0.97, "{stack}: a flat sawn face, {share}");
    }
}
