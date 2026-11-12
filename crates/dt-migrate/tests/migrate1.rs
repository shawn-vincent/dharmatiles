//! Migrating format 0 to format 1, text to text: what each old form becomes,
//! what stops a file, and that the result is a format 1 document the reader
//! accepts.

#![allow(clippy::unwrap_used)]

mod common;

use dt_doc::header::stamp;
use dt_doc::{DocError, Known, decode};
use dt_migrate::format1::{Outcome, migrate};

const VERSION: &str = "0.9.0+5.gabc1234";

fn known() -> Known {
    common::known()
}

fn lib(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/library0/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn codes(e: &[DocError]) -> Vec<&'static str> {
    e.iter().map(|e| e.code).collect()
}

fn moved(name: &str, text: &str) -> String {
    match migrate(name, text, &known(), VERSION) {
        Ok(Outcome::Migrated(m)) => m.text,
        other => panic!("{name}: {other:?}"),
    }
}

fn refused(text: &str) -> Vec<DocError> {
    migrate("t.dtile", text, &known(), VERSION).unwrap_err()
}

#[test]
fn the_stamp_has_the_formats_major() {
    // a program before 1.0.0 writes format 1 with its own build details
    assert_eq!(stamp("0.9.0+5.gabc1234"), "1.0.0+5.gabc1234");
    assert_eq!(stamp("0.9.0"), "1.0.0");
    assert_eq!(stamp("1.2.0+3.gf00"), "1.2.0+3.gf00");
    assert_eq!(stamp("1.0.0"), "1.0.0");
}

