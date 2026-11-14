//! The seam table: the places on a tile's outline where a line may cross it.
//!
//! The edge contract (`library/parts/edges.dtile`) lists the positions, as
//! shares of a square's side measured from its own corner: from the west on a
//! north or south side, from the north on a west or east side, and the word
//! each position has on each side. Every square edge on the tile's outline has
//! the same seams, so two tiles that meet at a seam meet at one place. A tile of
//! several squares has them on each of its squares. The tile's own four
//! corners are seams too, with words of their own; the point partway along a
//! side where two squares meet is not one.
//!
//! This is the one table: the check (`dt::seam`), its snap fix, the words a
//! document may write for a seam and the table on the tile-system page all read
//! it. The program keeps no word list of its own.

use crate::place::{Corner, Edge};

/// How near a position must be to a seam to be on it (squares).
pub const ON_SEAM: f64 = 1e-6;

/// A point this near a side (squares) without being on it is a slip: it is
/// 1 mm at the 35 mm square.
pub const NEAR_EDGE: f64 = 1.0 / 35.0;

/// The sides, in the order a seam keeps its words.
pub const SIDES: [Edge; 4] = [Edge::North, Edge::South, Edge::West, Edge::East];

/// One seam of a square's edge.
#[derive(Debug, Clone, PartialEq)]
pub struct Seam {
    /// Where it is on the side, 0 to 1 (exclusive).
    pub at: f64,
    /// Its word on the north, south, west and east sides.
    pub words: [String; 4],
}

/// The seams of a square's edge, and the words of the tile's corners.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeamTable {
    /// The seams between the corners, in order along the side.
    pub seams: Vec<Seam>,
    /// The word of each tile corner.
    pub corners: Vec<(Corner, String)>,
}

/// A place on a tile's outline, named by its square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    /// The square's column, counted from the west from 1.
    pub col: u32,
    /// The square's row, counted from the north from 1.
    pub row: u32,
    /// The side of the tile.
    pub side: Edge,
    /// Where on the square's edge, 0 to 1.
    pub at: f64,
}

/// What is wrong with a point on a side.
#[derive(Debug, Clone, PartialEq)]
pub struct OffSeam {
    /// Where it is.
    pub place: Place,
    /// The nearest seam of that square's edge.
    pub nearest: f64,
}

/// A seam word, read.
#[derive(Debug, Clone, PartialEq)]
pub enum Word {
    /// A corner of the tile.
    Corner(Corner),
    /// A place on the tile's outline, in squares from the south-west corner.
    At([f64; 2]),
}

/// Why a word names no place, with what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct WordError {
    /// What is wrong.
    pub message: String,
    /// What to do, when there is more to say than the message.
    pub help: Option<String>,
    /// A near word to offer as a fix.
    pub near: Option<String>,
}

fn side_index(side: Edge) -> usize {
    SIDES.iter().position(|s| *s == side).unwrap_or(0)
}

/// How many squares long a side of a `cols` x `rows` tile is.
#[must_use]
pub fn squares_along(side: Edge, cols: u32, rows: u32) -> u32 {
    match side {
        Edge::North | Edge::South => cols,
        Edge::West | Edge::East => rows,
    }
}

impl SeamTable {
    /// The positions of the seams between the corners.
    #[must_use]
    pub fn positions(&self) -> Vec<f64> {
        self.seams.iter().map(|s| s.at).collect()
    }

    /// The table is closed under the quarter turn, which maps a position t to
    /// 1 - t on the side it lands on: every seam has its mirror. `Err` names one
    /// that has not.
    ///
    /// # Errors
    /// The first position whose mirror is missing.
    pub fn closed(&self) -> Result<(), f64> {
        for s in &self.seams {
            if !self
                .seams
                .iter()
                .any(|m| (m.at - (1.0 - s.at)).abs() < ON_SEAM)
            {
                return Err(s.at);
            }
        }
        Ok(())
    }

