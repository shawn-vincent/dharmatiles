//! Documents end to end through an in-memory source: wrong documents give
//! an error at their line with a hint; imports, regions and the scheduler
//! work on real text.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Diag, Library, Memory, Options, build, check};

/// These tests write their documents in format 1 and read them from memory.
fn legacy() -> Options {
    Options::default()
}

fn mem(files: &[(&str, &str)]) -> Memory {
    common::memory(files)
}

fn errors(text: &str) -> Vec<Diag> {
    match check(&mem(&[("t.dtile", text)]), "t.dtile", &legacy()) {
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
    let e = errors(
        "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirtt\n    }\n}\n",
    );
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].location(), Some(("t.dtile".into(), 4, 9)));
    assert_eq!(e[0].help.as_deref(), Some("did you mean `dirt`?"));
    let r = rendered(&e[0]);
    assert!(
        r.contains("no element or definition is called `dirtt`")
            && r.contains("4 │")
            && r.contains("did you mean"),
        "{r}"
    );
}

#[test]
fn an_unknown_height_lists_the_heights() {
    let wobble = |v: &str| {
        format!(
            "dharmatiles version=1\ntile size=\"1x1\" {{\n  line s wobble=\"{v}\" {{ through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }}\n  stack {{\n    dirt\n  }}\n}}\n"
        )
    };
    let e = errors(&wobble("$grundHeight"));
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(
        e[0].help
            .as_deref()
            .unwrap()
            .starts_with("did you mean `$groundHeight`?"),
        "{:?}",
        e[0].help
    );
    let e = errors(&wobble("$lava"));
    assert_eq!(e[0].code, dt_lang::codes::UNKNOWN_NAME);
    assert!(e[0].help.as_deref().unwrap().contains("heights"), "{e:?}");
}

#[test]
fn an_unknown_control_names_the_ones_there_are() {
    let e = errors(
        "dharmatiles version=1\ntile size=\"1x1\" {\n  stack {\n    dirt bump-siz=3\n  }\n}\n",
    );
    assert_eq!(e[0].code, dt_lang::codes::UNKNOWN_CONTROL);
    assert_eq!(e[0].location().unwrap().1, 4);
    assert!(
        e[0].help.as_deref().unwrap().contains("bumpSize"),
        "{:?}",
        e[0].help
    );
}

#[test]
fn a_control_not_built_yet_is_checked_but_not_built() {
    let text = "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" {\n  stack {\n    rock wear=worn\n  }\n}\n";
    let src = mem(&[("t.dtile", text)]);
    let c = check(&src, "t.dtile", &legacy()).unwrap();
    assert_eq!(c.pending().len(), 1); // `wear`
    let Err(e) = build(&src, "t.dtile", &legacy()) else {
        panic!("built a control that is not built yet");
    };
    assert_eq!(e[0].code, dt_lang::codes::NOT_YET);
    assert!(e[0].message.contains("`wear`"), "{}", e[0].message);
    assert_eq!(e[0].location().unwrap().1, 3);
}

