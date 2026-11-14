//! `cargo xtask pictures`: the document elements' pictures, built from the
//! demo tiles in `docs/elements/demo/` and written to `docs/elements/img/`:
//!
//! - `el-line-straight.jpg`, `el-line.jpg`: `line.dtile` from above with
//!   wobble 0 and 5, its points and the straight path through them in brass
//! - `el-region.jpg`, `el-use.jpg`, `el-define.jpg`: those demos, painted
//! - `el-stack-1.jpg` … `el-stack-6.jpg`: `stack.dtile` with its first
//!   1 … 6 stack lines
//! - `el-wall-*.jpg`: the wall page's families, from library tiles and demos
//! - `el-meadow.jpg`: the composing page's meadow
//!
//! `cargo xtask pictures line stack` rebuilds only those groups (line,
//! region, use, define, stack, walls, floors, meadow).
//!
//! Labels are not drawn into the pictures: the pages set them in the
//! site's type.

use std::path::{Path, PathBuf};

use crate::image::Rgb;

const BRASS: [u8; 3] = [168, 114, 15];
const WHITE: [u8; 3] = [255, 255, 255];
const QUALITY: u8 = 86;

/// The floor pages' pictures: (document, view, picture).
const FLOORS: [(&str, &str, &str); 5] = [
    (
        "lib:tiles/floors/1x1-floor-stone.dtile",
        "painted",
        "el-flagstones.jpg",
    ),
    (
        "demo:flagstones-heaved",
        "painted",
        "el-flagstones-heaved.jpg",
    ),
    (
        "lib:tiles/floors/1x1-floor-stone.dtile",
        "low",
        "el-flagstones-edge.jpg",
    ),
    (
        "lib:tiles/floors/1x1-floor-planks.dtile",
        "painted",
        "el-planks.jpg",
    ),
    (
        "lib:tiles/floors/1x1-floor-planks.dtile",
        "close",
        "el-planks-close.jpg",
    ),
];

/// The wall page's families: (document, picture).
const WALLS: [(&str, &str); 4] = [
    (
        "lib:tiles/walls/1x1-wall-stone.dtile",
        "el-wall-cut-stone.jpg",
    ),
    ("demo:wall-battlements", "el-wall-battlements.jpg"),
    ("demo:flagstones-fieldstone", "el-wall-fieldstone.jpg"),
    ("demo:wall-ruins", "el-wall-ruins.jpg"),
];

pub fn run(root: &Path, only: &[String]) -> Result<(), String> {
    let want = |group: &str| only.is_empty() || only.iter().any(|o| o == group);
    let exe = crate::program(root, true)?;
    let demo = root.join("docs/elements/demo");
    let img = root.join("docs/elements/img");
    let tmp = root.join("out/pictures");
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));

    // Build a document and load one view. `name` is a demo's stem, or a
    // `lib:` path; `text`, when given, is a variant written to `tmp` first.
    let view = |name: &str, text: Option<&str>, which: &str| -> Result<Rgb, String> {
        let stem = name
            .rsplit('/')
            .next()
            .unwrap_or(name)
            .trim_end_matches(".dtile");
        let doc = match text {
            Some(t) => {
                let p = tmp.join(format!("{stem}.dtile"));
                std::fs::create_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
                std::fs::write(&p, t).map_err(|e| format!("{}: {e}", p.display()))?;
                p.to_string_lossy().into_owned()
            }
            None if name.starts_with("lib:") => name.to_owned(),
            None => demo
                .join(format!("{stem}.dtile"))
                .to_string_lossy()
                .into_owned(),
        };
        let dir = tmp.join(stem);
        crate::build_views(&exe, &doc, None, None, &dir)?;
        let png = crate::find_pngs(&dir)
            .into_iter()
            .find(|p| p.to_string_lossy().ends_with(&format!("-db-{which}.png")))
            .ok_or_else(|| format!("{name}: no {which} view"))?;
        Rgb::read_png(&png)
    };
    let save = |im: Rgb, name: &str, width: usize| -> Result<(), String> {
        let path: PathBuf = img.join(name);
        im.fit_width(width).write_jpeg(&path, QUALITY)?;
        println!("pictures: {}", path.display());
        Ok(())
    };

    // line: the same points, straight and wobbled
    if want("line") {
        let line = read(&demo.join("line.dtile"))?;
        let points = points_of(&line)?;
        let straight = line.replace("wobble=\"5dmm\"", "wobble=\"0dmm\"");
        for (name, text, out) in [
            (
                "line-straight",
                Some(straight.as_str()),
                "el-line-straight.jpg",
            ),
            ("line", None, "el-line.jpg"),
        ] {
            let mut top = view(name, text, "top")?;
            mark_points(&mut top, &points)?;
            save(top, out, 640)?;
        }
    }

    for stem in ["region", "use", "define"] {
        if want(stem) {
            save(view(stem, None, "painted")?, &format!("el-{stem}.jpg"), 900)?;
        }
    }

    if want("meadow") {
        save(view("meadow", None, "painted")?, "el-meadow.jpg", 900)?;
    }

    if want("walls") {
        for (doc, out) in WALLS {
            save(
                view(doc.trim_start_matches("demo:"), None, "painted")?,
                out,
                700,
            )?;
        }
    }

    if want("floors") {
        for (doc, which, out) in FLOORS {
            let name = doc.trim_start_matches("demo:");
            save(view(name, None, which)?, out, 700)?;
        }
    }

    // stack: one line at a time
    if !want("stack") {
        return Ok(());
    }
    let stack = read(&demo.join("stack.dtile"))?;
    let lines = stack_lines(&stack)?;
    for k in 1..=lines {
        let text = stack_prefix(&stack, k)?;
        let name = format!("stack-{k}");
        save(
            view(&name, Some(&text), "painted")?,
            &format!("el-{name}.jpg"),
            520,
        )?;
    }
    Ok(())
}

