//! Documents end to end through an in-memory source: wrong documents give
//! an error at their line with a hint; imports, regions and the scheduler
//! work on real text.

#![allow(clippy::unwrap_used)]

use dt_eval::{Diag, Memory, Options, build, check};

fn mem(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
            .collect(),
    )
}

fn errors(text: &str) -> Vec<Diag> {
    match check(&mem(&[("t.kdl", text)]), "t.kdl", &Options::default()) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    }
}

fn rendered(d: &Diag) -> String {
    let mut s = String::new();
    miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
        .render_report(&mut s, d)
        .unwrap();
    s
}

#[test]
fn a_misspelt_look_points_at_its_line_with_a_hint() {
    let e = errors("tile flat size=\"1x1\" {\n    stack {\n        grond\n    }\n}\n");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].location(), Some(("t.kdl".into(), 3, 9)));
    assert_eq!(e[0].help.as_deref(), Some("did you mean `ground`?"));
    let r = rendered(&e[0]);
    assert!(
        r.contains("unknown look `grond`") && r.contains("3 │") && r.contains("did you mean"),
        "{r}"
    );
}

#[test]
fn an_unknown_level_lists_the_levels() {
    let e = errors("tile flat {\n  stack {\n    ground level=grund\n  }\n}\n");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert_eq!(e[0].help.as_deref(), Some("did you mean `ground`?"));
    let e = errors("tile flat {\n  stack {\n    ground level=lava\n  }\n}\n");
    assert!(e[0].help.as_deref().unwrap().contains("wall-top (33.1 mm)"));
}

#[test]
fn an_unknown_control_names_the_ones_there_are() {
    let e = errors("tile flat {\n  stack {\n    ground bumps=3\n  }\n}\n");
    assert_eq!(e[0].message, "`ground` has no control `bumps`");
    assert_eq!(e[0].help.as_deref(), Some("ground takes: level"));
}

#[test]
fn kdl_syntax_errors_carry_their_place() {
    let e = errors("tile flat {\n  stack {\n    ground level=5mm\n  }\n}\n");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(e[0].help.is_some());
}

#[test]
fn a_bad_import_points_at_the_use_line() {
    let e = errors("use \"lib:parts/nothing\" lawn\ntile flat { stack { ground } }\n");
    assert_eq!(e[0].location().unwrap().1, 1);
    assert!(e[0].message.contains("nothing.kdl"));
}

#[test]
fn imports_inline_definitions_and_levels_resolve() {
    let src = mem(&[
        (
            "parts/flats.kdl",
            "define raised { stack { ground level=\"ground + 2\" } }\n",
        ),
        (
            "tiles/t.kdl",
            "use \"../parts/flats\" raised\ntile t { levels foot=4; stack { raised } }\n",
        ),
    ]);
    let c = check(&src, "tiles/t.kdl", &Options::default()).unwrap();
    assert_eq!(c.tile.stack.len(), 1);
    assert_eq!(c.tile.stack[0].src.name(), "parts/flats.kdl");
    let b = build(&src, "tiles/t.kdl", &Options::default()).unwrap();
    let top = b
        .export
        .mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::MIN, f64::max);
    assert!((top - 7.0).abs() < 1e-5, "top {top}");
}

#[test]
fn a_line_and_two_regions_make_a_step() {
    let src = mem(&[(
        "t.kdl",
        r#"tile step size="1x1" seed=3 {
    line across { through west east }
    region low { bounded-by across; fill "0.5,0.25" }
    region high { bounded-by across; fill "0.5,0.75" }
    stack {
        ground in=high level=8
        ground in=low
    }
}
"#,
    )]);
    let b = build(&src, "t.kdl", &Options::default()).unwrap();
    assert!(
        b.audit.failures(&Options::default().printer).is_empty(),
        "{:?}",
        b.audit
    );
    let z_at = |y: f64| {
        b.export
            .mesh
            .positions
            .iter()
            .filter(|p| (p[1] - y).abs() < 0.2 && p[2] > 0.0)
            .map(|p| p[2])
            .fold(f64::MIN, f64::max)
    };
    assert!((z_at(5.0) - 5.0).abs() < 1e-5 && (z_at(30.0) - 8.0).abs() < 1e-5);
}

#[test]
fn an_unknown_region_is_refused() {
    let e =
        errors("tile t {\n  region a { fill \"0.5,0.5\" }\n  stack {\n    ground in=b\n  }\n}\n");
    assert_eq!(e[0].code, "dt::unknown-region");
    assert_eq!(e[0].location().unwrap().1, 4);
}
