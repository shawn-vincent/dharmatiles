//! EXPERIMENT (segv-experiment branch): the wall and floor tiles built again
//! and again at once, to catch the level-set crash without the 45 s suite.
#![allow(clippy::unwrap_used)]
use dt_eval::{Library, Options, build};
use rayon::prelude::*;

const TILES: &[&str] = &[
    "lib:tiles/test/1x1-wall-cut-stone-corner.dtile",
    "lib:tiles/test/1x1-wall-fieldstone.dtile",
    "lib:tiles/test/1x1-floor-stone.dtile",
    "lib:tiles/test/1x1-flagstones-heaved.dtile",
    "lib:tiles/test/1x1-planks.dtile",
    "lib:tiles/test/1x1-wall-rim.dtile",
    "lib:tiles/floors/1x1-floor-stone-wall.dtile",
    "lib:tiles/walls/1x1-wall-stone.dtile",
];

#[test]
#[ignore]
fn loop_wall_builds() {
    let n: usize = std::env::var("SEGV_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
    for k in 0..n {
        TILES.par_iter().for_each(|p| {
            build(&Library, p, &Options::default()).unwrap_or_else(|e| panic!("{p}: {e:?}"));
        });
        eprintln!("iter {k} ok");
    }
}
