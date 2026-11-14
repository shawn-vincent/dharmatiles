//! The wall tiles, built once with the others (`memo`).

use super::memo::built;

#[test]
fn the_floor_and_wall_tiles_build_with_grass() {
    // a straight wall and a corner, each with grass to its foot (the golden
    // tests hold the stone floor with a wall on it)
    for path in [
        "lib:tests/walls/1x1-wall-stone+grass.dtile",
        "lib:tests/walls/1x1-wall-brick-corner+grass.dtile",
    ] {
        assert_eq!(built(path).audit.overhang_mm2, 0.0, "{path}");
    }
}