#[test]
fn library_tiles_check_and_say_which_look_is_missing_when_built() {
    // the full cliff run (bank, scree, pebbles on the dirt) is built now
    let path = "lib:tests/cliffs/2x1-cliff-full.dtile";
    let c = check(&Library, path, &legacy()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    assert!(c.pending().is_empty(), "{path}: {:?}", c.pending());
    // (a palette is not a tile document)
    for path in Library::paths()
        .expect("the library is listed")
        .into_iter()
        .filter(|p| !p.contains("palettes/"))
    {
        check(&Library, &path, &legacy()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    }
}

#[test]
fn heights_and_printer_come_from_the_library() {
    let c = check(&Library, "lib:tests/ground/1x1-flat.dtile", &legacy()).unwrap();
    let t = c.tile.unwrap();
    let height = |n: &str| {
        let e = t.heights.consts.get(&format!("${n}"))?;
        t.heights.consts.scalar(&e.expr, 0.5, &|_| None).ok()
    };
    assert_eq!(height("dirtEdgeHeight"), Some(3.4));
    assert_eq!(height("groundHeight"), Some(5.0));
    assert_eq!(height("waterSurfaceHeight"), Some(2.6));
    assert_eq!(height("wallHeight"), Some(33.1));
    let p = legacy().printer;
    assert_eq!(
        (p.line_mm, p.layer_mm, p.overhang_deg, p.gap_mm),
        (0.45, 0.2, 55.0, 0.2)
    );
}

#[test]
fn a_parts_file_checks_but_does_not_build() {
    let c = check(&Library, "lib:parts/grass.dtile", &legacy()).unwrap();
    assert!(c.tile.is_none());
    assert_eq!(c.file.doc.definitions.len(), 2);
    let Err(e) = build(&Library, "lib:parts/grass.dtile", &legacy()) else {
        panic!("built a parts file");
    };
    assert_eq!(e[0].code, dt_lang::codes::NO_TILE);
}

#[test]
fn use_paths_name_the_library_or_the_project() {
    let raised = "dharmatiles version=1\ndefine raised { stack { rock size=\"10dmm\" at=\"(0.5sq, 0.5sq)\" } }\n";
    let src = mem(&[
        ("lib:parts/raised.dtile", raised),
        ("proj/parts/raised.dtile", raised),
        (
            "proj/tiles/a.dtile",
            "dharmatiles version=1\nuse \"parts/raised\" raised\ntile size=\"1x1\" { stack { raised } }\n",
        ),
        (
            "proj/tiles/b.dtile",
            "dharmatiles version=1\nuse \"lib:parts/raised\" raised\ntile size=\"1x1\" { stack { raised } }\n",
        ),
        (
            "proj/tiles/c.dtile",
            "dharmatiles version=1\nuse \"../parts/raised\" raised\ntile size=\"1x1\" { stack { raised } }\n",
        ),
        (
            "proj/tiles/d.dtile",
            "dharmatiles version=1\nuse \"./raised\" raised\ntile size=\"1x1\" { stack { raised } }\n",
        ),
    ]);
    for (path, from) in [
        ("proj/tiles/a.dtile", "lib:parts/raised.dtile"),
        ("proj/tiles/b.dtile", "lib:parts/raised.dtile"),
        ("proj/tiles/c.dtile", "proj/parts/raised.dtile"),
    ] {
        let t = check(&src, path, &legacy()).unwrap().tile.unwrap();
        assert_eq!(t.stack[0].src.name(), from, "{path}");
    }
    let e = check(&src, "proj/tiles/d.dtile", &legacy()).err().unwrap();
    assert_eq!(e[0].location().unwrap().1, 2);
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
        "dharmatiles version=\"1.0.0\"; define sized {\n  param by \"8dmm\"\n  stack { rock size=by }\n}\ntile size=\"1x1\" {\n  stack { dirt; sized by=\"6dmm\" }\n}\n",
    )]);
    let t = check(&src, "t.dtile", &legacy()).unwrap().tile.unwrap();
    assert_eq!(
        t.stack[1].controls[0].value,
        dt_elements::Value::Number(6.0)
    );
}

#[test]
fn kdl_syntax_errors_carry_their_place() {
    let e = errors("tile size=\"1x1\" {\n  stack {\n    dirt top=5mm\n  }\n}\n");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(e[0].help.is_some());
}

#[test]
fn a_bad_import_points_at_the_use_line() {
    let e = errors("use \"lib:parts/nothing\" lawn\ntile size=\"1x1\" { stack { dirt } }\n");
    assert_eq!(e[0].location().unwrap().1, 1);
    assert!(e[0].message.contains("nothing.dtile"));
}

