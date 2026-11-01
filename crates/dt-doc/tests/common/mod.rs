//! The R6 library files, and a tiny stand-in for dt-eval's import
//! resolution: parts are decoded first and made known to the tiles.
#![allow(dead_code)]

use dt_doc::{DocError, Known, decode, model::Document};

pub const HEIGHTS: (&str, &str) = (
    "library/parts/heights.kdl",
    include_str!("../../../../library/parts/heights.kdl"),
);
pub const GRASS: (&str, &str) = (
    "library/parts/grass.kdl",
    include_str!("../../../../library/parts/grass.kdl"),
);
pub const GRASS_TREE: (&str, &str) = (
    "library/tiles/ground/2x2-grass-tree.kdl",
    include_str!("../../../../library/tiles/ground/2x2-grass-tree.kdl"),
);
pub const WALL_CORNER: (&str, &str) = (
    "library/tiles/walls/1x1-wall-fieldstone-corner.kdl",
    include_str!("../../../../library/tiles/walls/1x1-wall-fieldstone-corner.kdl"),
);
pub const CLIFF_RUN: (&str, &str) = (
    "library/tiles/cliffs/2x1-cliff-run.kdl",
    include_str!("../../../../library/tiles/cliffs/2x1-cliff-run.kdl"),
);

pub const ALL: [(&str, &str); 5] = [HEIGHTS, GRASS, GRASS_TREE, WALL_CORNER, CLIFF_RUN];
pub const TILES: [(&str, &str); 3] = [GRASS_TREE, WALL_CORNER, CLIFF_RUN];

/// What every tile sees: the standard heights and the shared parts.
pub fn known() -> Known {
    let mut k = Known::new();
    for (name, text) in [HEIGHTS, GRASS] {
        let doc = decode(name, text, &k).unwrap_or_else(|e| panic!("{name}: {}", render(&e)));
        k.add(&doc);
    }
    k
}

pub fn decode_ok((name, text): (&str, &str)) -> Document {
    decode(name, text, &known()).unwrap_or_else(|e| panic!("{name}:\n{}", render(&e)))
}

/// miette's graphical report, without colour, as a reader sees it.
pub fn render(errors: &[DocError]) -> String {
    let handler =
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
            .with_width(88);
    let mut out = String::new();
    for e in errors {
        handler.render_report(&mut out, e).expect("render");
    }
    out
}

/// One deliberate mistake: the tile, the text to replace, its replacement.
pub struct Case {
    pub title: &'static str,
    pub tile: (&'static str, &'static str),
    pub from: &'static str,
    pub to: &'static str,
}

pub const CASES: [Case; 5] = [
    Case {
        title: "A typo'd control name",
        tile: GRASS_TREE,
        from: "per-square=0.25",
        to: "per-sqare=0.25",
    },
    Case {
        title: "A wrong unit",
        tile: CLIFF_RUN,
        from: "height 1.057",
        to: "height (mm)35",
    },
    Case {
        title: "KDL v1 syntax",
        tile: GRASS_TREE,
        from: "foliage=#true",
        to: "foliage=true",
    },
    Case {
        title: "A missing required control",
        tile: WALL_CORNER,
        from: "wall along=spine family",
        to: "wall family",
    },
    Case {
        title: "A bad distribution",
        tile: CLIFF_RUN,
        from: "size=\"1..6^2\"",
        to: "size=\"1..6^x\"",
    },
];
