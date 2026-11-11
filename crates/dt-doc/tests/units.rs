//! Format 1's units and kinds, points and printers, as dt-doc reads them:
//! every rule has its refusal with the help that says what to write.

#![allow(clippy::unwrap_used)]

mod common;

use dt_doc::expr::{parse_units, parse_v2_units};
use dt_doc::{DocError, Known, decode};

fn known() -> Known {
    let mut k = Known::new().with_format_preview(true);
    k.add_heights("library/parts/heights.dtile", common::HEIGHTS)
        .unwrap();
    k
}

fn tile(body: &str) -> String {
    tile_in("1x1", body)
}

fn tile_in(size: &str, body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"{size}\" {{\n{body}\n}}\n")
}

fn errors(text: &str) -> Vec<DocError> {
    match decode("t.dtile", text, &known()) {
        Ok(_) => Vec::new(),
        Err(e) => e,
    }
}

fn ok(text: &str) {
    let e = errors(text);
    assert!(e.is_empty(), "{text}\n{e:#?}");
}

/// The text must be refused with `code`, its message or help saying `want`.
fn refused(text: &str, code: &str, want: &str) {
    let e = errors(text);
    assert!(
        e.iter().any(|e| e.code == code
            && (e.message.contains(want) || e.help.as_deref().is_some_and(|h| h.contains(want)))),
        "want {code} with `{want}`:\n{text}\n{e:#?}"
    );
}

#[test]
fn units_follow_numbers_and_bind_tighter_than_multiplication() {
    for good in [
        "8dmm",
        "4.5 mm",
        "2 pi rad",
        "(2 * pi) rad",
        "360deg",
        "0.5sq",
        "1in",
        "2layer",
        "3 * 2dmm",
        "-8dmm",
        "sin (90deg)",
        "$bottom + 30dmm * (1 - $d * $d)",
        "x($tile, sq)",
        "d($region, %)",
        "a(parent($region), deg)",
        "50%",
    ] {
        parse_v2_units(good).unwrap_or_else(|e| panic!("{good}: {e:?}"));
    }
    // `1/2rad` parses as one over two radians: the kind check refuses it
    let e = parse_v2_units("1/2rad").unwrap().to_string();
    assert_eq!(e, "(1 / (2 * rad))");
    // other names side by side stay an error, and the units are listed
    for (bad, said) in [
        ("1e3", "e3"),
        ("2foot", "foot"),
        ("8 foot", "foot"),
        ("4.5 mm foo", "foo"),
    ] {
        let e = parse_v2_units(bad).unwrap_err();
        assert!(e.message.contains(said), "{bad}: {e:?}");
        assert!(
            e.message.contains("not a unit") || e.message.contains("unexpected"),
            "{bad}: {e:?}"
        );
    }
    let e = parse_v2_units("1e3").unwrap_err();
    assert!(e.help.unwrap().contains("scientific notation"));
    // minus never starts a factor
    assert_eq!(
        parse_v2_units("3 -2dmm").unwrap().to_string(),
        "(3 - (2 * dmm))"
    );
    assert!(parse_v2_units("90°").is_err());
    // a weighting keeps its plain extra numbers
    let w = parse_units("low(1dmm .. 6dmm, 11)").unwrap();
    assert!(w.to_string().contains("low("), "{w}");
    // a unit alone is not a value
    assert!(parse_v2_units("mm").is_err());
    // no units in format 0's parser: the old refusal stands
    assert!(
        dt_doc::expr::parse("4.5mm")
            .unwrap_err()
            .message
            .contains("units are not written")
    );
}

