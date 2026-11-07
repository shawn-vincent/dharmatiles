//! The library files, and a tiny stand-in for dt-eval's import
//! resolution: the prelude and the parts are decoded first and made
//! known to the tiles.
#![allow(dead_code)]

use dt_doc::{DocError, Known, decode, model::Document};

macro_rules! lib {
    ($path:literal) => {
        (
            concat!("library/", $path),
            include_str!(concat!("../../../../library/", $path)),
        )
    };
}

pub const LEVELS: (&str, &str) = lib!("parts/levels.dtile");
pub const EDGES: (&str, &str) = lib!("parts/edges.dtile");
pub const GRASS: (&str, &str) = lib!("parts/grass.dtile");
pub const TREES: (&str, &str) = lib!("parts/trees.dtile");
pub const PRINTER: (&str, &str) = lib!("printers/default.dtile");
pub const FLAT: (&str, &str) = lib!("tiles/test/1x1-flat.dtile");
pub const GRASS_TREE: (&str, &str) = lib!("tiles/ground/2x2-grass-tree.dtile");
pub const WALL_CORNER: (&str, &str) = lib!("tiles/walls/1x1-wall-fieldstone-corner.dtile");
pub const CLIFF_RUN: (&str, &str) = lib!("tiles/test/2x1-cliff-full.dtile");
/// A tile written from scratch: a divided tile, not a library tile.
pub const PATH_ROCKS: (&str, &str) = (
    "tests/fixtures/1x2-grass-path-rocks.dtile",
    include_str!("../fixtures/1x2-grass-path-rocks.dtile"),
);

/// Every file under library/, plus that fixture.
pub const ALL: [(&str, &str); 10] = [
    LEVELS,
    EDGES,
    GRASS,
    TREES,
    PRINTER,
    FLAT,
    GRASS_TREE,
    WALL_CORNER,
    CLIFF_RUN,
    PATH_ROCKS,
];
pub const TILES: [(&str, &str); 5] = [FLAT, GRASS_TREE, WALL_CORNER, CLIFF_RUN, PATH_ROCKS];

/// What every tile sees: the standard levels and the shared parts,
/// registered under the keys their `use` lines name.
pub fn known() -> Known {
    let mut k = Known::standard();
    for (key, (name, text)) in [("parts/grass", GRASS), ("parts/trees", TREES)] {
        let doc = decode(name, text, &k).unwrap_or_else(|e| panic!("{name}: {}", render(&e)));
        k.add(key, &doc);
    }
    k
}

pub fn decode_ok((name, text): (&str, &str)) -> Document {
    decode(name, text, &known()).unwrap_or_else(|e| panic!("{name}:\n{}", render(&e)))
}

/// Decode a snippet; panics with the report if it fails.
pub fn snippet(text: &str) -> Document {
    decode("snippet.dtile", text, &known()).unwrap_or_else(|e| panic!("{text}\n{}", render(&e)))
}

/// Decode a snippet that must fail; returns the errors and the report.
pub fn fails(text: &str) -> (Vec<DocError>, String) {
    let e = decode("snippet.dtile", text, &known()).expect_err(text);
    let shown = render(&e);
    (e, shown)
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

/// A report with miette's line wrapping undone, for `contains` checks.
pub fn flat(shown: &str) -> String {
    shown.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One deliberate mistake in a real tile: the text to replace, and its
/// replacement.  Each has a snapshot of its report in tests/snapshots/.
pub struct Case {
    pub slug: &'static str,
    pub title: &'static str,
    pub tile: (&'static str, &'static str),
    pub from: &'static str,
    pub to: &'static str,
}

impl Case {
    /// The tile with the mistake made.
    pub fn broken(&self) -> String {
        assert!(
            self.tile.1.contains(self.from),
            "{}: `{}` not in {}",
            self.title,
            self.from,
            self.tile.0
        );
        self.tile.1.replacen(self.from, self.to, 1)
    }

    /// The errors and the report an author sees.
    pub fn report(&self) -> (Vec<DocError>, String) {
        let errors = decode(self.tile.0, &self.broken(), &known()).expect_err(self.title);
        let shown = render(&errors);
        (errors, shown)
    }
}

pub const CASES: [Case; 10] = [
    Case {
        slug: "typo",
        title: "A misspelt control",
        tile: GRASS_TREE,
        from: "clumps=32",
        to: "clumsp=32",
    },
    Case {
        slug: "unit-suffix",
        title: "A unit written on a size",
        tile: GRASS_TREE,
        from: "length=4.5",
        to: "length=\"4.5mm\"",
    },
    Case {
        slug: "kdl-v1",
        title: "KDL v1 syntax",
        tile: CLIFF_RUN,
        from: "top dirt",
        to: "top true",
    },
    Case {
        slug: "missing-along",
        title: "A wall with nowhere to stand",
        tile: WALL_CORNER,
        from: "wall along=\"north west\" style",
        to: "wall style",
    },
    Case {
        slug: "unquoted-range",
        title: "A range without quotes",
        tile: CLIFF_RUN,
        from: "size=\"1..6\"",
        to: "size=1..6",
    },
    Case {
        slug: "old-vocabulary",
        title: "Old words",
        tile: GRASS_TREE,
        from: "ground ",
        to: "hills  ",
    },
    Case {
        slug: "thing-count",
        title: "Asking a thing for many",
        tile: GRASS_TREE,
        from: "tree height=40",
        to: "tree height=40 count=3",
    },
    Case {
        slug: "expert-in-tile",
        title: "An expert control in a tile",
        tile: GRASS_TREE,
        from: "leaf length=4.5 width=3",
        to: "branch split=34",
    },
    Case {
        slug: "side-on-edge",
        title: "side= on a tile edge",
        tile: WALL_CORNER,
        from: "seed=14",
        to: "seed=14 side=left",
    },
    Case {
        slug: "several",
        title: "Several mistakes at once",
        tile: CLIFF_RUN,
        from: "rock mixed                  // or: columns, blocks, slabs\n            stone-size massive",
        to: "rock mixd                   // or: columns, blocks, slabs\n            stone-size \"135\"",
    },
];
