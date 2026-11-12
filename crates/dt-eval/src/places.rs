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
    /// Places of `tile` on `frame`'s lattice, with every region's mask
    /// already made (`masks`, one per region of the tile, as
    /// [`Places::all_regions`] gave them): asking for a region is a copy.
    #[must_use]
    pub fn with_masks(tile: &'a Tile, frame: &'a Frame, masks: &[Mask]) -> Self {
        let places = Self::new(tile, frame);
        places.regions.borrow_mut().extend(
            tile.regions
                .iter()
                .zip(masks)
                .map(|(r, m)| (r.name.clone(), m.clone())),
        );
        places
    }

    /// Every region of the tile as a mask, made once: each region by its
    /// index, or every region that cannot be made with its reason.
    ///
    /// # Errors
    /// Each region that cannot be made (regions that name each other round a
    /// loop, a name that is no place), as (its index, why).
    pub fn all_regions(&self) -> Result<Vec<Mask>, Vec<(usize, String)>> {
        let mut out = Vec::new();
        let mut flaws = Vec::new();
        for (i, r) in self.tile.regions.iter().enumerate() {
            match self.region(r) {
                Ok(m) => out.push(m),
                Err(why) => flaws.push((i, why)),
            }
        }
        if flaws.is_empty() {
            Ok(out)
        } else {
            Err(flaws)
        }
    }

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

    /// The node nearest a point in squares.
    fn point(&self, xy: [f64; 2]) -> Mask {
        let (nx, ny) = self.dims();
        let mut m = Mask::new(nx, ny, false);
        let (i, j) = self.frame.node_at(xy);
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
            PlaceRef::Framed(f) => Err(format!(
                "the point `{f}` is in a region's frame, found once the tile's regions are known"
            )),
            PlaceRef::Name(n) => {
                if n == BUILTIN_REGION {
                    return Ok(Mask::new(nx, ny, true));
                }
                if let Some(xy) = self.tile.heights.points.get(n) {
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

    /// The depth a ranged `within` draws along `of`: masses of 6 to 15 mm
    /// (at the reference square) side by side, each as deep as the range
    /// draws it, the same for a given region and place whatever the seed;
    /// the end of each is the standard end section, the range's middle.
    fn drawn_depth(
        &self,
        region: &str,
        of: &PlaceRef,
        place: &Mask,
        range: (f64, f64),
        scale: f64,
    ) -> Drawn {
        let (nx, _) = self.dims();
        let step = self.frame.step_mm();
        let (mut i0, mut i1, mut j0, mut j1) = (usize::MAX, 0, usize::MAX, 0);
        for (k, &b) in place.bits.iter().enumerate() {
            if b {
                let (i, j) = (k % nx, k / nx);
                i0 = i0.min(i);
                i1 = i1.max(i);
                j0 = j0.min(j);
                j1 = j1.max(j);
            }
        }
        let along_x = i1.saturating_sub(i0) >= j1.saturating_sub(j0);
        let (lo, hi) = if along_x { (i0, i1) } else { (j0, j1) };
        let len = hi.saturating_sub(lo) as f64 * step;
        let key = dt_kit::Seed::new(0x00d7_a11e)
            .key(region)
            .key(&format!("{of:?}"));
        let mut rng = dt_kit::Rng::new(key);
        let mut edges = vec![0.0];
        while edges.last().copied().unwrap_or(0.0) < len {
            let l = edges.last().copied().unwrap_or(0.0);
            edges.push(l + rng.uniform(6.0 * scale, 15.0 * scale));
        }
        let n = edges.len();
        edges[n - 1] = len;
        if n > 2 && edges[n - 1] - edges[n - 2] < 3.0 * scale {
            edges.remove(n - 2);
        }
        let depths: Vec<f64> = (0..edges.len() - 1)
            .map(|_| rng.uniform(range.0, range.1))
            .collect();
        Drawn {
            nx,
            along_x,
            lo,
            step,
            len,
            end: (4.0 * scale, 0.5 * (range.0 + range.1)),
            edges,
            depths,
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
        if let Some(m) = self.regions.borrow().get(&r.name) {
            return Ok(m.clone());
        }
        let m = self.region_at(r, 0)?;
        self.regions.borrow_mut().insert(r.name.clone(), m.clone());
        Ok(m)
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
            let (dist, near) = distance_nearest(&of.bits, nx, ny);
            let step = self.frame.step_mm();
            if let Some((a, b)) = w.range {
                // the depth wanders between a and b along the place
                let reach = self.drawn_depth(&r.name, &w.of, &of, (a * scale, b * scale), scale);
                for k in 0..out.bits.len() {
                    if let Some(src) = near[k] {
                        let t = reach.along(src);
                        out.bits[k] |= dist[k] * step <= t + 1e-9;
                    }
                }
            } else {
                let reach = w.mm * scale / step;
                for (o, d) in out.bits.iter_mut().zip(&dist) {
                    *o |= *d <= reach + 1e-9;
                }
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

/// A depth drawn along a place ([`Places::drawn_depth`]).
struct Drawn {
    nx: usize,
    along_x: bool,
    lo: usize,
    step: f64,
    len: f64,
    /// The end section's length and depth, mm.
    end: (f64, f64),
    edges: Vec<f64>,
    depths: Vec<f64>,
}

impl Drawn {
    /// The depth, mm, at the place's node `src`.
    fn along(&self, src: usize) -> f64 {
        let at = if self.along_x {
            src % self.nx
        } else {
            src / self.nx
        };
        let t = at.saturating_sub(self.lo) as f64 * self.step;
        if t < self.end.0 || t > self.len - self.end.0 {
            return self.end.1;
        }
        let k = self.edges.partition_point(|&e| e <= t).saturating_sub(1);
        self.depths[k.min(self.depths.len() - 1)]
    }
}