#[test]
fn imports_inline_definitions_and_lets_resolve() {
    let src = mem(&[
        (
            "parts/flats.dtile",
            "dharmatiles version=1\ndefine raised { stack { rock size=\"10dmm\" at=\"(0.5sq, 0.5sq)\" } }\n",
        ),
        (
            "tiles/t.dtile",
            "dharmatiles version=1\nuse \"../parts/flats\" raised\ntile size=\"1x1\" { let foot \"4dmm\"; stack { dirt; raised } }\n",
        ),
    ]);
    let c = check(&src, "tiles/t.dtile", &legacy()).unwrap();
    let t = c.tile.unwrap();
    assert_eq!(t.stack.len(), 2);
    assert_eq!(t.stack[1].src.name(), "parts/flats.dtile");
    let b = build(&src, "tiles/t.dtile", &legacy()).unwrap();
    assert!(b.materials.contains(&dt_kit::Material::Rock));
}

#[test]
fn a_line_and_two_regions_make_a_step() {
    let src = mem(&[(
        "t.dtile",
        r#"dharmatiles version="1.0.0"; tile size="1x1" seed=3 {
    line across { through west east }
    region low { boundedBy across; fill "(0.5sq, 0.75sq)" }
    region high { boundedBy across; fill "(0.5sq, 0.25sq)" }
    stack {
        dirt top="8dmm" {
            pin top="5dmm" at=low
        }
    }
}
"#,
    )]);
    let b = build(&src, "t.dtile", &legacy()).unwrap();
    assert!(
        b.audit.failures(&legacy().printer).is_empty(),
        "{:?}",
        b.audit
    );
    // the surface's height along x = 17.5 (a flat stretch is a few large
    // triangles, so read the surface, not its vertices)
    let z_at = |y: f64| {
        let m = &b.export.mesh;
        m.triangles
            .iter()
            .filter_map(|t| {
                let [a, b, c] = t.map(|i| m.positions[i as usize]);
                let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if d.abs() < 1e-12 {
                    return None;
                }
                let l1 = ((b[1] - c[1]) * (17.5 - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
                let l2 = ((c[1] - a[1]) * (17.5 - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
                let l3 = 1.0 - l1 - l2;
                (l1.min(l2).min(l3) >= -1e-9).then(|| l1 * a[2] + l2 * b[2] + l3 * c[2])
            })
            .fold(f64::MIN, f64::max)
    };
    // each height holds inside its region (the dirt's grain is set into it,
    // so the land sits up to the grain's height under its band); both meet
    // the tile edge at the library's edge height (3.4 mm)
    assert!((z_at(8.75) - 5.0).abs() < 1.5, "low {}", z_at(8.75));
    assert!(
        z_at(26.0) > 4.5 && z_at(26.0) > z_at(8.75),
        "high {}",
        z_at(26.0)
    );
    assert!(
        z_at(35.0) <= 3.4 + 1e-5 && z_at(35.0) > 3.4 - 2.0,
        "edge {}",
        z_at(35.0)
    );
}

#[test]
fn an_unknown_region_is_refused() {
    let e = errors(
        "dharmatiles version=1\ntile size=\"1x1\" {\n  line across { through west east }\n  region a { boundedBy across; fill \"(0.5sq, 0.75sq)\" }\n  stack {\n    dirt at=b\n  }\n}\n",
    );
    assert_eq!(e[0].code, dt_lang::codes::PLACE);
    assert_eq!(e[0].location().unwrap().1, 6);
    assert_eq!(e[0].help.as_deref(), Some("did you mean `a`?"));
}

#[test]
fn a_rock_placed_past_the_edge_is_refused() {
    let src = mem(&[(
        "t.dtile",
        r#"dharmatiles version="1.0.0"; tile size="1x1" seed=1 {
    stack {
        dirt
        rock at="(0.05sq, 0.5sq)" size="10dmm"
    }
}
"#,
    )]);
    let Err(e) = build(&src, "t.dtile", &legacy()) else {
        panic!("built a rock standing past the tile's edge");
    };
    assert!(
        rendered(&e[0]).contains("past the tile's edge"),
        "{}",
        rendered(&e[0])
    );
}

/// A tile with a drawn line and `body` in its stack.
fn on_a_line(body: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"; tile size=\"2x1\" seed=3 {{\n  line road {{ through \"(0.1sq, 0.5sq)\" \"(1.9sq, 0.5sq)\" }}\n  stack {{\n    dirt\n    {body}\n  }}\n}}\n"
    )
}

#[test]
fn a_scatter_at_a_line_builds_for_rocks_and_flowers() {
    for (body, material) in [
        (
            "scatter at=road perSquare=4 reach=\"3dmm\" { rock }",
            dt_kit::Material::Rock,
        ),
        (
            "scatter at=road count=8 reach=\"3dmm\" { flower }",
            dt_kit::Material::Flower,
        ),
    ] {
        let src = mem(&[("t.dtile", &on_a_line(body))]);
        let b = build(&src, "t.dtile", &legacy()).unwrap();
        assert!(b.materials.contains(&material), "{body}");
    }
}

#[test]
fn a_scatter_of_trees_checks_and_a_tree_at_a_line_is_refused() {
    let ok = on_a_line("scatter at=road count=3 { tree top=\"$bottom + 24dmm\" }");
    let c = check(&mem(&[("t.dtile", &ok)]), "t.dtile", &legacy()).unwrap();
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    let text = on_a_line("tree at=road");
    let e = errors(&text);
    assert_eq!(e[0].code, dt_lang::codes::TYPE, "{e:?}");
    assert!(
        e[0].help.as_deref().unwrap().contains("scatter at=<line>"),
        "{e:?}"
    );
}

#[test]
fn a_mixed_scatter_builds_and_is_deterministic() {
    // rocks and flowers, in a region and at a line
    for body in [
        "scatter count=10 { rock size=\"4dmm .. 6dmm\"; flower }",
        "scatter at=road count=8 reach=\"3dmm\" { rock size=\"4dmm .. 6dmm\" weight=2; flower }",
    ] {
        let src = mem(&[("t.dtile", &on_a_line(body))]);
        let a = build(&src, "t.dtile", &legacy()).unwrap();
        assert!(a.materials.contains(&dt_kit::Material::Rock), "{body}");
        assert!(a.materials.contains(&dt_kit::Material::Flower), "{body}");
        let b = build(&src, "t.dtile", &legacy()).unwrap();
        assert_eq!(a.materials, b.materials, "{body}");
        assert_eq!(
            format!("{:?}", a.report.stages),
            format!("{:?}", b.report.stages),
            "{body}"
        );
    }
}

#[test]
fn a_mix_with_trees_checks() {
    let ok = on_a_line(
        "scatter at=road count=4 reach=\"3dmm\" { tree top=\"$bottom + 24dmm\"; rock; flower }",
    );
    let c = check(&mem(&[("t.dtile", &ok)]), "t.dtile", &legacy()).unwrap();
    assert!(c.pending().is_empty(), "{:?}", c.pending());
}

#[test]
fn water_alone_builds_a_tile() {
    let src = mem(&[(
        "t.dtile",
        r#"dharmatiles version="1.0.0"; tile size="1x1" seed=1 {
    stack { water }
}
"#,
    )]);
    let b = build(&src, "t.dtile", &legacy()).unwrap();
    assert!(b.materials.contains(&dt_kit::Material::Water));
}

