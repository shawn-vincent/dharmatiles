//! Dirt follows a loose rock: a dirt listed after a rock lies on its top where
//! that is flatter than `maxSlope`; a dirt listed before it is as it was.

#![allow(clippy::unwrap_used)]

use dt_eval::{Built, Memory, Options, build};
use dt_kit::Material;

fn built(text: &str) -> Built {
    let mem = Memory(std::iter::once(("t.dtile".to_owned(), text.to_owned())).collect());
    build(&mem, "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// A rock in the middle of the tile, a dirt over it listed `after` the rock
/// or before it.
fn tile(rock: &str, after: bool, extra: &str) -> String {
    let rock = format!("rock at=\"0.5,0.5\" {rock}");
    let over = format!("dirt top=\"bottom + [0.6 .. 1.2]\" stones=none {extra}");
    let (a, b) = if after {
        (&rock, &over)
    } else {
        (&over, &rock)
    };
    format!(
        "tile size=\"1x1\" seed=3 {{\n    stack {{\n        base\n        dirt stones=none\n        {a}\n        {b}\n    }}\n}}\n"
    )
}

const MID: [f64; 2] = [17.5, 17.5];

/// The highest earth vertex within `r` mm of the tile's middle, and the
/// highest rock vertex there.
fn tops(b: &Built, r: f64) -> (f64, f64) {
    let top = |m: Material| {
        let mut top = f64::MIN;
        for (t, mat) in b.export.mesh.triangles.iter().zip(&b.materials) {
            if *mat != m {
                continue;
            }
            for &i in t {
                let p = b.export.mesh.positions[i as usize];
                if (p[0] - MID[0]).hypot(p[1] - MID[1]) <= r {
                    top = top.max(p[2]);
                }
            }
        }
        top
    };
    (top(Material::Earth), top(Material::Rock))
}

/// The ground's highest clod there, mm: nothing earth stands higher than this
/// unless it lies on the rock.
const GROUND_MM: f64 = 6.5;

#[test]
fn dirt_lies_on_a_big_flat_topped_rock() {
    let b = built(&tile("shape=flat size=20", true, ""));
    let (earth, rock) = tops(&b, 5.0);
    assert!(rock > GROUND_MM, "the rock stands up: {rock}");
    assert!(
        earth > rock && earth < rock + 2.5,
        "dirt lies on the rock top: earth {earth}, rock {rock}"
    );
}

#[test]
fn dirt_skips_a_rock_steeper_than_max_slope() {
    let b = built(&tile("shape=rounded size=14", true, ""));
    let (earth, rock) = tops(&b, 5.0);
    assert!(rock > GROUND_MM, "the rock stands up: {rock}");
    assert!(earth < GROUND_MM, "no dirt on a dome: earth {earth}");
    // a steeper limit lets it on
    let b = built(&tile("shape=rounded size=14", true, "maxSlope=85"));
    let (earth, _) = tops(&b, 5.0);
    assert!(earth > GROUND_MM, "maxSlope=85 holds dirt on it: {earth}");
}

#[test]
fn dirt_listed_before_the_rock_is_as_it_was() {
    let b = built(&tile("shape=flat size=20", false, ""));
    let (earth, rock) = tops(&b, 5.0);
    assert!(rock > GROUND_MM, "the rock stands up: {rock}");
    assert!(
        earth < GROUND_MM,
        "the rock beds into the dirt, nothing lies on it: earth {earth}"
    );
}