#[test]
fn a_bare_number_where_a_length_or_an_angle_is_needed_is_an_error() {
    refused(
        &tile("stack { base\n scatter count=2 { rock size=\"8\" } }"),
        "dt::unit",
        "8dmm or 8mm",
    );
    refused(
        &tile("stack { base\n scatter count=2 { rock size=8 } }"),
        "dt::unit",
        "8dmm or 8mm",
    );
    refused(
        &tile("stack { base\n dirt maxSlope=35 }"),
        "dt::unit",
        "35deg",
    );
    refused(
        &tile("stack { base\n dirt top=8 }"),
        "dt::unit",
        "8dmm or 8mm",
    );
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 4\" }"),
        "dt::type",
        "4dmm",
    );
    // a bare zero is the same in every unit
    ok(&tile("stack { base\n dirt top=0 maxSlope=\"35deg\" }"));
    ok(&tile("stack { base\n dirt {\n pin top=0 at=center } }"));
    ok(&tile("stack { base\n dirt top=\"$groundHeight + 0\" }"));
}

#[test]
fn units_and_kinds_in_control_values() {
    ok(&tile(
        "stack { base\n scatter count=2 { rock size=\"8dmm\" } }",
    ));
    ok(&tile(
        "stack { base\n scatter count=2 { rock size=\"8 dmm\" } }",
    ));
    ok(&tile(
        "stack { base\n scatter count=2 { rock size=\"6dmm..10dmm\" } }",
    ));
    ok(&tile(
        "stack { base\n scatter count=2 { rock size=\"low(1dmm .. 6dmm, 11)\" } }",
    ));
    ok(&tile(
        "stack { base\n scatter count=2 { rock size=\"0.2sq\" } }",
    ));
    ok(&tile(
        "stack { base\n scatter perSquare=3 { pebble size=\"1mm..2in\" } }",
    ));
    ok(&tile("stack { base\n dirt maxSlope=\"35deg\" }"));
    ok(&tile("stack { base\n dirt maxSlope=\"0.6 rad\" }"));
    // an expert control, so in a part
    ok("dharmatiles version=1\ndefine oak { stack { tree { branch split=\"34deg\" } } }\n");
    refused(
        "dharmatiles version=1\ndefine oak { stack { tree { branch split=34 } } }\n",
        "dt::unit",
        "34deg",
    );
    // a range of bare numbers
    refused(
        &tile("stack { base\n scatter count=2 { rock size=\"6..10\" } }"),
        "dt::unit",
        "6dmm",
    );
    // a length where a count goes, and a count where a length goes
    refused(
        &tile("stack { base\n scatter count=\"3dmm\" { rock } }"),
        "dt::type",
        "plain number",
    );
    refused(
        &tile("stack { base\n scatter count=2 { rock size=\"2mm * 3mm\" } }"),
        "dt::type",
        "no area",
    );
    refused(
        &tile("stack { base\n scatter count=2 { rock size=\"1/2rad\" } }"),
        "dt::type",
        "(1/2) rad",
    );
    refused(
        &tile("stack { base\n scatter count=2 { rock size=\"1e3\" } }"),
        "dt::unit",
        "scientific notation",
    );
    // counts and shares stay plain
    ok(&tile("stack { base\n scatter count=3 { rock } }"));
    ok(&tile("stack { base\n grass cover=\"85%\" }"));
    ok(&tile("stack { base\n grass cover=0.85 }"));
    // the old annotation is gone
    refused(
        &tile("stack { base\n scatter count=2 { rock size=(mm)\"8\" } }"),
        "dt::unit",
        "not written in format 1",
    );
}

