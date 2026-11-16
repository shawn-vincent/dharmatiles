//! Where a piece goes and which seed it gets, said as an intent in the edit and settled by the program in the same
//! write, under the document's lock.
//!
//! An edit can ask for a piece "near (x, y)" (the nearest free square to that point) or for a seed that is "fresh" (one
//! nobody owns, that the map does not use, and not repeated within the edit).  The program settles both against the
//! map as it is when the write is made, with every piece placed so far, those of the same edit included, so another edit
//! or another tab cannot take the square or the seed between an answer and a write: there is no answer to wait on.
//! The edit's answer says where each piece went and which seed it got.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::scene::{SceneJson, ScenePiece};

/// A wish for a place: the point, in squares of the piece's own system.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, ts_rs::TS)]
#[serde(deny_unknown_fields)]
pub struct Near {
    pub x: f64,
    pub y: f64,
}

/// What a new piece's seed is to be when none is given.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum SeedWord {
    /// One nobody owns and the map does not use.
    Fresh,
    /// The tile's own seed, unless you own one of it or the map uses it: then a fresh one.
    IfNeeded,
}

/// What a piece takes up of the map: its size, its square and the height it stands at.
#[derive(Clone, Copy)]
pub struct Geom {
    pub w: f64,
    pub d: f64,
    pub sq: f64,
    pub bottom: f64,
}

impl Geom {
    pub fn of(p: &ScenePiece) -> Self {
        Self {
            w: p.size[0],
            d: p.size[1],
            sq: p.sq,
            bottom: p.bottom_mm,
        }
    }
}

fn is_turned(rot: i64) -> bool {
    (rot.rem_euclid(360) / 90) % 2 == 1
}

fn footprint(at: [f64; 2], g: &Geom, rot: i64) -> [f64; 4] {
    let (w, d) = if is_turned(rot) {
        (g.d, g.w)
    } else {
        (g.w, g.d)
    };
    [at[0], at[1], at[0] + w, at[1] + d]
}

fn overlaps(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01
}

/// Settles intents against one map, in the order the edit asks.
pub struct Resolver {
    /// What the user owns, when an inventory exists and reads.
    shelf: Option<dt_doc::inventory::Inventory>,
    scene: SceneJson,
    /// The footprints taken, with the height each stands at: the pieces placed (but those the edit moves) and those it placed.
    taken: Vec<(f64, [f64; 4])>,
    /// The stream fresh seeds are drawn from: a word the host gave (its entropy), advanced for every number drawn.
    entropy: u64,
    /// Seeds handed out in this edit, by tile and system.
    given: BTreeMap<(String, dt_lang::System), BTreeSet<u64>>,
    /// How many places went to a square other than the one wished.
    pub shifted: usize,
}

impl Resolver {
    /// `entropy`: a number nobody chose, from [`crate::scene_read::SceneHost::entropy`]: the host's own source, since what a
    /// seed that merely has to be new is drawn from differs by where the program runs.  `leave`: the pieces this edit
    /// moves, which do not stand in the way of their own new places.
    #[must_use]
    pub fn new(
        entropy: u64,
        shelf: Option<dt_doc::inventory::Inventory>,
        scene: SceneJson,
        leave: &[&str],
    ) -> Self {
        let taken = scene
            .pieces
            .iter()
            .filter(|p| !leave.contains(&p.id.as_str()))
            .filter_map(|p| {
                let at = p.at?;
                Some((p.bottom_mm, footprint(at, &Geom::of(p), p.rot.unwrap_or(0))))
            })
            .collect();
        Self {
            shelf,
            scene,
            taken,
            entropy,
            given: BTreeMap::new(),
            shifted: 0,
        }
    }

    pub fn scene(&self) -> &SceneJson {
        &self.scene
    }

