//! Documents end to end through an in-memory source: wrong documents give
//! an error at their line with a hint; imports, regions and the scheduler
//! work on real text.

#![allow(clippy::unwrap_used)]

use dt_eval::{Diag, Library, Memory, Options, build, check};

fn mem(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
            .collect(),
    )
}

fn errors(text: &str) -> Vec<Diag> {
    match check(&mem(&[("t.dtile", text)]), "t.dtile", &Options::default()) {
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
    assert_eq!(e[0].location(), Some(("t.dtile".into(), 3, 9)));
    assert_eq!(e[0].help.as_deref(), Some("did you mean `ground`?"));
    let r = rendered(&e[0]);
    assert!(
        r.contains("no look or definition is called `grond`")
            && r.contains("3 │")
            && r.contains("did you mean"),
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
    let e = errors("tile flat {\n  stack {\n    ground bumpz=3\n  }\n}\n");
    assert_eq!(e[0].code, "dt::unknown-control");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(
        e[0].help.as_deref().unwrap().contains("bumps"),
        "{:?}",
        e[0].help
    );
}

#[test]
fn a_control_not_built_yet_is_checked_but_not_built() {
    let text = "tile flat {\n  stack {\n    rock wear=worn\n  }\n}\n";
    let src = mem(&[("t.dtile", text)]);
    let c = check(&src, "t.dtile", &Options::default()).unwrap();
    assert_eq!(c.pending().len(), 1); // `wear`
    let Err(e) = build(&src, "t.dtile", &Options::default()) else {
        panic!("built a control that is not built yet");
    };
    assert_eq!(e[0].code, "dt::not-yet");
    assert!(e[0].message.contains("`wear`"), "{}", e[0].message);
    assert_eq!(e[0].location().unwrap().1, 3);
}

#[test]
fn library_tiles_check_and_say_which_look_is_missing_when_built() {
    // the full cliff run (bank, scree, pebbles on the dirt) is built now
    let path = "lib:tiles/test/2x1-cliff-full.dtile";
    let c = check(&Library, path, &Options::default()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    assert!(c.pending().is_empty(), "{path}: {:?}", c.pending());
    // (a palette is not a tile document)
    for path in Library::paths()
        .into_iter()
        .filter(|p| !p.contains("palettes/"))
    {
        check(&Library, &path, &Options::default()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    }
}

#[test]
fn levels_edges_and_printer_come_from_the_library() {
    let c = check(
        &Library,
        "lib:tiles/test/1x1-flat.dtile",
        &Options::default(),
    )
    .unwrap();
    let t = c.tile.unwrap();
    let level = |n: &str| t.levels.iter().find(|(k, _)| k == n).map(|(_, v)| *v);
    assert_eq!(level("edge"), Some(3.4));
    assert_eq!(level("ground"), Some(5.0));
    assert_eq!(level("water"), Some(2.6));
    assert_eq!(level("wall-top"), Some(33.1));
    assert_eq!(
        t.edges,
        vec![("ground".to_owned(), 3.4), ("water".to_owned(), 2.6)]
    );
    let p = Options::default().printer;
    assert_eq!(
        (p.line_mm, p.layer_mm, p.overhang_deg, p.gap_mm),
        (0.45, 0.2, 55.0, 0.2)
    );
}

#[test]
fn a_parts_file_checks_but_does_not_build() {
    let c = check(&Library, "lib:parts/grass.dtile", &Options::default()).unwrap();
    assert!(c.tile.is_none());
    assert_eq!(c.file.doc.definitions.len(), 2);
    let Err(e) = build(&Library, "lib:parts/grass.dtile", &Options::default()) else {
        panic!("built a parts file");
    };
    assert_eq!(e[0].code, "dt::no-tile");
}

#[test]
fn use_paths_name_the_library_or_the_project() {
    let raised = "define raised { stack { ground level=\"ground + 2\" } }\n";
    let src = mem(&[
        ("lib:parts/raised.dtile", raised),
        ("proj/parts/raised.dtile", raised),
        (
            "proj/tiles/a.dtile",
            "use \"parts/raised\" raised\ntile a { stack { raised } }\n",
        ),
        (
            "proj/tiles/b.dtile",
            "use \"lib:parts/raised\" raised\ntile b { stack { raised } }\n",
        ),
        (
            "proj/tiles/c.dtile",
            "use \"../parts/raised\" raised\ntile c { stack { raised } }\n",
        ),
        (
            "proj/tiles/d.dtile",
            "use \"./raised\" raised\ntile d { stack { raised } }\n",
        ),
    ]);
    for (path, from) in [
        ("proj/tiles/a.dtile", "lib:parts/raised.dtile"),
        ("proj/tiles/b.dtile", "lib:parts/raised.dtile"),
        ("proj/tiles/c.dtile", "proj/parts/raised.dtile"),
    ] {
        let t = check(&src, path, &Options::default())
            .unwrap()
            .tile
            .unwrap();
        assert_eq!(t.stack[0].src.name(), from, "{path}");
    }
    let e = check(&src, "proj/tiles/d.dtile", &Options::default())
        .err()
        .unwrap();
    assert_eq!(e[0].location().unwrap().1, 1);
    assert!(
        e[0].message.contains("proj/tiles/raised.dtile"),
        "{}",
        e[0].message
    );
}

#[test]
fn definition_parameters_take_arguments_and_defaults() {
    let src = mem(&[(
        "t.dtile",
        "define raised {\n  param by 2\n  stack { ground level=by }\n}\ntile t {\n  stack { raised by=6 }\n}\n",
    )]);
    let t = check(&src, "t.dtile", &Options::default())
        .unwrap()
        .tile
        .unwrap();
    assert_eq!(t.stack[0].controls[0].value, dt_looks::Value::Number(6.0));
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
    assert!(e[0].message.contains("nothing.dtile"));
}

#[test]
fn imports_inline_definitions_and_levels_resolve() {
    let src = mem(&[
        (
            "parts/flats.dtile",
            "define raised { stack { ground level=\"ground + 2\" } }\n",
        ),
        (
            "tiles/t.dtile",
            "use \"../parts/flats\" raised\ntile t { levels foot=4; stack { raised } }\n",
        ),
    ]);
    let c = check(&src, "tiles/t.dtile", &Options::default()).unwrap();
    let t = c.tile.unwrap();
    assert_eq!(t.stack.len(), 1);
    assert_eq!(t.stack[0].src.name(), "parts/flats.dtile");
    let b = build(&src, "tiles/t.dtile", &Options::default()).unwrap();
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
        "t.dtile",
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
    let b = build(&src, "t.dtile", &Options::default()).unwrap();
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
            .filter(|p| (p[1] - y).abs() < 0.5 && p[2] > 0.0)
            .map(|p| p[2])
            .fold(f64::MIN, f64::max)
    };
    // each level holds inside its region; both meet the tile edge at the
    // edge level (3.4 mm), ramping in no steeper than a hill
    assert!((z_at(8.75) - 5.0).abs() < 1e-5, "low {}", z_at(8.75));
    assert!((z_at(18.5) - 8.0).abs() < 1e-3, "high {}", z_at(18.5));
    assert!((z_at(0.0) - 3.4).abs() < 0.05, "edge {}", z_at(0.0));
}

#[test]
fn an_unknown_region_is_refused() {
    let e = errors(
        "tile t {\n  line across { through west east }\n  region a { bounded-by across; fill \"0.5,0.25\" }\n  stack {\n    ground in=b\n  }\n}\n",
    );
    assert_eq!(e[0].code, "dt::unknown-name");
    assert_eq!(e[0].location().unwrap().1, 5);
    assert_eq!(e[0].help.as_deref(), Some("did you mean `a`?"));
}

#[test]
fn a_later_paint_removes_the_stones_it_covers() {
    let src = mem(&[(
        "t.dtile",
        r#"tile pool size="2x2" seed=2 {
    line across { through west east }
    region low { bounded-by across; fill "1,0.5" }
    region high { bounded-by across; fill "1,1.5" }
    stack {
        dirt level=8 { stones many }
        ground in=low level=5
    }
}
"#,
    )]);
    let b = build(&src, "t.dtile", &Options::default()).unwrap();
    let m = &b.export.mesh;
    let rock_ys: Vec<f64> = m
        .triangles
        .iter()
        .zip(&b.materials)
        .filter(|(_, mat)| **mat == dt_kit::Material::Rock)
        .map(|(t, _)| t.iter().map(|&v| m.positions[v as usize][1]).sum::<f64>() / 3.0)
        .collect();
    assert!(!rock_ys.is_empty(), "the high dirt keeps its stones");
    // the line is at y = 35 mm; the low paint covers every stone below it
    assert!(
        rock_ys.iter().all(|&y| y > 35.0),
        "a stone left on the lowered ground: min y {}",
        rock_ys.iter().copied().fold(f64::MAX, f64::min)
    );
}

#[test]
fn a_rock_placed_past_the_edge_is_refused() {
    let src = mem(&[(
        "t.dtile",
        r#"tile r size="1x1" seed=1 {
    stack {
        ground
        rock at="0.05,0.5" size=10
    }
}
"#,
    )]);
    let Err(e) = build(&src, "t.dtile", &Options::default()) else {
        panic!("built a rock standing past the tile's edge");
    };
    assert!(
        rendered(&e[0]).contains("past the tile's edge"),
        "{}",
        rendered(&e[0])
    );
}

#[test]
fn water_alone_builds_a_tile() {
    let src = mem(&[(
        "t.dtile",
        r#"tile w size="1x1" seed=1 {
    stack { water }
}
"#,
    )]);
    let b = build(&src, "t.dtile", &Options::default()).unwrap();
    assert!(b.materials.contains(&dt_kit::Material::Water));
}

