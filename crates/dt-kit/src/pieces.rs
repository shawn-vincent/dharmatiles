//! **Pieces**: what a pieces body (floor slabs and planks, later cliff
//! stones) needs to be made to fit its band.
//!
//! A field body is clamped point by point ([`crate::Band::fit`]) and cannot
//! miss its band. A pieces body is separate solids, each with a top of its
//! own, so each piece is *made to fit*: [`Pieces::plane`] draws a top plane
//! (a height and a tilt) from what the body wants and reduces it, at the
//! moment the piece is made, until the plane is inside the band over the
//! piece's whole footprint. Nothing is trimmed afterwards.
//! [`Pieces::admits`] then checks the finished mesh against the band: a miss
//! there is a bug in the body (`dt::band`).

use dt_geom::TriMesh;

use crate::{Band, Field, Frame, Refusal};

/// How far a finished piece may stand over its band before it is refused,
/// mm (float rounding only).
pub const BAND_TOL_MM: f64 = 0.005;

/// A top plane: the height at a point and the slope along each axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// The point the height is at, mm.
    pub at: [f64; 2],
    /// The height there, mm.
    pub z: f64,
    /// The rise per mm along x and y.
    pub g: [f64; 2],
}

impl Plane {
    /// The height at `p`.
    #[must_use]
    pub fn height(&self, p: [f64; 2]) -> f64 {
        self.z + self.g[0] * (p[0] - self.at[0]) + self.g[1] * (p[1] - self.at[1])
    }
}

/// A band, and the ways a piece is made to fit it.
#[derive(Clone, Copy)]
pub struct Pieces<'a> {
    band: &'a Band,
    frame: Frame,
    nx: usize,
    ny: usize,
    step: f64,
}