#[test]
fn a_seed_wobbles_the_lines_again() {
    let text = r#"dharmatiles version="1.0.0"; tile size="1x1" seed=3 {
    line across wobble="4dmm" { through west east }
    region low { boundedBy across; fill "(0.5sq, 0.9sq)" }
    region high { boundedBy across; fill "(0.5sq, 0.1sq)" }
    stack {
        dirt top="8dmm" {
            pin top="5dmm" at=low
        }
    }
}
"#;
    let src = mem(&[("t.dtile", text)]);
    let hash = |seed| {
        let o = Options {
            seed: Some(seed),
            ..legacy()
        };
        build(&src, "t.dtile", &o).unwrap().report.stages
    };
    let plain = build(&src, "t.dtile", &legacy()).unwrap().report.stages;
    // the tile's own seed again changes nothing; another moves the line
    assert_eq!(hash(3), plain);
    assert_ne!(hash(4), plain);
}

#[test]
fn the_floor_and_wall_tiles_build_with_grass() {
    // a straight wall and a corner, each with grass to its foot (the golden
    // tests hold the stone floor with a wall on it)
    for path in [
        "lib:tests/walls/1x1-wall-stone+grass.dtile",
        "lib:tests/walls/1x1-wall-brick-corner+grass.dtile",
    ] {
        let b = build(&Library, path, &legacy()).unwrap_or_else(|e| panic!("{path}: {e:?}"));
        assert_eq!(b.audit.overhang_mm2, 0.0, "{path}");
    }
}

