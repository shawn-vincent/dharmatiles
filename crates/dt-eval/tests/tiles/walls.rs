//! The wall tiles, built once with the others (`memo`).

use super::memo::built;

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_floor_and_wall_tiles_build_with_grass() {
    the_floor_and_wall_tiles_build_with_grass_on(&["lib:tests/walls/1x1-wall-stone+grass.dtile"]);
}

fn the_floor_and_wall_tiles_build_with_grass_on(paths: &[&str]) {
    // a straight wall and a corner, each with grass to its foot (the golden
    // tests hold the stone floor with a wall on it)
    for &path in paths {
        assert_eq!(built(path).audit.overhang_mm2, 0.0, "{path}");
    }
}
