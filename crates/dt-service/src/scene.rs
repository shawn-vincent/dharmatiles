//! A map as the page reads it, and a note about it: the wire types of a map's scene.  Reading a map is the binary's
//! (`preview/scene.rs`); where a placed piece stands and how it is turned is here, a plain value.

use serde::{Deserialize, Serialize};

/// A map as the page reads it (`scene`): pieces, grid, bounds, heights.
#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct SceneJson {
    /// The revision of the file this is the reading of.
    pub doc_rev: u64,
    /// A map (placed pieces or a grid) rather than a print list.
    pub map: bool,
    pub grid: Option<SceneGrid>,
    pub about: Option<String>,
    pub pieces: Vec<ScenePiece>,
    /// How many pieces are placed.
    pub placed: usize,
    /// `[x0, y0, x1, y1]` of the placed pieces, mm, y down.
    pub bounds: Option<[f64; 4]>,
    pub heights: Vec<HeightName>,
    /// An inventory exists, so the shelf's word on each piece means something.
    pub shelf_exists: bool,
    /// The pieces the inventory leaves off (`shelf.state` is `have`) and the pieces that print.
    pub have: usize,
    pub to_print: usize,
    /// `single` or `multi`.
    pub colour: String,
    pub system: dt_lang::System,
    /// The square of `system`, mm: where a tile dropped on the map lands.
    pub square_mm: f64,
    pub printer: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct SceneGrid {
    pub cols: u32,
    pub rows: u32,
}

/// A standard or the map's own height name, in dmm.
#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct HeightName {
    pub name: String,
    pub dmm: f64,
    pub own: bool,
}

/// One piece of a map.
#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ScenePiece {
    /// The id the piece has, or the one the first edit of the file will write on it.
    pub id: String,
    /// Whether the file has written it yet.
    pub written: bool,
    /// The tile as the piece names it.
    pub tile: String,
    pub canon: String,
    pub name: String,
    /// The listed tile's id, when the tile is listed.
    pub tile_id: Option<String>,
    pub path: String,
    pub seed: Option<u64>,
    pub seed_used: u64,
    pub system: dt_lang::System,
    pub colour: String,
    pub cols: u32,
    pub rows: u32,
    /// One square, mm.
    pub sq: f64,
    /// Width and depth, mm.
    pub size: [f64; 2],
    pub at: Option<[f64; 2]>,
    pub rot: Option<i64>,
    pub bottom_mm: f64,
    pub bottom: Option<String>,
    pub print: String,
    pub count: u32,
    pub pinned: bool,
    pub error: Option<String>,
    /// What the shelf says of the piece (none for a piece with a mistake).
    pub shelf: Option<ShelfMark>,
    /// The seeds you own of this piece's tile in its system and colour mode, and how many of each.
    pub owned_seeds: Vec<OwnedSeed>,
}

/// A seed you own of a tile, and how many prints of it.
#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct OwnedSeed {
    pub seed: u64,
    pub have: u32,
}

/// What the inventory says of a piece: `have` (a print you own is this piece), `need` (it prints), `other` (you own
/// it in the other colour mode: it still prints), `skip` (`print=skip`).
#[derive(Clone, Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ShelfMark {
    pub state: String,
    pub text: String,
}

/// A note about the scene for the message stream: said where it was found.
#[derive(Clone, Debug)]
pub struct Note {
    pub text: String,
    pub code: &'static str,
    pub help: Option<String>,
}

/// Where a placed piece stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Its north-west corner, across and down from the map's, mm.
    pub x: f64,
    pub y: f64,
    /// The tile's width and depth in its own frame, mm.
    pub w: f64,
    pub d: f64,
    /// Clockwise quarter turns: 0, 1, 2 or 3.
    pub turns: u8,
    /// The height above the map's ground of the tile's zero, mm.
    pub z: f64,
}

impl Placement {
    /// The footprint on the map, `[x0, y0, x1, y1]` with y down.
    pub fn footprint(self) -> [f64; 4] {
        let (w, d) = if self.turns % 2 == 1 {
            (self.d, self.w)
        } else {
            (self.w, self.d)
        };
        [self.x, self.y, self.x + w, self.y + d]
    }

    /// A point of the tile's frame (x east, y north) on the map (y down).
    pub fn on_map(self, px: f64, py: f64) -> [f64; 2] {
        let f = self.footprint();
        let (cw, cd) = ((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0);
        let (dx, dy) = (px - self.w / 2.0, (self.d - py) - self.d / 2.0);
        let (s, c) = match self.turns % 4 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
        [cw + dx * c - dy * s, cd + dx * s + dy * c]
    }

    /// A box of the tile's frame as a box on the map.
    pub fn box_on_map(self, b: [f64; 4]) -> [f64; 4] {
        let corners = [
            self.on_map(b[0], b[1]),
            self.on_map(b[2], b[1]),
            self.on_map(b[2], b[3]),
            self.on_map(b[0], b[3]),
        ];
        let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for c in corners {
            r = [
                r[0].min(c[0]),
                r[1].min(c[1]),
                r[2].max(c[0]),
                r[3].max(c[1]),
            ];
        }
        r
    }
}