#[test]
fn heights_are_kind_checked_before_anything_is_built() {
    // length + length, length * plain
    ok(&tile(
        "stack { base\n dirt top=\"$bottom + 30dmm * (1 - $d * $d)\" }",
    ));
    ok(&tile(
        "stack { base\n dirt top=\"$groundHeight + 4dmm * $x\" }",
    ));
    // sin and cos take angles
    ok(&tile(
        "stack { base\n dirt top=\"$groundHeight + 4dmm * cos(720deg * ($x - $y))\" }",
    ));
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 4dmm * cos(720 * $x)\" }"),
        "dt::type",
        "takes an angle",
    );
    refused(
        &tile("stack { base\n dirt top=\"$bottom + 20%\" }"),
        "dt::type",
        "mixes",
    );
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + $x\" }"),
        "dt::type",
        "mixes a length and a plain",
    );
    refused(
        &tile("stack { base\n dirt top=\"2mm * 3mm\" }"),
        "dt::type",
        "no area",
    );
    refused(
        &tile("stack { base\n dirt top=\"$region\" }"),
        "dt::type",
        "region",
    );
    // a let keeps its kind
    ok(&tile(
        "let w \"3dmm\"\n let n 3\n stack { base\n dirt top=\"$groundHeight + w * n\" }",
    ));
    refused(
        &tile("let w \"3dmm\"\n stack { base\n dirt top=\"$groundHeight + w * w\" }"),
        "dt::type",
        "no area",
    );
    // coordinates take a region and a unit
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 1dmm * x($tile)\" }"),
        "dt::type",
        "takes 2 values",
    );
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 1dmm * x($tile, deg)\" }"),
        "dt::type",
        "counts in",
    );
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 1dmm * a($tile, sq)\" }"),
        "dt::type",
        "counts in",
    );
    refused(
        &tile("stack { base\n dirt top=\"$groundHeight + 1dmm * x(1, sq)\" }"),
        "dt::type",
        "region",
    );
    // a plain name is no word of the language
    refused(&tile("let sq 3\n stack { base }"), "dt::name", "unit");
}

#[test]
fn a_point_is_written_in_parentheses_with_units() {
    let line = |through: &str| {
        tile(&format!(
            "line run {{ through {through} }}\n stack {{ base }}"
        ))
    };
    ok(&line("\"(0.65sq, 1sq)\" south"));
    ok(&line("\"(50%, 2dmm)\" \"(10mm, 25%)\""));
    ok(&line("\"(3sq + 4mm, 0)\" south").replace("1x1", "4x1"));
    ok(&line("\"(0.2sq .. 0.4sq, 50%)\" south"));
    ok(&line(
        "\"point($tile, 50%, 0)\" \"point($region, 25%, 75%)\"",
    ));
    ok(&line(
        "\"polar($tile, 40%, 90deg)\" \"polar($tile, 0%, 0deg)\"",
    ));
    ok(&line("\"polar($tile, 100%, 2 pi rad)\" south"));
    // a point without them is an error whose help shows the tuple
    refused(&line("\"0.65,1\" south"), "dt::point", "(0.65sq, 1sq)");
    refused(&line("\"0.65\" south"), "dt::point", "parentheses");
    // each half is a length
    refused(
        &line("\"(0.65, 1sq)\" south"),
        "dt::unit",
        "0.65dmm or 0.65mm",
    );
    refused(&line("\"(1sq)\" south"), "dt::point", "two halves");
    refused(
        &line("\"(1sq, 1sq, 1sq)\" south"),
        "dt::point",
        "two halves",
    );
    // never a name an element sets
    refused(
        &line("\"($x, 0)\" south"),
        "dt::type",
        "worked out while an element builds",
    );
    refused(
        &line("\"(0, $bottom)\" south"),
        "dt::type",
        "once where it is written",
    );
    // the frame is always written, and a point lies on its tile
    refused(
        &line("\"point(50%, 0)\" south"),
        "dt::point",
        "takes 3 values",
    );
    refused(
        &line("\"point(parent($region), 50%, 0)\" south"),
        "dt::point",
        "no parent",
    );
    refused(
        &line("\"polar($tile, 40%, 90)\" south"),
        "dt::unit",
        "90deg",
    );
    refused(
        &line("\"polar($tile, 40deg, 90deg)\" south"),
        "dt::type",
        "plain number",
    );
    refused(&line("\"(2sq, 0)\" south"), "dt::point", "off the tile");
    refused(&line("\"(0, 2sq)\" south"), "dt::point", "off the tile");
    // a plain name is a `let` of the tile, in place
    ok(&tile(
        "let w \"0.25sq\"\n line run { through \"(w, 50%)\" \"(w + 0.5sq, 0)\" }\n stack { base }",
    ));
    refused(
        &tile("line run { through \"(w, 50%)\" south }\n stack { base }"),
        "dt::unknown-name",
        "no `let` is called `w`",
    );
}