#[test]
fn a_seed_wobbles_the_lines_again() {
    let text = r#"tile step size="1x1" seed=3 {
    line across wobble=4 { through west east }
    region low { bounded-by across; fill "0.5,0.1" }
    region high { bounded-by across; fill "0.5,0.9" }
    stack {
        ground in=high level=8
        ground in=low
    }
}
"#;
    let src = mem(&[("t.dtile", text)]);
    let hash = |seed| {
        let o = Options {
            seed: Some(seed),
            ..Options::default()
        };
        build(&src, "t.dtile", &o).unwrap().stages
    };
    let plain = build(&src, "t.dtile", &Options::default()).unwrap().stages;
    // the tile's own seed again changes nothing; another moves the line
    assert_eq!(hash(3), plain);
    assert_ne!(hash(4), plain);
}

#[test]
fn the_floor_and_wall_tiles_build_with_grass() {
    for path in [
        "lib:tiles/floors/1x1-floor-stone.dtile",
        "lib:tiles/floors/1x1-floor-stone-wall.dtile",
        "lib:tiles/walls/1x1-wall-stone.dtile",
        "lib:tiles/walls/1x1-wall-stone-corner.dtile",
        "lib:tiles/walls/1x1-wall-brick.dtile",
        "lib:tiles/walls/1x1-wall-brick-corner.dtile",
        "lib:tiles/walls/1x1-wall-fieldstone.dtile",
        "lib:tiles/walls/1x1-wall-fieldstone-corner.dtile",
    ] {
        let b =
            build(&Library, path, &Options::default()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
        assert_eq!(b.audit.overhang_mm2, 0.0, "{path}");
    }
}

#[test]
fn a_wall_off_its_area_is_refused() {
    let text = "tile t size=\"1x1\" {\n  line across { through west east }\n  region south { bounded-by across; fill \"0.5,0.25\" }\n  stack {\n    dirt\n    wall in=south along=north style=cut-stone\n  }\n}\n";
    let src = mem(&[("t.dtile", text)]);
    let Err(e) = build(&src, "t.dtile", &Options::default()) else {
        panic!("built a wall off its area");
    };
    assert!(e[0].message.contains("wall"), "{e:?}");
    assert_eq!(e[0].location().unwrap().1, 6, "{e:?}");
}

#[test]
fn a_wall_in_a_definition_needs_along_from_the_call() {
    let def = "define ruin-corner {\n    stack {\n        wall style=cut-stone\n    }\n}\n";
    let without = format!(
        "{def}tile t size=\"1x1\" {{\n    stack {{\n        dirt\n        ruin-corner\n    }}\n}}\n"
    );
    let e = errors(&without);
    assert!(
        e.iter()
            .any(|d| d.code == "dt::missing-control" && rendered(d).contains("along=")),
        "{e:?}"
    );
    let with = format!(
        "{def}tile t size=\"1x1\" {{\n    stack {{\n        dirt\n        ruin-corner along=\"north\"\n    }}\n}}\n"
    );
    let src = mem(&[("t.dtile", &with)]);
    assert!(check(&src, "t.dtile", &Options::default()).is_ok());
}

/// The naming standard (docs/standards/tile-system.html, "File names"): a
/// library tile is `<NxM>-<name>.dtile`, size first, lower case, and the size
/// in the name is the size the document asks for.
#[test]
fn library_tile_file_names_follow_the_standard() {
    let mut bad = Vec::new();
    for path in dt_eval::Library::paths()
        .into_iter()
        .filter(|p| p.contains("tiles/"))
    {
        let file = path.rsplit('/').next().unwrap_or(&path);
        let stem = file.strip_suffix(".dtile").unwrap_or("");
        let Some((size, name)) = stem.split_once('-') else {
            bad.push(format!("{path}: not `<NxM>-<name>.dtile`"));
            continue;
        };
        let dims: Vec<&str> = size.split('x').collect();
        let sized = dims.len() == 2
            && dims
                .iter()
                .all(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
        let named = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '+' || c == '-');
        if !(sized && named) {
            bad.push(format!(
                "{path}: not `<NxM>-<name>.dtile` (lower case, digits, `+`, `-`)"
            ));
            continue;
        }
        let c = check(&Library, &path, &Options::default()).unwrap();
        if let Some(t) = &c.tile {
            let want = format!("{}x{}", t.cols, t.rows);
            if want != size {
                bad.push(format!(
                    "{path}: the name says {size}, the document says {want}"
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