impl<'a> Pieces<'a> {
    /// The fitter for `band` on `frame`'s lattice.
    #[must_use]
    pub fn new(band: &'a Band, frame: &Frame) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            band,
            frame: *frame,
            nx,
            ny,
            step: frame.step_mm(),
        }
    }

    /// The nodes inside the box `lo..hi` (mm), grown by `grow` mm: the
    /// footprint of a piece.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn nodes(&self, lo: [f64; 2], hi: [f64; 2], grow: f64) -> Vec<usize> {
        let (is, js) = self.frame.node_range(lo, hi, grow);
        let mut out = Vec::new();
        for j in js {
            for i in is.clone() {
                out.push(j * self.nx + i);
            }
        }
        out
    }

    #[allow(clippy::cast_precision_loss)]
    fn pos(&self, k: usize) -> [f64; 2] {
        [
            (k % self.nx) as f64 * self.step,
            (k / self.nx) as f64 * self.step,
        ]
    }

    /// The heights a plane of this tilt may be at its `at`, over `nodes`:
    /// `(low, high)`, empty when low is above high.
    fn room(&self, nodes: &[usize], at: [f64; 2], g: [f64; 2]) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for &k in nodes {
            let p = self.pos(k);
            let rise = g[0] * (p[0] - at[0]) + g[1] * (p[1] - at[1]);
            let b = self.band.at(k);
            lo = lo.max(b.lo - rise);
            hi = hi.min(b.hi - rise);
        }
        (lo, hi)
    }

    /// **plane**: the plane nearest what a piece wants (`want`: its height
    /// at `want.at` and its tilt) that is inside the band at every one of
    /// `nodes`. The tilt is reduced as little as the band needs: whole,
    /// then along one axis only (a piece against a pinned edge keeps the
    /// tilt that leaves the edge's height alone), then none; the height is
    /// held to what the tilt leaves room for.
    ///
    /// # Errors
    /// [`Refusal::Band`] when no plane, not even a flat one, fits: the band
    /// jumps across the piece. The refusal names the worst node.
    pub fn plane(&self, nodes: &[usize], want: Plane) -> Result<Plane, Refusal> {
        let at = want.at;
        let tol = 1e-9;
        let feasible = |g: [f64; 2]| {
            let (lo, hi) = self.room(nodes, at, g);
            lo <= hi + tol
        };
        let mut best: Option<([f64; 2], f64)> = None;
        for dir in [want.g, [want.g[0], 0.0], [0.0, want.g[1]], [0.0, 0.0]] {
            // the largest share of the tilt that fits (the fitting shares
            // are an interval from 0, the set of fitting tilts being convex)
            let f = if feasible(dir) {
                1.0
            } else if feasible([0.0, 0.0]) {
                let (mut ok, mut bad) = (0.0, 1.0);
                for _ in 0..24 {
                    let mid = 0.5 * (ok + bad);
                    if feasible([dir[0] * mid, dir[1] * mid]) {
                        ok = mid;
                    } else {
                        bad = mid;
                    }
                }
                ok
            } else {
                0.0
            };
            let g = [dir[0] * f, dir[1] * f];
            let size = g[0].hypot(g[1]);
            if best.is_none_or(|(_, s)| size > s + 1e-12) {
                best = Some((g, size));
            }
        }
        let g = best.map_or([0.0, 0.0], |(g, _)| g);
        let (lo, hi) = self.room(nodes, at, g);
        if lo > hi + tol {
            // report the node the band is worst for a flat plane
            let mut worst = (nodes.first().copied().unwrap_or(0), 0.0_f64);
            let mid = want.z;
            for &k in nodes {
                let b = self.band.at(k);
                let by = (b.lo - mid).max(mid - b.hi);
                if by > worst.1 {
                    worst = (k, by);
                }
            }
            let b = self.band.at(worst.0);
            return Err(Refusal::Band {
                at: self.pos(worst.0),
                value: mid,
                lo: b.lo,
                hi: b.hi,
                by: worst.1.max(lo - hi),
            });
        }
        Ok(Plane {
            at,
            z: want.z.clamp(lo, hi.max(lo)),
            g,
        })
    }

    /// **plane_or_flat**: [`Pieces::plane`], and where the band jumps across
    /// the piece (no plane fits), a flat top at the lowest of the band's
    /// highs over `nodes`: the piece stands over its lower side and stops
    /// short of the higher. A pieces body that cannot be set to a band's
    /// step (a cliff's stone) takes this; a floor refuses instead.  The flag
    /// is true where the flat top was taken, so the caller can say so (the
    /// build reports it with what an element left out).
    #[must_use]
    pub fn plane_or_flat(&self, nodes: &[usize], want: Plane) -> (Plane, bool) {
        match self.plane(nodes, want) {
            Ok(plane) => (plane, false),
            Err(_) => (
                Plane {
                    at: want.at,
                    z: nodes
                        .iter()
                        .map(|&k| self.band.at(k).hi)
                        .fold(f64::INFINITY, f64::min),
                    g: [0.0, 0.0],
                },
                true,
            ),
        }
    }

    /// Whether `plane` is under the band's high at every one of `nodes`.
    #[must_use]
    pub fn under(&self, nodes: &[usize], plane: Plane) -> bool {
        nodes
            .iter()
            .all(|&k| plane.height(self.pos(k)) <= self.band.at(k).hi + 1e-9)
    }

    /// Whether `plane` reaches the band's low at any of `nodes`.
    #[must_use]
    pub fn reaches(&self, nodes: &[usize], plane: Plane) -> bool {
        nodes
            .iter()
            .any(|&k| plane.height(self.pos(k)) >= self.band.at(k).lo - 1e-9)
    }

    /// The band's high at a point, bilinear between nodes.
    fn high_at(&self, p: [f64; 2]) -> f64 {
        let hi: &Field = self.band.hi();
        hi.sample(p[0], p[1])
    }

    /// **admits**: whether a finished piece stays under the band's high
    /// everywhere inside the tile (`dt::band`: a miss is a bug in the body).
    /// What lies past the tile is cut away and not looked at.
    ///
    /// # Errors
    /// [`Refusal::Band`] at the vertex farthest over the band.
    pub fn admits(&self, mesh: &TriMesh) -> Result<(), Refusal> {
        self.admits_within(mesh, BAND_TOL_MM)
    }

    /// As [`Pieces::admits`], with `tol_mm` of room over the high: a draft
    /// carves on coarser grids than a print, and is never written.
    ///
    /// # Errors
    /// [`Refusal::Band`] at the vertex farthest over the band.
    #[allow(clippy::cast_precision_loss)]
    pub fn admits_within(&self, mesh: &TriMesh, tol_mm: f64) -> Result<(), Refusal> {
        let w = (self.nx - 1) as f64 * self.step;
        let h = (self.ny - 1) as f64 * self.step;
        let mut worst: Option<([f64; 3], f64)> = None;
        for p in &mesh.positions {
            if p[0] < 0.0 || p[1] < 0.0 || p[0] > w || p[1] > h {
                continue;
            }
            let by = p[2] - self.high_at([p[0], p[1]]);
            if by > tol_mm && worst.is_none_or(|(_, b)| by > b) {
                worst = Some((*p, by));
            }
        }
        worst.map_or(Ok(()), |(p, by)| {
            let hi = self.high_at([p[0], p[1]]);
            Err(Refusal::Band {
                at: [p[0], p[1]],
                value: p[2],
                lo: hi,
                hi,
                by,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn frame() -> Frame {
        Frame::for_system("db", 1, 1)
    }

    fn band(f: impl Fn(f64, f64) -> (f64, f64)) -> (Band, Frame) {
        let fr = frame();
        let (nx, ny) = fr.nodes();
        let b = Band::from_fn(nx, ny, fr.step_mm(), |x, y| {
            let (lo, hi) = f(x, y);
            dt_geom::Iv { lo, hi }
        });
        (b, fr)
    }

    #[test]
    fn a_flat_band_holds_a_piece_flat() {
        let (b, fr) = band(|_, _| (7.4, 7.4));
        let p = Pieces::new(&b, &fr);
        let nodes = p.nodes([2.0, 2.0], [12.0, 12.0], 0.0);
        let want = Plane {
            at: [7.0, 7.0],
            z: 9.0,
            g: [0.05, 0.02],
        };
        let got = p.plane(&nodes, want).unwrap();
        assert_eq!(got.g, [0.0, 0.0]);
        assert!((got.z - 7.4).abs() < 1e-9);
    }

    #[test]
    fn a_wide_band_lets_a_piece_tilt_and_a_pinned_edge_keeps_its_height() {
        // wide over the tile, pinned to 7.4 along the south edge
        let (b, fr) = band(|_, y| if y < 0.01 { (7.4, 7.4) } else { (5.0, 9.0) });
        let p = Pieces::new(&b, &fr);
        let nodes = p.nodes([2.0, 0.0], [12.0, 10.0], 0.0);
        let want = Plane {
            at: [7.0, 5.0],
            z: 7.4,
            g: [0.03, 0.03],
        };
        let got = p.plane(&nodes, want).unwrap();
        // the tilt along the pinned edge goes; the tilt away from it stays
        assert_eq!(got.g[0], 0.0);
        assert!(got.g[1] > 0.0);
        for &k in &nodes {
            let z = got.height(p.pos(k));
            let iv = b.at(k);
            assert!(z >= iv.lo - 1e-9 && z <= iv.hi + 1e-9, "{z}");
        }
    }

    #[test]
    fn a_band_that_jumps_across_a_piece_is_a_miss() {
        let (b, fr) = band(|x, _| if x < 7.0 { (3.0, 3.0) } else { (8.0, 8.0) });
        let p = Pieces::new(&b, &fr);
        let nodes = p.nodes([2.0, 2.0], [12.0, 12.0], 0.0);
        let want = Plane {
            at: [7.0, 7.0],
            z: 5.0,
            g: [0.0, 0.0],
        };
        let e = p.plane(&nodes, want).unwrap_err();
        assert_eq!(e.code(), dt_lang::codes::BAND);
    }

    #[test]
    fn a_mesh_over_the_band_is_refused() {
        let (b, fr) = band(|_, _| (7.4, 7.4));
        let p = Pieces::new(&b, &fr);
        let tri = |z: f64| TriMesh {
            positions: vec![[1.0, 1.0, z], [2.0, 1.0, z], [1.0, 2.0, z]],
            triangles: vec![[0, 1, 2]],
            ..TriMesh::default()
        };
        assert!(p.admits(&tri(7.4)).is_ok());
        assert!(p.admits(&tri(7.5)).is_err());
    }
}