#[test]
fn a_tile_takes_the_header_names_units_and_points() {
    let out = moved(
        "t.dtile",
        r#"// A pond.
tile size="1x2" seed=3 {
    line shore wobble=2.5 { through "0,0.4" south "0.4,1.5" }
    region pool { boundedBy shore; fill "0.1,0.1" }
    region band { within 7 of=shore }
    stack {
        base
        dirt top="[groundHeight .. groundHeight + 1]" {
            pin top="bottom + 2" at=pool
        }
        scatter at=pool count=3 { rock size="4..8" }
        water
    }
}
"#,
    );
    assert!(
        out.starts_with("dharmatiles version=\"1.0.0+5.gabc1234\"\n\n// A pond.\n"),
        "{out}"
    );
    // rows is 2: y becomes 2 - y
    assert!(
        out.contains(r#"through "(0sq, 1.6sq)" south "(0.4sq, 0.5sq)""#),
        "{out}"
    );
    assert!(out.contains(r#"fill "(0.1sq, 1.9sq)""#), "{out}");
    assert!(out.contains(r#"within "7dmm" of=shore"#), "{out}");
    assert!(out.contains(r#"wobble="2.5dmm""#), "{out}");
    assert!(
        out.contains(r#"top="[$groundHeight .. $groundHeight + 1dmm]""#),
        "{out}"
    );
    assert!(out.contains(r#"top="$bottom + 2dmm""#), "{out}");
    assert!(out.contains(r#"size="4dmm .. 8dmm""#), "{out}");
    assert!(out.contains("count=3"), "{out}");
    // what is not a value stays as it was, layout and comment included
    assert!(out.contains("        scatter at=pool count=3 {"), "{out}");
}

#[test]
fn a_decimal_flips_to_the_decimal_the_reader_flips_back() {
    // `1 - 0.1` is not 0.9 in floats; the reader works the flip to twelve
    // decimals, so every decimal comes back as itself
    let out = moved(
        "t.dtile",
        "tile size=\"1x1\" { line l { through \"0.1,0.1\" \"0.3,0.7\" }\n stack { base } }\n",
    );
    assert!(
        out.contains(r#"through "(0.1sq, 0.9sq)" "(0.3sq, 0.3sq)""#),
        "{out}"
    );
    let doc = decode("t", &out, &known()).unwrap();
    let line = &doc.tile().unwrap().lines[0];
    let dt_doc::place::Point::Pair { x, y } = &line.through[0].value else {
        panic!()
    };
    assert_eq!(x.to_string(), "0.1");
    assert_eq!(y.to_string(), "0.1");
}

#[test]
fn a_range_in_a_point_runs_the_other_way() {
    let out = moved(
        "t.dtile",
        "tile size=\"2x2\" { line l { through \"0,0.5\" \"2,0.5\" }\n region r { boundedBy l; fill \"0.6..1.4,0.2..0.4\" }\n stack { base } }\n",
    );
    assert!(
        out.contains(r#"fill "(0.6sq .. 1.4sq, 1.6sq .. 1.8sq)""#),
        "{out}"
    );
}

#[test]
fn the_tide_reads_y_from_the_north() {
    let out = moved("heights.dtile", &lib("parts/heights.dtile"));
    assert!(
        out.contains(
            r#"let $waterEdgeHeight    "$waterSurfaceHeight + 0.6dmm * cos(720deg * ($x - $y))""#
        ),
        "{out}"
    );
    assert!(out.contains(r#"let $groundHeight       "5dmm""#), "{out}");
    assert!(
        out.contains(r#"let $floorEdgeHeight    "$floorHeight""#),
        "{out}"
    );
}

#[test]
fn a_tile_that_reads_y_flips_it_against_its_rows() {
    let out = moved(
        "t.dtile",
        "tile size=\"2x3\" { stack { base\n dirt top=\"groundHeight + 0.5 * cos(90 * y)\" } }\n",
    );
    assert!(
        out.contains(r#"top="$groundHeight + 0.5dmm * cos(90deg * (3 - $y))""#),
        "{out}"
    );
}

#[test]
fn a_part_that_reads_y_has_no_rows_to_flip_against() {
    let e = refused("define d { stack { dirt top=\"groundHeight + 0.5 * cos(90 * y)\" } }\n");
    assert_eq!(codes(&e), ["dt::migrate-by-hand"]);
}

#[test]
fn a_pin_in_a_body_at_a_region_keeps_its_frame() {
    let out = moved(
        "t.dtile",
        r#"tile size="1x1" {
    line l { through "0,0.5" "1,0.5" }
    region r { boundedBy l; fill "0.5,0.2" }
    stack {
        base
        dirt at=r { pin top=5 at="(0.5, 0.25)" }
        dirt { pin top=5 at="(0.5, 0.25)" }
    }
}
"#,
    );
    // in a region's frame a bare tuple would be that region's box
    assert!(
        out.contains(r#"pin top="5dmm" at="point($tile, 0.5sq, 0.75sq)""#),
        "{out}"
    );
    assert!(
        out.contains(r#"pin top="5dmm" at="(0.5sq, 0.75sq)""#),
        "{out}"
    );
}

#[test]
fn angles_lengths_and_weightings() {
    let out = moved(
        "t.dtile",
        r#"tile size="1x1" {
    stack {
        base
        dirt maxSlope=40 slope=30
        scatter perSquare=55 { stone size="low(1..6, 11)" }
        tree top="bottom + 35" {
            trunk height=18
            crown width=44 clumps=60
        }
    }
}
"#,
    );
    assert!(out.contains(r#"slope="30deg""#), "{out}");
    assert!(out.contains(r#"maxSlope="40deg""#), "{out}");
    assert!(out.contains("perSquare=55"), "{out}");
    assert!(out.contains(r#"size="low(1dmm .. 6dmm, 11)""#), "{out}");
    assert!(out.contains(r#"trunk height="18dmm""#), "{out}");
    assert!(out.contains(r#"crown width="44dmm" clumps=60"#), "{out}");
}

#[test]
fn an_mm_annotation_is_a_dmm_value() {
    let out = moved(
        "t.dtile",
        "tile size=\"1x1\" { stack { base\n scatter perSquare=2 { rock size=(mm)4.5 } } }\n",
    );
    assert!(out.contains(r#"size="4.5dmm""#), "{out}");
}

#[test]
fn a_lets_unit_comes_from_where_it_is_used() {
    let out = moved(
        "t.dtile",
        r#"tile size="1x1" {
    let foot 4   // the foot
    let n 3
    stack {
        base
        dirt top=foot
        scatter count=n { rock size="2..3" }
    }
}
"#,
    );
    assert!(out.contains(r#"let foot "4dmm"   // the foot"#), "{out}");
    assert!(out.contains("let n 3\n"), "{out}");
    assert!(out.contains("dirt top=foot"), "{out}");
}

#[test]
fn a_let_used_both_ways_or_never_stops_the_file() {
    let both = refused(
        "tile size=\"1x1\" { let k 3\n stack { base\n dirt top=k\n scatter count=k { rock } } }\n",
    );
    assert_eq!(codes(&both), ["dt::migrate-unit"]);
    assert!(both[0].message.contains("`k`"), "{}", both[0].message);
    let never = refused("tile size=\"1x1\" { let k 3\n stack { base } }\n");
    assert_eq!(codes(&never), ["dt::migrate-unit"]);
    assert!(
        never[0].message.contains("never used"),
        "{}",
        never[0].message
    );
}

#[test]
fn a_param_takes_its_unit_through_nested_definitions() {
    let out = moved("p.dtile", &lib("parts/grass.dtile"));
    assert!(out.contains(r#"param length "3.5dmm .. 6.5dmm""#), "{out}");
    assert!(out.contains(r#"param height "1.5dmm .. 2dmm""#), "{out}");
    assert!(out.contains("param clearings 0"), "{out}");
    assert!(out.contains(r#"param cover "100%""#), "{out}");
    assert!(
        out.contains(r#"lawn length="5dmm .. 9dmm" height="2dmm .. 3dmm""#),
        "{out}"
    );
}

#[test]
fn a_call_passes_its_values_in_the_callees_units() {
    // the part is format 0 here; its parameters say what each value is
    let out = moved("t.dtile", &lib("tiles/ground/1x1-grass-flowers.dtile"));
    assert!(out.contains("dharmatiles version="), "{out}");
    assert!(!out.contains("lawn length=\"3.5\""), "{out}");
}

#[test]
fn an_override_of_a_library_height_gains_its_dollar() {
    let out = moved(
        "t.dtile",
        "tile size=\"1x1\" { override groundHeight 6\n stack { base\n dirt } }\n",
    );
    assert!(out.contains(r#"override $groundHeight "6dmm""#), "{out}");
}

#[test]
fn an_override_of_a_plain_let_has_no_format_1_form() {
    let e = refused("tile size=\"1x1\" { let k 3\n override k 4\n stack { base\n dirt top=k } }\n");
    assert!(codes(&e).contains(&"dt::migrate-by-hand"), "{e:?}");
}

#[test]
fn a_let_named_for_a_word_of_the_language_is_renamed() {
    let out = moved(
        "t.dtile",
        "tile size=\"1x1\" { let high 4\n stack { base\n dirt top=high } }\n",
    );
    assert!(out.contains(r#"let myHigh "4dmm""#), "{out}");
    assert!(out.contains(r#"top="myHigh""#), "{out}");
}

#[test]
fn a_printer_takes_printed_units() {
    let out = moved("printer.dtile", &lib("printers/default.dtile"));
    assert!(out.contains(r#"layer    "0.2mm""#), "{out}");
    assert!(out.contains(r#"overhang "55deg""#), "{out}");
    assert!(out.contains(r#"steepest "65deg""#), "{out}");
    assert!(out.contains(r#"gap      "0.2mm""#), "{out}");
    assert!(out.contains(r#"line     "0.45mm""#), "{out}");
    // the same numbers
    let old = dt_migrate::old::decode("old", &lib("printers/default.dtile"), &known()).unwrap();
    let new = decode("new", &out, &known()).unwrap();
    let (a, b) = (old.printer.unwrap(), new.printer.unwrap());
    assert_eq!(
        (
            a.line.value,
            a.layer.value,
            a.overhang.value,
            a.steepest.value,
            a.gap.value
        ),
        (
            b.line.value,
            b.layer.value,
            b.overhang.value,
            b.steepest.value,
            b.gap.value
        )
    );
}

#[test]
fn a_palette_and_a_part_take_the_header_only() {
    let out = moved("palette.dtile", &lib("palettes/default.dtile"));
    assert!(
        out.starts_with("dharmatiles version=\"1.0.0+5.gabc1234\"\n\n// The default"),
        "{out}"
    );
    assert_eq!(
        out.matches('\n').count(),
        lib("palettes/default.dtile").matches('\n').count() + 2
    );
}

#[test]
fn a_document_already_at_format_1_is_left_alone() {
    for header in [
        "dharmatiles version=1\n",
        "dharmatiles version=\"1.0.0+5.gf9481c8\"\n",
        "dharmatiles version=\"1.7.2\"\n",
    ] {
        let text = format!("{header}tile size=\"1x1\" {{ stack {{ base }} }}\n");
        assert_eq!(
            migrate("t", &text, &known(), VERSION).unwrap(),
            Outcome::Current(1),
            "{header}"
        );
    }
}

#[test]
fn a_stamped_format_0_document_has_its_stamp_replaced() {
    let out = moved(
        "t.dtile",
        "dharmatiles version=\"0.9.0+1.gaaa\"\ntile size=\"1x1\" { stack { base } }\n",
    );
    assert!(
        out.starts_with("dharmatiles version=\"1.0.0+5.gabc1234\"\n"),
        "{out}"
    );
    assert_eq!(out.matches("dharmatiles").count(), 1);
}

#[test]
fn an_unstamped_tile_with_new_forms_is_refused_where_they_are() {
    // only new forms: a fresh tile that forgot its header
    let e = refused(
        "tile size=\"1x1\" { line l { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }\n stack { base } }\n",
    );
    assert_eq!(codes(&e), ["dt::migrate-header"]);
    assert!(
        e[0].help
            .as_deref()
            .unwrap()
            .contains("dharmatiles version=1")
    );
    // both: a point in the old form and a unit in the new
    let e = refused(
        "tile size=\"1x1\" { line l { through \"0,0.5\" \"1,0.5\" }\n stack { base\n dirt top=\"8dmm\" } }\n",
    );
    assert_eq!(codes(&e), ["dt::migrate-mixed"]);
    let text = "tile size=\"1x1\" { line l { through \"0,0.5\" \"1,0.5\" }\n stack { base\n dirt top=\"8dmm\" } }\n";
    let at = e[0].span.offset();
    assert!(
        text[at..].starts_with('8')
            || text[at..].starts_with("dmm")
            || text[at - 1..].starts_with('8'),
        "{}",
        &text[at..]
    );
    // a `$` name
    let e = refused("tile size=\"1x1\" { stack { base\n dirt top=\"$groundHeight\" } }\n");
    assert_eq!(codes(&e), ["dt::migrate-header"]);
    // a tuple with no units is a format 0 point
    assert!(
        migrate(
            "t",
            "tile size=\"1x1\" { point p \"(0.5, 0.5)\"\n stack { base } }\n",
            &known(),
            VERSION
        )
        .is_ok()
    );
}

#[test]
fn a_file_with_its_own_errors_is_not_migrated() {
    let e = refused("tile size=\"1x1\" { stack { base\n dirt top=nonsense } }\n");
    assert_eq!(codes(&e), ["dt::unknown-name"]);
}

#[test]
fn the_result_reads_as_format_1_and_migrating_it_again_changes_nothing() {
    let k = known();
    for p in [
        "tiles/water/1x1-water+soil-angle.dtile",
        "tiles/ground/1x1-soil-u+grass.dtile",
        "tests/cliffs/2x1-cliff-full.dtile",
        "tiles/ground/2x2-grass-tree.dtile",
    ] {
        let once = moved(p, &lib(p));
        decode(p, &once, &k).unwrap_or_else(|e| panic!("{p}: {e:?}\n{once}"));
        assert_eq!(
            migrate(p, &once, &known(), VERSION).unwrap(),
            Outcome::Current(1)
        );
    }
}