/// The quoted `"(Xsq, Ysq)"` points of the first `through` in `text`, in
/// squares with y up (format 1 counts down from the north-west corner, so
/// y is taken from the tile's `rows`).
fn points_of(text: &str) -> Result<Vec<(f64, f64)>, String> {
    let size = text
        .split_once("size=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .ok_or("line.dtile: no `size=\"…\"`")?
        .0;
    let rows: f64 = size
        .split_once('x')
        .and_then(|(_, r)| r.parse().ok())
        .ok_or_else(|| format!("line.dtile: bad size {size:?}"))?;
    let body = text
        .split_once("{ through")
        .and_then(|(_, rest)| rest.split_once('}'))
        .ok_or("line.dtile: no `{ through … }`")?
        .0;
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(|p| {
            let (x, y) = p
                .trim_matches(['(', ')'])
                .split_once(',')
                .ok_or_else(|| format!("bad point {p:?}"))?;
            // a number of squares, or an exact fraction of one (`2sq/3`)
            let num = |v: &str| {
                let v = v.trim().replace("sq", "");
                let one = |t: &str| t.parse::<f64>().map_err(|e| format!("{p:?}: {e}"));
                match v.split_once('/') {
                    Some((a, b)) => Ok(one(a)? / one(b)?),
                    None => one(&v),
                }
            };
            Ok((num(x)?, rows - num(y)?))
        })
        .collect()
}

/// Draw the line's points (in squares, y up) and the straight dashed path
/// through them, in brass edged with white.
fn mark_points(im: &mut Rgb, points: &[(f64, f64)]) -> Result<(), String> {
    let (x0, y0, x1, y1) = im.footprint().ok_or("no tile in the top view")?;
    let px: Vec<(f64, f64)> = points
        .iter()
        .map(|(u, v)| (x0 + u * (x1 - x0), y1 - v * (y1 - y0)))
        .collect();
    let lerp =
        |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    for pair in px.windows(2) {
        const DASHES: usize = 22;
        for (width, colour) in [(5.0, WHITE), (3.0, BRASS)] {
            for i in (0..DASHES).step_by(2) {
                let t = |i: usize| i as f64 / DASHES as f64;
                im.stroke(
                    lerp(pair[0], pair[1], t(i)),
                    lerp(pair[0], pair[1], t(i + 1)),
                    width,
                    colour,
                );
            }
        }
    }
    for &(x, y) in &px {
        im.disc(x, y, 11.0, WHITE);
        im.disc(x, y, 8.0, BRASS);
    }
    Ok(())
}

/// The stack's head, its non-blank lines, and the rest of the document.
fn split_stack(text: &str) -> Result<(&str, Vec<&str>, &str), String> {
    let (head, rest) = text
        .split_once("stack {")
        .ok_or("stack.dtile: no `stack {`")?;
    let (body, tail) = rest
        .split_once("\n    }")
        .ok_or("stack.dtile: no closing `}`")?;
    Ok((
        head,
        body.lines().filter(|l| !l.trim().is_empty()).collect(),
        tail,
    ))
}

fn stack_lines(text: &str) -> Result<usize, String> {
    Ok(split_stack(text)?.1.len())
}

/// `text` with only the first `k` lines of its stack.
fn stack_prefix(text: &str, k: usize) -> Result<String, String> {
    let (head, lines, tail) = split_stack(text)?;
    Ok(format!(
        "{head}stack {{\n{}\n    }}{tail}",
        lines[..k.min(lines.len())].join("\n")
    ))
}