    pub fn piece(&self, id: &str) -> Result<&ScenePiece, String> {
        self.scene
            .pieces
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("the map has no piece {id}"))
    }

    fn clash(&self, g: &Geom, f: [f64; 4]) -> bool {
        self.taken
            .iter()
            .any(|(b, o)| (b - g.bottom).abs() < 0.5 && overlaps(f, *o))
    }

    /// The free place nearest to `want` (squares) for a piece of `g` turned `rot`, in rings out from the wish, and its
    /// footprint is taken from then on.  `(x, y)` in squares.
    ///
    /// # Errors
    /// There is no free place within 40 squares.
    pub fn place(&mut self, g: &Geom, rot: i64, want: Near) -> Result<(f64, f64), String> {
        let boxat = |x: f64, y: f64| footprint([x * g.sq, y * g.sq], g, rot);
        let found = if self.clash(g, boxat(want.x, want.y)) {
            let mut found = None;
            'rings: for r in 1..=40_i32 {
                let mut best: Option<(f64, f64, f64)> = None;
                for dx in -r..=r {
                    for dy in -r..=r {
                        if dx.abs().max(dy.abs()) != r {
                            continue;
                        }
                        let (x, y) = (want.x + f64::from(dx), want.y + f64::from(dy));
                        if self.clash(g, boxat(x, y)) {
                            continue;
                        }
                        let d = libm::hypot(f64::from(dx), f64::from(dy));
                        if best.is_none_or(|b| d < b.2) {
                            best = Some((x, y, d));
                        }
                    }
                }
                if let Some((x, y, _)) = best {
                    found = Some((x, y));
                    self.shifted += 1;
                    break 'rings;
                }
            }
            found
        } else {
            Some((want.x, want.y))
        };
        let (x, y) = found.ok_or("there is no free place for that on the map")?;
        self.taken.push((g.bottom, boxat(x, y)));
        Ok((x, y))
    }

    /// The seeds you own of this tile in this system and colour.
    fn owned(
        &self,
        canon: &str,
        system: dt_lang::System,
        colour: dt_lang::Colour,
    ) -> BTreeSet<u64> {
        let Some(l) = &self.shelf else {
            return BTreeSet::new();
        };
        l.pieces()
            .iter()
            .filter(|p| {
                p.have > 0 && p.id.tile == canon && p.id.system == system && p.id.colour == colour
            })
            .map(|p| p.id.seed)
            .collect()
    }

    /// A seed for a piece of `canon` in `system`: one you do not own, that no piece of the map and no earlier one of this
    /// edit has, and that is not `own` (the tile's own).
    ///
    /// # Errors
    /// None was found in a thousand tries.
    pub fn fresh(
        &mut self,
        canon: &str,
        system: dt_lang::System,
        colour: dt_lang::Colour,
        own: Option<u64>,
    ) -> Result<u64, String> {
        let owned = self.owned(canon, system, colour);
        let key = (canon.to_owned(), system);
        let mut used: BTreeSet<u64> = self
            .scene
            .pieces
            .iter()
            .filter(|p| p.canon == canon && p.system == system)
            .map(|p| p.seed_used)
            .collect();
        used.extend(self.given.get(&key).into_iter().flatten());
        for _ in 0..1000 {
            let s = 1 + self.draw() % 99_998;
            if !owned.contains(&s) && !used.contains(&s) && Some(s) != own {
                self.given.entry(key).or_default().insert(s);
                return Ok(s);
            }
        }
        Err("no free seed in a thousand tries".to_owned())
    }

    /// The next number of the stream (splitmix64: every word of a stream is its own).
    fn draw(&mut self) -> u64 {
        self.entropy = self.entropy.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.entropy;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Does this tile in this system have a seed you own, or a piece on the map?  (A new piece of it then needs a fresh one.)
    /// Gives the map's piece's seed when there is one.
    pub fn taken_seed(
        &self,
        canon: &str,
        system: dt_lang::System,
        colour: dt_lang::Colour,
    ) -> Option<Option<u64>> {
        let on_map = self
            .scene
            .pieces
            .iter()
            .find(|p| p.at.is_some() && p.canon == canon && p.system == system)
            .map(|p| p.seed_used);
        if on_map.is_some() || !self.owned(canon, system, colour).is_empty() {
            Some(on_map)
        } else {
            None
        }
    }
}

