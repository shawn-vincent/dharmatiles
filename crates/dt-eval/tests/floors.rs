//! The floors, flagstones and planks: pieces bodies made to fit their band.
//!
//! A single height is a flat floor, a wide band a heaved one; a floor
//! meets the tile edge flush and square at `floorEdgeHeight`; a band that
//! jumps across a piece is refused (`dt::band`); a floor stands on the land
//! under it, or on the base with none.

#![allow(clippy::unwrap_used)]

use dt_eval::{Built, Library, Memory, Options, build};

fn mem(text: &str) -> Memory {
    Memory(std::iter::once(("t.dtile".to_owned(), text.to_owned())).collect())
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
    for stack in [
        "base\n dirt\n flagstones",
        "base\n dirt\n flagstones style=fieldstone",
        "base\n dirt\n planks",
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
        // and the edge reaches it (square: its top is whole at the plane)
        assert!(tops(&b, true) >= 7.4 - 0.15, "{stack}: {}", tops(&b, true));
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
        "lib:tiles/floors/1x1-floor-stone.dtile",
        "lib:tiles/floors/1x1-floor-planks.dtile",
        "lib:tiles/test/1x1-flagstones-heaved.dtile",
    ] {
        let b = build(&Library, path, &Options::default()).unwrap();
        assert!(
            b.audit.failures(&Options::default().printer).is_empty(),
            "{path}"
        );
    }
}
