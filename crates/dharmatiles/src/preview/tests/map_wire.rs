//! The map as the program writes it, held for the page's tests: `web/preview/tests/fixtures/tile-map.json` is the
//! `TileMap` of `tile-map.dtile` beside it (the page's overlay and map tests read the same two files), so a test of the
//! page draws a whole map, never a hand-made fragment.  The page's tsc checks the fixture against the generated `TileMap`.

use dt_eval::{Files, check};

const MAP: &str = include_str!("../../../../../web/preview/tests/fixtures/tile-map.json");
const DOCUMENT: &str = "../../web/preview/tests/fixtures/tile-map.dtile";

#[test]
fn the_map_fixture_is_the_map_the_program_makes_of_its_document() {
    let opts = crate::library::options_in(dt_lang::System::Db, "default", None, None)
        .expect("default options");
    let path = format!("{}/{DOCUMENT}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("the fixture document reads");
    let checked = check(&Files, &path, &opts).expect("the fixture document checks");
    let made = dt_eval::map::json(&checked, &text, opts.units())
        .expect("the regions' masks are made")
        .expect("the document has a tile");
    // compared as written: the fixture's numbers are the program's own digits
    let written = serde_json::to_string(&made).expect("a map writes");
    assert_eq!(
        written,
        MAP.trim_end(),
        "tile-map.json is what the program makes of tile-map.dtile; rewrite it with `dharmatiles poster` if the map changed"
    );
    let _: dt_eval::map::TileMap = serde_json::from_str(MAP).expect("the fixture is a TileMap");
}
