//! Places as masks on a lattice: the built-in points, edges and `tile`, the
//! tile's own points, lines and regions, and a region's `within` and
//! `except`.

use std::cell::RefCell;
use std::collections::HashMap;

use dt_doc::model::PlaceRef;
use dt_doc::place::{BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION, builtin_line, builtin_point};
use dt_geom::draw_line;
use dt_kit::{Frame, Mask, distance_nearest};

use crate::doc::{Region, Tile};

/// The deepest chain of regions naming each other.
const DEPTH: usize = 16;

/// Resolves places on `frame`'s lattice for one tile.
pub struct Places<'a> {
    tile: &'a Tile,
    frame: &'a Frame,
    regions: RefCell<HashMap<String, Mask>>,
}

impl<'a> Places<'a> {
    /// Places of `tile` on `frame`'s lattice.
    #[must_use]
    pub fn new(tile: &'a Tile, frame: &'a Frame) -> Self {
        Self {
            tile,
            frame,
            regions: RefCell::new(HashMap::new()),
        }
    }

    fn dims(&self) -> (usize, usize) {
        self.frame.nodes()
    }

    fn per(&self) -> f64 {
        f64::from(self.frame.cells_per_square)
    }

    fn node(&self, v: f64, n: usize) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (v * self.per()).round().clamp(0.0, (n - 1) as f64) as usize;
        k
    }

    /// The node nearest a point in squares.
    fn point(&self, xy: [f64; 2]) -> Mask {
        let (nx, ny) = self.dims();
        let mut m = Mask::new(nx, ny, false);
        let (i, j) = (self.node(xy[0], nx), self.node(xy[1], ny));
        m.bits[j * nx + i] = true;
        m
    }

    fn line(&self, pts: &[[f64; 2]]) -> Mask {
        let (nx, ny) = self.dims();
        let per = self.per();
        let mut m = Mask::new(nx, ny, false);
        for w in pts.windows(2) {
            draw_line(
                &mut m,
                [w[0][0] * per, w[0][1] * per],
                [w[1][0] * per, w[1][1] * per],
            );
        }
        m
    }

    /// The nodes of a place.
    ///
    /// # Errors
    /// A name nothing declares, or regions that name each other round a loop.
    pub fn mask(&self, p: &PlaceRef) -> Result<Mask, String> {
        self.mask_at(p, 0)
    }

    fn mask_at(&self, p: &PlaceRef, depth: usize) -> Result<Mask, String> {
        let (nx, ny) = self.dims();
        let (cols, rows) = (f64::from(self.tile.cols), f64::from(self.tile.rows));
        match p {
            PlaceRef::Point(x, y) => Ok(self.point([*x, *y])),
            PlaceRef::Name(n) => {
                if n == BUILTIN_REGION {
                    return Ok(Mask::new(nx, ny, true));
                }
                if let Some(xy) = self.tile.heights.as_ref().and_then(|h| h.points.get(n)) {
                    return Ok(self.point(*xy));
                }
                if BUILTIN_POINTS.contains(&n.as_str())
                    && let Some((x, y)) = builtin_point(n, cols, rows)
                {
                    return Ok(self.point([x, y]));
                }
                if BUILTIN_LINES.contains(&n.as_str())
                    && let Some(pts) = builtin_line(n, cols, rows)
                {
                    return Ok(self.line(&pts));
                }
                if let Some(pts) = self.tile.lines.get(n) {
                    return Ok(self.line(pts));
                }
                if let Some(i) = self.tile.regions.iter().position(|r| &r.name == n) {
                    return self.region_named(&self.tile.regions[i], depth);
                }
                Err(format!("no place is called `{n}`"))
            }
        }
    }

    fn region_named(&self, r: &Region, depth: usize) -> Result<Mask, String> {
        if let Some(m) = self.regions.borrow().get(&r.name) {
            return Ok(m.clone());
        }
        let m = self.region_at(r, depth + 1)?;
        self.regions.borrow_mut().insert(r.name.clone(), m.clone());
        Ok(m)
    }

    /// A region's nodes: its bucket fills and its discs, less what it excepts;
    /// a region that only excepts starts from the whole tile.
    ///
    /// # Errors
    /// As [`Places::mask`].
    pub fn region(&self, r: &Region) -> Result<Mask, String> {
        self.region_at(r, 0)
    }

    fn region_at(&self, r: &Region, depth: usize) -> Result<Mask, String> {
        if depth > DEPTH {
            return Err(format!(
                "the region `{}` names itself through others",
                r.name
            ));
        }
        let (nx, ny) = self.dims();
        let has_bucket = !r.bounded_by.is_empty() || !r.fills.is_empty();
        let mut out = if has_bucket {
            let lines: Vec<Vec<[f64; 2]>> = r
                .bounded_by
                .iter()
                .filter_map(|l| self.tile.lines.get(&l.value).cloned())
                .collect();
            dt_kit::area(self.frame, &lines, &r.fills)
        } else {
            Mask::new(nx, ny, r.within.is_empty() && !r.except.is_empty())
        };
        let scale = self.frame.square_mm / dt_kit::REFERENCE_SQUARE_MM;
        for w in &r.within {
            let of = self.mask_at(&w.of, depth + 1)?;
            let (dist, _) = distance_nearest(&of.bits, nx, ny);
            let reach = w.mm * scale / self.frame.step_mm();
            for (o, d) in out.bits.iter_mut().zip(&dist) {
                *o |= *d <= reach + 1e-9;
            }
        }
        for e in &r.except {
            let gone = self.mask_at(&PlaceRef::Name(e.clone()), depth + 1)?;
            for (o, g) in out.bits.iter_mut().zip(&gone.bits) {
                *o &= !*g;
            }
        }
        Ok(out)
    }
}