#[test]
fn fills_places_and_pins_take_points_too() {
    ok(&tile(
        "line shore { through \"(0, 50%)\" \"(1sq, 0.5sq)\" }\n region pool { boundedBy shore; fill \"(50%, 25%)\" }\n point knoll \"(0.25sq, 0.75sq)\"\n stack { base\n dirt {\n pin top=\"$groundHeight\" at=\"(0.5sq, 0.5sq)\"\n pin top=\"$groundHeight\" at=knoll } }",
    ));
    refused(
        &tile("point knoll \"0.25,0.75\"\n stack { base }"),
        "dt::point",
        "(0.25sq, 0.75sq)",
    );
    // a tree stands at a point
    ok(&tile(
        "stack { base\n dirt\n tree at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 35dmm\" }",
    ));
    refused(
        &tile("stack { base\n dirt\n tree at=\"0.5,0.5\" top=\"$bottom + 35dmm\" }"),
        "dt::point",
        "parentheses",
    );
    // a point in the frame of a named region is not available yet: written
    // against the tile instead
    let in_region = |at: &str| {
        tile(&format!(
            "line shore {{ through \"(0, 0.5sq)\" \"(1sq, 0.5sq)\" }}\n region bank {{ boundedBy shore; fill \"(0.5sq, 0.2sq)\" }}\n stack {{ base\n dirt at=bank {{\n pin top=\"$groundHeight\" at={at} }} }}"
        ))
    };
    refused(
        &in_region("\"(0.5sq, 0.5sq)\""),
        "dt::point",
        "frame of the region `bank`",
    );
    ok(&in_region("\"point($tile, 0.5sq, 0.5sq)\""));
}

#[test]
fn a_printer_and_a_slope_take_units() {
    let printer = |body: &str| format!("dharmatiles version=1\nprinter p {{\n{body}\n}}\n");
    ok(&printer(
        "line \"0.5mm\"\n layer \"0.2mm\"\n overhang \"55deg\"\n steepest \"65deg\"\n gap \"0.6mm\"",
    ));
    ok(&printer(
        "line \"0.5mm\"\n layer \"0.008in\"\n overhang \"0.96 rad\"\n steepest \"65deg\"\n gap \"0.6mm\"",
    ));
    refused(
        &printer("line 0.5\n layer 0.2\n overhang 55\n steepest 65\n gap 0.6"),
        "dt::unit",
        "\"0.4mm\"",
    );
    refused(
        &printer(
            "line \"0.5mm\"\n layer \"0.2dmm\"\n overhang \"55deg\"\n steepest \"65deg\"\n gap \"0.6mm\"",
        ),
        "dt::unit",
        "not a unit of a printer",
    );
    refused(
        &printer(
            "line \"0.5mm\"\n layer \"0.2mm\"\n overhang \"55mm\"\n steepest \"65deg\"\n gap \"0.6mm\"",
        ),
        "dt::type",
        "angle",
    );
    ok(&tile("stack { base\n dirt {\n slope \"30deg\" } }"));
    refused(
        &tile("stack { base\n dirt {\n slope 30 } }"),
        "dt::unit",
        "30deg",
    );
}

#[test]
fn a_part_has_no_tile_to_read_a_point_in() {
    refused(
        "dharmatiles version=1\ndefine run { stack { wall at=\"(0.5sq, 0.5sq)\" } }\n",
        "dt::point",
        "a part has no tile",
    );
}