    /// Every word the table has, for a near-word suggestion.
    #[must_use]
    pub fn words(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.corners.iter().map(|c| c.1.as_str()).collect();
        for s in &self.seams {
            out.extend(s.words.iter().map(String::as_str));
        }
        out
    }

    /// The seam nearest `t`.
    #[must_use]
    pub fn nearest(&self, t: f64) -> f64 {
        self.seams
            .iter()
            .map(|s| s.at)
            .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
            .unwrap_or(0.5)
    }

    /// The place on a `cols` x `rows` tile's outline that a point (squares,
    /// y north from the south-west corner) is at: `None` when it is not on a
    /// side. A point on a corner of the tile is on two sides: the first is
    /// north or south.
    #[must_use]
    pub fn place_of(p: [f64; 2], cols: u32, rows: u32) -> Option<Place> {
        let (w, h) = (f64::from(cols), f64::from(rows));
        let on = |v: f64, edge: f64| (v - edge).abs() < ON_SEAM;
        let side = if on(p[1], h) {
            Edge::North
        } else if on(p[1], 0.0) {
            Edge::South
        } else if on(p[0], 0.0) {
            Edge::West
        } else if on(p[0], w) {
            Edge::East
        } else {
            return None;
        };
        // along the side, in squares from its start (west, or north)
        let along = match side {
            Edge::North | Edge::South => p[0],
            Edge::West | Edge::East => h - p[1],
        };
        let n = squares_along(side, cols, rows);
        // the square the point belongs to: the one it starts, unless it ends the side
        let mut k = (along + ON_SEAM).floor().max(0.0);
        if k >= f64::from(n) {
            k = f64::from(n) - 1.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = k as u32;
        let at = along - f64::from(k);
        let (col, row) = match side {
            Edge::North => (k + 1, 1),
            Edge::South => (k + 1, rows),
            Edge::West => (1, k + 1),
            Edge::East => (cols, k + 1),
        };
        Some(Place { col, row, side, at })
    }

    /// `Some` when the point is on a side but not on a seam: not one of its
    /// square's seams, and not a corner of the tile (the point partway along a
    /// side where two squares meet is not a seam).
    #[must_use]
    pub fn off_seam(&self, p: [f64; 2], cols: u32, rows: u32) -> Option<OffSeam> {
        let (w, h) = (f64::from(cols), f64::from(rows));
        let on = |v: f64, edge: f64| (v - edge).abs() < ON_SEAM;
        if (on(p[0], 0.0) || on(p[0], w)) && (on(p[1], 0.0) || on(p[1], h)) {
            return None;
        }
        let place = Self::place_of(p, cols, rows)?;
        let at = self.nearest(place.at);
        ((at - place.at).abs() >= ON_SEAM).then_some(OffSeam { place, nearest: at })
    }

    /// The point (squares, y north) at position `at` of the square `place`
    /// names, on its side.
    #[must_use]
    pub fn point_at(place: &Place, at: f64, cols: u32, rows: u32) -> [f64; 2] {
        let (w, h) = (f64::from(cols), f64::from(rows));
        let (x, y_down) = match place.side {
            Edge::North => (f64::from(place.col - 1) + at, 0.0),
            Edge::South => (f64::from(place.col - 1) + at, h),
            Edge::West => (0.0, f64::from(place.row - 1) + at),
            Edge::East => (w, f64::from(place.row - 1) + at),
        };
        [x, h - y_down]
    }

    /// The word for position `at` on the square `place` names, as a document
    /// writes it: `nnw` on a one-square side, `nnw(2)` on a longer one.
    #[must_use]
    pub fn word_for(&self, place: &Place, at: f64, cols: u32, rows: u32) -> Option<String> {
        let seam = self.seams.iter().find(|s| (s.at - at).abs() < ON_SEAM)?;
        let word = &seam.words[side_index(place.side)];
        if squares_along(place.side, cols, rows) <= 1 {
            return Some(word.clone());
        }
        let k = match place.side {
            Edge::North | Edge::South => place.col,
            Edge::West | Edge::East => place.row,
        };
        Some(format!("{word}({k})"))
    }

    /// The seam words of a side on a tile, each with its square number when
    /// the side is longer than one square: what a bare side word could mean.
    fn numbered(&self, word: &str, side: Edge, cols: u32, rows: u32) -> Vec<String> {
        (1..=squares_along(side, cols, rows))
            .map(|k| format!("\"{word}({k})\""))
            .collect()
    }

    /// Read a word written for a point on a `cols` x `rows` tile. `None` when
    /// the text is not shaped like a word (`nnw`, `nnw(2)`) or names nothing
    /// and nothing near it; `Some(Err)` when it is a word with a mistake in it.
    #[must_use]
    pub fn read(&self, text: &str, cols: u32, rows: u32) -> Option<Result<Word, WordError>> {
        let t = text.trim();
        let (name, number) = match t.split_once('(') {
            Some((name, rest)) => {
                let digits = rest.strip_suffix(')')?;
                (name.trim(), Some(digits.trim().parse::<u32>().ok()?))
            }
            None => (t, None),
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase()) {
            return None;
        }
        if let Some((corner, _)) = self.corners.iter().find(|c| c.1 == name) {
            return Some(match number {
                None => Ok(Word::Corner(*corner)),
                Some(_) => Err(WordError {
                    message: format!("`{name}` is a corner of the tile and takes no number"),
                    help: Some(format!("write `{name}`")),
                    near: None,
                }),
            });
        }
        let found = self.seams.iter().find_map(|s| {
            SIDES
                .iter()
                .zip(&s.words)
                .find(|(_, w)| w.as_str() == name)
                .map(|(side, _)| (*side, s.at))
        });
        let Some((side, at)) = found else {
            let near = crate::schema::did_you_mean(name, self.words())?;
            return Some(Err(WordError {
                message: format!("`{name}` is not a place on a tile's side"),
                help: None,
                near: Some(near.to_owned()),
            }));
        };
        let n = squares_along(side, cols, rows);
        let k = match (number, n) {
            (None, 1) | (Some(1), 1) => 1,
            (None, _) => {
                return Some(Err(WordError {
                    message: format!(
                        "this side is {n} squares long: say which square `{name}` is on"
                    ),
                    help: Some(format!(
                        "write one of {}",
                        self.numbered(name, side, cols, rows).join(", ")
                    )),
                    near: None,
                }));
            }
            (Some(k), _) if (1..=n).contains(&k) => k,
            (Some(k), _) => {
                return Some(Err(WordError {
                    message: format!(
                        "`{name}({k})` is not a square of this side, which is {n} long"
                    ),
                    help: Some(if n == 1 {
                        format!("this side is one square long: write `{name}`")
                    } else {
                        format!(
                            "write one of {}",
                            self.numbered(name, side, cols, rows).join(", ")
                        )
                    }),
                    near: None,
                }));
            }
        };
        let (col, row) = match side {
            Edge::North => (k, 1),
            Edge::South => (k, rows),
            Edge::West => (1, k),
            Edge::East => (cols, k),
        };
        let place = Place { col, row, side, at };
        Some(Ok(Word::At(Self::point_at(&place, at, cols, rows))))
    }
}

/// A square's place as a person says it: `column 2, row 1, north side`.
#[must_use]
pub fn say(p: &Place) -> String {
    format!("column {}, row {}, {} side", p.col, p.row, p.side.as_str())
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn table() -> SeamTable {
        let seam = |at, w: [&str; 4]| Seam {
            at,
            words: w.map(str::to_owned),
        };
        SeamTable {
            seams: vec![
                seam(1.0 / 3.0, ["nnw", "ssw", "wnw", "ene"]),
                seam(0.5, ["n", "s", "w", "e"]),
                seam(2.0 / 3.0, ["nne", "sse", "wsw", "ese"]),
            ],
            corners: vec![
                (Corner::NorthWest, "nw".into()),
                (Corner::NorthEast, "ne".into()),
                (Corner::SouthEast, "se".into()),
                (Corner::SouthWest, "sw".into()),
            ],
        }
    }

    #[test]
    fn closed_under_turning() {
        assert!(table().closed().is_ok());
        let mut t = table();
        t.seams.remove(2);
        assert!((t.closed().unwrap_err() - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_point_is_at_a_square_of_its_side() {
        let p = SeamTable::place_of([1.5, 2.0], 2, 2).unwrap();
        assert_eq!((p.col, p.row, p.side), (2, 1, Edge::North));
        assert!((p.at - 0.5).abs() < 1e-9);
        let p = SeamTable::place_of([0.0, 0.5], 2, 2).unwrap();
        assert_eq!((p.col, p.row, p.side), (1, 2, Edge::West));
        assert!(SeamTable::place_of([0.5, 0.5], 1, 1).is_none());
        let p = SeamTable::place_of([2.0, 0.0], 2, 1).unwrap();
        assert_eq!((p.col, p.row), (2, 1));
    }

    #[test]
    fn off_seams_are_found_and_snapped_on_their_own_square() {
        let t = table();
        assert!(t.off_seam([1.0 + 2.0 / 3.0, 2.0], 2, 2).is_none());
        // a tile corner is a seam; the point where two squares meet is not
        assert!(t.off_seam([0.0, 0.0], 1, 1).is_none());
        assert!(t.off_seam([2.0, 0.0], 2, 1).is_none());
        let o = t.off_seam([1.0, 2.0], 3, 2).unwrap();
        assert_eq!((o.place.col, o.place.side), (2, Edge::North));
        let o = t.off_seam([0.45, 1.0], 1, 1).unwrap();
        assert!((o.nearest - 0.5).abs() < 1e-12);
        assert_eq!(say(&o.place), "column 1, row 1, north side");
        let o = t.off_seam([1.45, 2.0], 2, 2).unwrap();
        let p = SeamTable::point_at(&o.place, o.nearest, 2, 2);
        assert!((p[0] - 1.5).abs() < 1e-12 && p[1] == 2.0);
        assert_eq!(
            t.word_for(&o.place, o.nearest, 2, 2).as_deref(),
            Some("n(2)")
        );
    }

    #[test]
    fn words_make_points() {
        let t = table();
        let at = |w: &str, c, r| match t.read(w, c, r).unwrap().unwrap() {
            Word::At(p) => p,
            Word::Corner(_) => panic!("{w}"),
        };
        let p = at("nnw(2)", 2, 2);
        assert!((p[0] - (1.0 + 1.0 / 3.0)).abs() < 1e-12 && p[1] == 2.0);
        let p = at("e", 1, 1);
        assert!(p[0] == 1.0 && (p[1] - 0.5).abs() < 1e-12);
        assert_eq!(at("e(1)", 1, 1), at("e", 1, 1));
        let p = at("wsw(2)", 3, 2);
        assert!(p[0] == 0.0 && (p[1] - (2.0 - 1.0 - 2.0 / 3.0)).abs() < 1e-12);
        assert_eq!(
            t.read("sw", 3, 3).unwrap().unwrap(),
            Word::Corner(Corner::SouthWest)
        );
        // a bare side word on a longer side, a number out of range, a number on a corner
        let e = t.read("n", 2, 1).unwrap().unwrap_err();
        assert!(e.help.unwrap().contains("\"n(1)\", \"n(2)\""));
        assert!(t.read("n(3)", 2, 1).unwrap().is_err());
        assert!(t.read("nw(1)", 2, 1).unwrap().is_err());
        // an unknown word with a near one is a suggestion; a word of its own is not ours
        assert_eq!(
            t.read("nnww", 1, 1).unwrap().unwrap_err().near.as_deref(),
            Some("nnw")
        );
        assert!(t.read("knoll", 1, 1).is_none());
        assert!(t.read("(0sq, 0sq)", 1, 1).is_none());
    }
}