#[test]
fn a_wall_in_a_definition_needs_its_place_from_the_call() {
    let def = "dharmatiles version=\"1.0.0\"; define ruin-corner {\n    stack {\n        wall style=cut-stone\n    }\n}\n";
    let without = format!(
        "{def}tile size=\"1x1\" {{\n    stack {{\n        dirt\n        ruin-corner\n    }}\n}}\n"
    );
    let e = errors(&without);
    assert!(
        e.iter()
            .any(|d| d.code == dt_lang::codes::MISSING_CONTROL && rendered(d).contains("at=")),
        "{e:?}"
    );
    let with = format!(
        "{def}tile size=\"1x1\" {{\n    stack {{\n        dirt\n        ruin-corner at=\"north\"\n    }}\n}}\n"
    );
    let src = mem(&[("t.dtile", &with)]);
    assert!(check(&src, "t.dtile", &legacy()).is_ok());
}

/// The naming standard (docs/standards/tile-system.html, "File names"): a
/// library tile is `<NxM>-<name>.dtile`, size first, lower case, and the size
/// in the name is the size the document asks for.
#[test]
fn library_tile_file_names_follow_the_standard() {
    let mut bad = Vec::new();
    for path in dt_eval::Library::paths()
        .expect("the library is listed")
        .into_iter()
        .filter(|p| p.starts_with("lib:tiles/") || p.starts_with("lib:tests/"))
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
        let c = check(&Library, &path, &legacy()).unwrap();
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

fn tile_with(body: &str) -> String {
    format!("dharmatiles version=\"1.0.0\"; tile size=\"1x1\" seed=3 {{\n{body}\n}}\n")
}

#[test]
fn a_lines_width_makes_a_strip_that_a_surface_follows() {
    let text = tile_with(
        "line trail width=\"12dmm\" { through southCenter northCenter }\n stack {\n base\n dirt\n grass at=trail\n }",
    );
    let src = mem(&[("t.dtile", text.as_str())]);
    let c = check(&src, "t.dtile", &legacy()).unwrap();
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    let b = build(&src, "t.dtile", &legacy()).unwrap();
    assert!(b.materials.contains(&dt_kit::Material::Grass));
    // a wider strip is another tile
    let wide = text.replace("width=\"12dmm\"", "width=\"24dmm\"");
    let hash = |t: &str| {
        build(&mem(&[("t.dtile", t)]), "t.dtile", &legacy())
            .unwrap()
            .report
            .stages
    };
    assert_ne!(hash(&text), hash(&wide));
}

#[test]
fn a_surface_along_a_line_without_a_width_says_to_give_it_one() {
    let e = errors(&tile_with(
        "line trail { through southCenter northCenter }\n stack {\n base\n dirt\n grass at=trail\n }",
    ));
    assert_eq!(e[0].code, dt_lang::codes::TYPE);
    assert!(e[0].message.contains("no width"), "{}", e[0].message);
    assert!(e[0].help.as_deref().unwrap().contains("width="));
}

#[test]
fn a_surface_along_a_tile_edge_points_to_a_region() {
    let e = errors(&tile_with("stack {\n base\n dirt\n grass at=northEdge\n }"));
    assert_eq!(e[0].code, dt_lang::codes::TYPE);
    assert!(e[0].help.as_deref().unwrap().contains("within"));
}

#[test]
fn a_width_is_a_length_above_zero() {
    for w in ["0dmm", "-3dmm", "90dmm"] {
        let e = errors(&tile_with(&format!(
            "line trail width=\"{w}\" {{ through southCenter northCenter }}\n stack {{\n dirt\n }}"
        )));
        assert!(
            [dt_lang::codes::VALUE, dt_lang::codes::RANGE].contains(&e[0].code),
            "{w}: {}",
            e[0].code
        );
    }
}

#[test]
fn a_scatter_along_a_line_reaches_half_its_width_unless_told() {
    let t = |extra: &str| {
        tile_with(&format!(
            "line trail width=\"20dmm\" {{ through southCenter northCenter }}\n stack {{\n base\n dirt\n scatter at=trail count=6 {extra} {{ pebble }}\n }}"
        ))
    };
    let hash = |t: &str| {
        build(&mem(&[("t.dtile", t)]), "t.dtile", &legacy())
            .unwrap()
            .report
            .stages
    };
    // reach=\"10dmm\" is half the width: the same tile
    assert_eq!(hash(&t("")), hash(&t("reach=\"10dmm\"")));
    assert_ne!(hash(&t("")), hash(&t("reach=\"3dmm\"")));
}

#[test]
fn a_scatter_holds_a_definition_of_one_thing() {
    let text = |size: &str| {
        format!(
            "dharmatiles version=\"1.0.0\"; define boulder {{\n param size \"6dmm .. 9dmm\"\n stack {{ rock size=size }}\n}}\ndefine oak {{\n stack {{ boulder size=\"{size}\" }}\n}}\n{}",
            tile_with(
                "stack {\n base\n dirt\n scatter count=3 { boulder size=\"7dmm .. 10dmm\" weight=2; flower }\n }"
            )
            .replacen("dharmatiles version=\"1.0.0\"; ", "", 1)
        )
    };
    let build_of = |t: &str| build(&mem(&[("t.dtile", t)]), "t.dtile", &legacy());
    let c = check(
        &mem(&[("t.dtile", text("5dmm").as_str())]),
        "t.dtile",
        &legacy(),
    )
    .unwrap();
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    let a = build_of(&text("5dmm")).unwrap();
    assert!(a.materials.contains(&dt_kit::Material::Rock) || a.materials.len() > 1);
    // the parameter reaches the rock: a different size is a different tile
    let wide = text("5dmm").replace("\"7dmm .. 10dmm\"", "\"12dmm .. 14dmm\"");
    assert_ne!(a.report.stages, build_of(&wide).unwrap().report.stages);
}

#[test]
fn a_scattered_thing_takes_no_place_word_of_its_own() {
    let e = errors(&tile_with(
        "stack {\n base\n dirt\n scatter count=3 { rock at=center }\n }",
    ));
    assert!(e.iter().any(|d| d.code == dt_lang::codes::PLACE), "{e:?}");
}

#[test]
fn a_point_drawn_from_a_range_is_drawn_once_from_the_tile_seed() {
    let text = |seed: u32| {
        format!(
            "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" seed={seed} {{\n line shore {{ through \"(0sq, 0.4sq .. 0.7sq)\" \"(bell(0.3sq .. 0.7sq), 0.4sq .. 0.6sq)\" \"(1sq, 0.4sq .. 0.7sq)\" }}\n region low {{ boundedBy shore; fill \"(0.4sq .. 0.6sq, 0.85sq .. 0.95sq)\" }}\n stack {{\n dirt top=\"8dmm\" {{\n pin top=\"5dmm\" at=low\n }}\n }}\n}}\n"
        )
    };
    let check_of = |t: &str, seed: Option<u64>| {
        let o = Options { seed, ..legacy() };
        check(&mem(&[("t.dtile", t)]), "t.dtile", &o).unwrap()
    };
    let c = check_of(&text(3), None);
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    let t = c.tile.as_ref().unwrap();
    let shore = &t.lines["shore"];
    assert_eq!(shore.len(), 3);
    // each point lands inside its range, and a plain number stays put
    assert!((0.3..=0.6).contains(&shore[0][1]) && shore[0][0] == 0.0);
    assert!((0.3..=0.7).contains(&shore[1][0]) && (0.4..=0.6).contains(&shore[1][1]));
    assert!((0.05..=0.15).contains(&t.regions[0].fills[0][1]));
    // the same seed draws the same line; another seed, and `--seed`, draw again
    assert_eq!(
        &check_of(&text(3), None).tile.unwrap().lines["shore"],
        shore
    );
    assert_ne!(
        &check_of(&text(4), None).tile.unwrap().lines["shore"],
        shore
    );
    let moved = check_of(&text(3), Some(4)).tile.unwrap();
    assert_eq!(
        moved.lines["shore"],
        check_of(&text(4), None).tile.unwrap().lines["shore"]
    );
    assert_eq!(
        moved.regions[0].fills,
        check_of(&text(4), None).tile.unwrap().regions[0].fills
    );
    // and it builds
    build(&mem(&[("t.dtile", text(3).as_str())]), "t.dtile", &legacy()).unwrap();
}

#[test]
fn a_drawn_point_off_the_tile_is_refused_where_it_is_written() {
    let e = errors(&tile_with(
        "line s { through \"(0sq, 0.5sq)\" \"(0.5sq .. 1.5sq, 0.5sq)\" }\n stack {\n dirt\n }",
    ));
    assert!(e[0].location().is_some(), "{e:?}");
}

// ---- fixes: a suggestion is a button, made with its help ----

/// `text` with the diagnostic's fix applied, as the editor lays it in.
fn mended(text: &str, d: &Diag) -> String {
    let f = d.fix.as_ref().expect("the diagnostic carries a fix");
    assert_eq!(
        &text[f.offset..f.offset + f.len],
        f.was,
        "the fix is for the text it points at"
    );
    format!(
        "{}{}{}",
        &text[..f.offset],
        f.replacement,
        &text[f.offset + f.len..]
    )
}

/// The same document, mended once, checks clean.
fn mends(text: &str, expect: &str) {
    let e = errors(text);
    assert_eq!(e.len(), 1, "{e:?}");
    let help = e[0].help.as_deref().unwrap();
    let f = e[0].fix.as_ref().expect("a fix");
    assert!(
        help.starts_with(&format!("did you mean `{expect}`?")),
        "{help}"
    );
    assert_eq!(f.label, format!("Change to {expect}"));
    let next = mended(text, &e[0]);
    assert!(
        check(&mem(&[("t.dtile", &next)]), "t.dtile", &legacy()).is_ok(),
        "{next}"
    );
}

#[test]
fn a_misspelt_element_carries_the_fix_that_mends_it() {
    mends(
        "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirtt\n    }\n}\n",
        "dirt",
    );
}

#[test]
fn a_misspelt_region_and_a_misspelt_height_carry_fixes() {
    mends(
        "dharmatiles version=1\ntile size=\"1x1\" {\n  line across { through west east }\n  region a { boundedBy across; fill \"(0.5sq, 0.75sq)\" }\n  stack {\n    dirt at=b\n  }\n}\n",
        "a",
    );
    mends(
        "dharmatiles version=1\ntile size=\"1x1\" {\n  line s wobble=\"$grundHeight\" { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }\n  stack {\n    dirt\n  }\n}\n",
        "$groundHeight",
    );
}

#[test]
fn a_quoted_word_stays_quoted_and_a_control_name_is_replaced_alone() {
    // a word written in quotes is replaced in quotes
    let (help, fix) = dt_doc::error::suggest("a=\"lfet\"", (2, 6).into(), "left");
    assert_eq!(help, "did you mean `left`?");
    assert_eq!(
        (fix.replacement.as_str(), fix.was.as_str()),
        ("\"left\"", "\"lfet\"")
    );
    let text = "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirt bumpSiez=30\n    }\n}\n";
    let e = errors(text);
    assert_eq!(e[0].code, dt_lang::codes::UNKNOWN_CONTROL);
    assert_eq!(
        mended(text, &e[0]).lines().nth(3).unwrap(),
        "        dirt bumpSize=30"
    );
}