/// `(2sq, 1.5sq)`: a place as the document writes it, in the piece's own squares.
pub fn at_word(x: f64, y: f64) -> String {
    let num = |v: f64| format!("{}", (v * 1000.0).round() / 1000.0);
    format!("({}sq, {}sq)", num(x), num(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{HeightName, SceneJson};

    fn piece(id: &str, at: Option<[f64; 2]>, size: [f64; 2], bottom: f64, seed: u64) -> ScenePiece {
        ScenePiece {
            id: id.to_owned(),
            written: true,
            tile: "t".to_owned(),
            canon: "home:t".to_owned(),
            name: "t".to_owned(),
            tile_id: None,
            path: "t.dtile".to_owned(),
            seed: None,
            seed_used: seed,
            system: dt_lang::System::Db,
            colour: dt_lang::Colour::Single,
            cols: 1,
            rows: 1,
            sq: 35.0,
            size,
            at,
            rot: Some(0),
            bottom_mm: bottom,
            bottom: None,
            print: "missing".to_owned(),
            count: 1,
            pinned: false,
            error: None,
            shelf: None,
            owned_seeds: Vec::new(),
        }
    }

    fn scene(pieces: Vec<ScenePiece>) -> SceneJson {
        let _: Option<HeightName> = None;
        SceneJson {
            doc_rev: 1,
            map: true,
            grid: None,
            about: None,
            placed: pieces.iter().filter(|p| p.at.is_some()).count(),
            pieces,
            bounds: None,
            heights: Vec::new(),
            shelf_exists: false,
            have: 0,
            to_print: 0,
            colour: dt_lang::Colour::Single,
            system: dt_lang::System::Db,
            square_mm: 35.0,
            printer: "default".to_owned(),
        }
    }

    const G: Geom = Geom {
        w: 35.0,
        d: 35.0,
        sq: 35.0,
        bottom: 0.0,
    };

    #[test]
    fn the_nearest_free_place_is_the_wish_when_it_is_free_else_the_nearest_square_that_is() {
        let a = piece("a", Some([0.0, 0.0]), [35.0, 35.0], 0.0, 1);
        let mut rs = Resolver::new(7, None, scene(vec![a]), &[]);
        assert_eq!(
            rs.place(&G, 0, Near { x: 3.0, y: 0.0 }).unwrap(),
            (3.0, 0.0)
        );
        assert_eq!(rs.shifted, 0);
        // the wish is taken: a square beside it, and the answer counts as moved
        let (x, y) = rs.place(&G, 0, Near { x: 0.0, y: 0.0 }).unwrap();
        assert!(
            (x.abs() + y.abs() - 1.0).abs() < 1e-9 && (x, y) != (3.0, 0.0),
            "{x} {y}"
        );
        assert_eq!(rs.shifted, 1);
        // a piece asked for after sees the ones placed before it in the same edit
        let (x2, y2) = rs.place(&G, 0, Near { x: 0.0, y: 0.0 }).unwrap();
        assert_ne!((x2, y2), (x, y));
        assert_ne!((x2, y2), (0.0, 0.0));
    }

    #[test]
    fn a_piece_at_another_height_is_not_in_the_way_and_a_turned_one_is_searched_turned() {
        let a = piece("a", Some([0.0, 0.0]), [35.0, 35.0], 0.0, 1);
        let mut rs = Resolver::new(7, None, scene(vec![a]), &[]);
        let high = Geom { bottom: 33.0, ..G };
        assert_eq!(
            rs.place(&high, 0, Near { x: 0.0, y: 0.0 }).unwrap(),
            (0.0, 0.0),
            "it floats over"
        );
        // a 2x1 turned stands 1x2: beside a wall at (1, 0) it fits at (0, 0)
        let wall = piece("w", Some([35.0, 0.0]), [35.0, 35.0], 0.0, 2);
        let mut rs = Resolver::new(7, None, scene(vec![wall]), &[]);
        let long = Geom {
            w: 70.0,
            d: 35.0,
            ..G
        };
        assert_eq!(
            rs.place(&long, 90, Near { x: 0.0, y: 0.0 }).unwrap(),
            (0.0, 0.0)
        );
        // the pieces an edit moves do not stand in the way of their own new places
        let mover = piece("m", Some([0.0, 0.0]), [35.0, 35.0], 0.0, 3);
        let mut rs = Resolver::new(7, None, scene(vec![mover]), &["m"]);
        assert_eq!(
            rs.place(&G, 0, Near { x: 0.0, y: 0.0 }).unwrap(),
            (0.0, 0.0)
        );
    }

    #[test]
    fn a_fresh_seed_is_not_the_tiles_own_not_on_the_map_and_not_repeated() {
        let a = piece("a", Some([0.0, 0.0]), [35.0, 35.0], 0.0, 4);
        let mut rs = Resolver::new(7, None, scene(vec![a]), &[]);
        let mut seen = BTreeSet::new();
        for _ in 0..200 {
            let s = rs
                .fresh(
                    "home:t",
                    dt_lang::System::Db,
                    dt_lang::Colour::Single,
                    Some(5),
                )
                .unwrap();
            assert!(s != 4 && s != 5 && (1..=99_998).contains(&s));
            assert!(seen.insert(s), "a seed repeated within an edit");
        }
    }
}
