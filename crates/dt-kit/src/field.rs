//! Fields: values over the tile on the one grid convention, and the field
//! operations looks are written in (noise, distance, morphology with the
//! smooth maximum, scatter, the half-space cap).
//!
//! The grid convention (the only one; it replaces the Python library's
//! three, core/mesh.py:69, layers/water.py:542 and core/grid.py): a field
//! has a value at every node of the frame's lattice, node `(i, j)` at
//! `(i·step, j·step)` mm from the tile's south-west corner, so the edge
//! nodes lie exactly on the tile edge; values are row-major, `j` outer;
//! between nodes a field is bilinear ([`Field::sample`]).
//!
//! Sizes: a field works in world mm (the built system's). Layout-scale
//! things (hills, patches, clusters) are keyed at reference positions
//! ([`Scale::Layout`]), so a 25.4 mm build is a photocopy of the 35 mm one;
//! print-scale details (grit, clods, pebbles) keep their true mm
//! ([`Scale::Print`], default 1).

use dt_geom::{Footprint, Rng, Seed, SpatialHash};
use rayon::prelude::*;

use crate::Frame;

/// Values at every node of a frame's lattice, in world mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// Nodes across.
    pub nx: usize,
    /// Nodes up.
    pub ny: usize,
    /// Node spacing, mm.
    pub step: f64,
    /// `nx * ny` values, row-major (`j` outer).
    pub z: Vec<f64>,
}

impl Field {
    /// A field of `value` on the frame's lattice.
    #[must_use]
    pub fn new(frame: &Frame, value: f64) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            nx,
            ny,
            step: frame.step_mm(),
            z: vec![value; nx * ny],
        }
    }

    /// A field from a function of position (mm).
    #[must_use]
    pub fn from_fn(frame: &Frame, f: impl Fn(f64, f64) -> f64 + Sync) -> Self {
        let mut out = Self::new(frame, 0.0);
        let (nx, step) = (out.nx, out.step);
        out.z.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
            #[allow(clippy::cast_precision_loss)]
            let y = j as f64 * step;
            for (i, o) in row.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let x = i as f64 * step;
                *o = f(x, y);
            }
        });
        out
    }

    /// The same lattice with every value `f` of this one's.
    #[must_use]
    pub fn map(&self, f: impl Fn(f64) -> f64) -> Self {
        Self {
            z: self.z.iter().map(|&v| f(v)).collect(),
            ..*self
        }
    }

    /// Combine with another field on the same lattice, node by node.
    #[must_use]
    pub fn zip(&self, other: &Self, f: impl Fn(f64, f64) -> f64) -> Self {
        Self {
            z: self
                .z
                .iter()
                .zip(&other.z)
                .map(|(&a, &b)| f(a, b))
                .collect(),
            ..*self
        }
    }

    /// A field on the same lattice with values `z`.
    #[must_use]
    pub fn with_z(&self, z: Vec<f64>) -> Self {
        debug_assert_eq!(z.len(), self.z.len());
        Self {
            nx: self.nx,
            ny: self.ny,
            step: self.step,
            z,
        }
    }

    /// The x of column `i`, mm.
    #[must_use]
    pub fn x(&self, i: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64 * self.step;
        x
    }

    /// The y of row `j`, mm.
    #[must_use]
    pub fn y(&self, j: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let y = j as f64 * self.step;
        y
    }

    /// The value at node `(i, j)`.
    #[must_use]
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.z[j * self.nx + i]
    }

    /// The node at or below `(x, y)` mm, clamped to the lattice.
    #[must_use]
    pub fn node(&self, x: f64, y: f64) -> (usize, usize) {
        let k = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = libm::floor(v / self.step).max(0.0) as usize;
            k.min(n - 1)
        };
        (k(x, self.nx), k(y, self.ny))
    }

    /// The bilinear value at `(x, y)` mm (clamped to the tile).
    #[must_use]
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let (mx, my) = ((self.nx - 1) as f64, (self.ny - 1) as f64);
        let (u, v) = (
            (x / self.step).clamp(0.0, mx),
            (y / self.step).clamp(0.0, my),
        );
        let (fu, fv) = (libm::floor(u), libm::floor(v));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (fu as usize, fv as usize);
        let (i1, j1) = ((i + 1).min(self.nx - 1), (j + 1).min(self.ny - 1));
        let (tu, tv) = (u - fu, v - fv);
        let a = self.at(i, j) * (1.0 - tu) + self.at(i1, j) * tu;
        let b = self.at(i, j1) * (1.0 - tu) + self.at(i1, j1) * tu;
        a * (1.0 - tv) + b * tv
    }

    /// The nodes within `r` mm of `(x, y)`, as index ranges `(i0..i1, j0..j1)`.
    #[must_use]
    pub fn window(
        &self,
        x: f64,
        y: f64,
        r: f64,
    ) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let lo = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = libm::ceil((v - r) / self.step).max(0.0) as usize;
            k.min(n)
        };
        let hi = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = (libm::floor((v + r) / self.step) + 1.0).max(0.0) as usize;
            k.min(n)
        };
        (
            lo(x, self.nx)..hi(x, self.nx),
            lo(y, self.ny)..hi(y, self.ny),
        )
    }

    // ---- morphology ----------------------------------------------------

    /// **morphology**: Gaussian blur of `sigma` mm (scipy's
    /// `gaussian_filter`, reflected ends; recursive past 32 nodes).
    #[must_use]
    pub fn blur(&self, sigma_mm: f64) -> Self {
        let s = sigma_mm / self.step;
        let z = if s > 32.0 {
            dt_geom::blur_recursive(&self.z, self.nx, self.ny, s)
        } else {
            dt_geom::blur(&self.z, self.nx, self.ny, s)
        };
        Self { z, ..*self }
    }

    /// **morphology**: grey dilation over a disc of `r` mm.
    #[must_use]
    pub fn dilate(&self, r_mm: f64) -> Self {
        Self {
            z: dt_geom::dilate(&self.z, self.nx, self.ny, Footprint::Disc(r_mm / self.step)),
            ..*self
        }
    }

    /// **morphology**: grey erosion over a disc of `r` mm.
    #[must_use]
    pub fn erode(&self, r_mm: f64) -> Self {
        Self {
            z: dt_geom::erode(&self.z, self.nx, self.ny, Footprint::Disc(r_mm / self.step)),
            ..*self
        }
    }

    /// **morphology**: the relief range (max − min) over the square window
    /// of side `w` mm round each node: the printable-texture measure.
    #[must_use]
    pub fn range(&self, w_mm: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r = libm::round(0.5 * w_mm / self.step) as usize;
        let hi = dt_geom::dilate(&self.z, self.nx, self.ny, Footprint::Square(r));
        let lo = dt_geom::erode(&self.z, self.nx, self.ny, Footprint::Square(r));
        Self {
            z: hi.iter().zip(&lo).map(|(a, b)| a - b).collect(),
            ..*self
        }
    }

    /// **morphology**: numpy's linear quantile of the values where `keep`
    /// holds (all values when `keep` is `None`).
    #[must_use]
    pub fn quantile(&self, q: f64, keep: Option<&[bool]>) -> f64 {
        match keep {
            None => dt_geom::quantile(&self.z, q),
            Some(k) => {
                let v: Vec<f64> = self
                    .z
                    .iter()
                    .zip(k)
                    .filter(|(_, k)| **k)
                    .map(|(v, _)| *v)
                    .collect();
                dt_geom::quantile(&v, q)
            }
        }
    }

    /// **morphology**: the steepest slope (rise per mm), with numpy's
    /// `gradient` (central differences inside, one-sided at the ends).
    #[must_use]
    pub fn max_slope(&self) -> f64 {
        let (nx, ny, h) = (self.nx, self.ny, self.step);
        let d = |a: f64, b: f64, n: f64| (a - b) / (n * h);
        let mut m: f64 = 0.0;
        for j in 0..ny {
            for i in 0..nx {
                let gx = if nx < 2 {
                    0.0
                } else if i == 0 {
                    d(self.at(1, j), self.at(0, j), 1.0)
                } else if i == nx - 1 {
                    d(self.at(i, j), self.at(i - 1, j), 1.0)
                } else {
                    d(self.at(i + 1, j), self.at(i - 1, j), 2.0)
                };
                let gy = if ny < 2 {
                    0.0
                } else if j == 0 {
                    d(self.at(i, 1), self.at(i, 0), 1.0)
                } else if j == ny - 1 {
                    d(self.at(i, j), self.at(i, j - 1), 1.0)
                } else {
                    d(self.at(i, j + 1), self.at(i, j - 1), 2.0)
                };
                m = m.max(libm::sqrt(gx * gx + gy * gy));
            }
        }
        m
    }
}

/// **morphology**: the polynomial smooth maximum, a soft fillet of width
/// about `k` where `a` and `b` meet (dirtlab's `smax`, :137). `k <= 0` is
/// the plain maximum.
#[must_use]
pub fn smax(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.max(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    a * (1.0 - h) + b * h + k * h * (1.0 - h)
}

/// Smoothstep of `t` clamped to [0, 1].
#[must_use]
pub fn smooth01(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance to the nearest tile edge, mm, at every node.
#[must_use]
pub fn rim(frame: &Frame) -> Field {
    let [w, h] = frame.size_mm();
    Field::from_fn(frame, |x, y| x.min(w - x).min(y).min(h - y).max(0.0))
}

// ---- noise -----------------------------------------------------------------

/// What noise: both are keyed by position, never by grid cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Noise {
    /// Gaussian-filtered white noise of this sigma, unit variance
    /// (dirtlab's `noise()`).
    Smooth {
        /// Sigma, mm.
        sigma_mm: f64,
    },
    /// Value-noise fBm at this scale over this many octaves, in about
    /// [-1, 1] (`stone.noise.fbm`).
    Fbm {
        /// The largest octave's lattice spacing, mm.
        scale_mm: f64,
        /// Octaves.
        octaves: u32,
    },
}

/// How a noise follows the system's square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// Sizes are mm at the reference square and scale with the system;
    /// keyed at reference positions, so every system shows one layout.
    Layout,
    /// Sizes are true mm (print-scale detail, default 1).
    Print,
}

/// **noise**: a position-keyed noise field over the frame.
#[must_use]
pub fn noise(frame: &Frame, seed: Seed, kind: Noise, scale: Scale) -> Field {
    let mut out = Field::new(frame, 0.0);
    // keyed positions: reference mm for layout, world mm for print
    let k = match scale {
        Scale::Layout => 1.0 / frame.scale(),
        Scale::Print => 1.0,
    };
    let xs: Vec<f64> = (0..out.nx).map(|i| out.x(i) * k).collect();
    let ys: Vec<f64> = (0..out.ny).map(|j| out.y(j) * k).collect();
    match kind {
        Noise::Smooth { sigma_mm } => {
            out.z = dt_geom::noise::smooth(seed.value(), sigma_mm, &xs, &ys);
        }
        Noise::Fbm { scale_mm, octaves } => {
            out.z
                .par_chunks_mut(xs.len())
                .zip(&ys)
                .for_each(|(row, &y)| {
                    for (o, &x) in row.iter_mut().zip(&xs) {
                        *o = dt_geom::noise::fbm(seed.value(), scale_mm, octaves, x, y);
                    }
                });
        }
    }
    out
}

/// **distance**: mm from every node to the nearest node of `mask`, and
/// that node's index (`None` everywhere for an empty mask).
#[must_use]
pub fn distance(frame: &Frame, mask: &[bool]) -> (Field, Vec<Option<usize>>) {
    let (nx, ny) = frame.nodes();
    let (d, near) = dt_geom::distance_nearest(mask, nx, ny);
    let step = frame.step_mm();
    (
        Field {
            nx,
            ny,
            step,
            z: d.into_iter().map(|v| v * step).collect(),
        },
        near,
    )
}

/// **geodesic distance**: mm from the nearest `source` node to every node
/// along 8-connected paths through `passable` nodes (Dijkstra), each step
/// weighing `weight` times its length; unreached nodes are infinite.
#[must_use]
pub fn geodesic(frame: &Frame, source: &[bool], passable: &[bool], weight: f64) -> Field {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let (d, _) = dt_geom::geodesic(source, passable, nx, ny, step * weight);
    Field { nx, ny, step, z: d }
}

// ---- scatter ---------------------------------------------------------------

/// How [`scatter`] spreads things.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    /// Darts per thing before it is given up.
    pub tries: u32,
    /// A thing of radius `r` keeps `inset[0] + inset[1]·r` mm from the
    /// tile edge.
    pub inset: [f64; 2],
    /// `true`: darts land only where the inset allows; `false`: darts land
    /// anywhere and those too near the edge are refused (a refusal uses a
    /// try).
    pub confine: bool,
    /// Two things of radii `r` and `q` keep their centres `gap +
    /// factor·(r + q)` apart.
    pub gap: f64,
    /// See `gap`.
    pub factor: f64,
}

/// A thing [`scatter`] placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its index in the radii given.
    pub index: usize,
    /// Where, mm.
    pub at: [f64; 2],
    /// Its radius, mm.
    pub r: f64,
}

/// **scatter**: things of the given radii placed largest first (stable),
/// each by up to `spread.tries` darts, a dart kept with probability
/// `density(x, y)` (in [0, 1]) where it keeps its spacing from everything
/// placed before and from `avoid` (discs `x, y, r`). Returns what was
/// placed, in placement order.
#[must_use]
pub fn scatter(
    frame: &Frame,
    rng: &mut Rng,
    radii: &[f64],
    spread: &Spread,
    density: &dyn Fn(f64, f64) -> f64,
    avoid: &[[f64; 3]],
) -> Vec<Placed> {
    let [w, h] = frame.size_mm();
    let rmax = radii
        .iter()
        .chain(avoid.iter().map(|a| &a[2]))
        .fold(0.0f64, |m, r| m.max(*r));
    let reach = spread.gap + spread.factor * 2.0 * rmax;
    let mut hash = SpatialHash::new(w, h, reach.max(frame.step_mm()));
    for a in avoid {
        hash.insert(a[0], a[1], a[2]);
    }
    let mut order: Vec<usize> = (0..radii.len()).collect();
    order.sort_by(|&a, &b| radii[b].total_cmp(&radii[a]));
    let mut out = Vec::new();
    for k in order {
        let r = radii[k];
        let inset = spread.inset[0] + spread.inset[1] * r;
        for _ in 0..spread.tries {
            let (x, y) = if spread.confine {
                (rng.uniform(inset, w - inset), rng.uniform(inset, h - inset))
            } else {
                (rng.uniform(0.0, w), rng.uniform(0.0, h))
            };
            if rng.random() > density(x, y) {
                continue;
            }
            if x.min(w - x).min(y).min(h - y) < inset {
                continue;
            }
            let clash = |d: &[f64; 3]| {
                libm::hypot(d[0] - x, d[1] - y) < spread.gap + spread.factor * (r + d[2])
            };
            if hash.any_near(x, y, reach, clash) {
                continue;
            }
            hash.insert(x, y, r);
            out.push(Placed {
                index: k,
                at: [x, y],
                r,
            });
            break;
        }
    }
    out
}

// ---- carve: the half-space cap ------------------------------------------------

/// A plane over the ground, `z = z0 + gx·x + gy·y` in a look's local
/// coordinates: a solid's top lies on or below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Height at the local origin, mm.
    pub z0: f64,
    /// Rise per mm along x.
    pub gx: f64,
    /// Rise per mm along y.
    pub gy: f64,
}

/// **carve** (the half-space cap): the height of the region under every
/// plane at local `(x, y)`, the lowest of them (`+∞` with none). A height
/// field cannot overhang, so any set of planes is printable; this is
/// rocklab's one-facet-per-plane idea in 2.5D (dirtlab's pebbles).
#[must_use]
pub fn cap(planes: &[Plane], x: f64, y: f64) -> f64 {
    planes
        .iter()
        .map(|p| p.z0 + p.gx * x + p.gy * y)
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    fn frame(square: f64) -> Frame {
        Frame {
            cols: 1,
            rows: 1,
            square_mm: square,
            cells_per_square: 100,
        }
    }

    #[test]
    fn smax_is_a_soft_max() {
        assert_eq!(smax(1.0, 3.0, 0.0), 3.0);
        assert_eq!(smax(1.0, 3.0, 0.5), 3.0);
        assert!(smax(2.0, 2.0, 0.4) > 2.0);
        assert!((smax(2.0, 2.0, 0.4) - 2.1).abs() < 1e-12);
    }

    #[test]
    fn layout_noise_is_a_photocopy_across_systems() {
        // the same relative position on a 35 mm and a 25.4 mm square
        let s = Seed::new(4).key("hills");
        let db = noise(
            &frame(35.0),
            s,
            Noise::Smooth { sigma_mm: 6.0 },
            Scale::Layout,
        );
        let ol = noise(
            &frame(25.4),
            s,
            Noise::Smooth { sigma_mm: 6.0 },
            Scale::Layout,
        );
        assert_eq!(db.z.len(), ol.z.len());
        let err =
            db.z.iter()
                .zip(&ol.z)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
        assert!(err < 1e-12, "{err}");
        // print-scale noise keeps true mm: different pattern per system
        let a = noise(
            &frame(35.0),
            s,
            Noise::Fbm {
                scale_mm: 1.6,
                octaves: 3,
            },
            Scale::Print,
        );
        let b = noise(
            &frame(25.4),
            s,
            Noise::Fbm {
                scale_mm: 1.6,
                octaves: 3,
            },
            Scale::Print,
        );
        assert_eq!(a.at(0, 0), b.at(0, 0));
        assert_ne!(a.at(50, 50), b.at(50, 50));
    }

    #[test]
    fn noise_does_not_depend_on_resolution() {
        let s = Seed::new(2);
        let mut f = frame(35.0);
        let fine = noise(&f, s, Noise::Smooth { sigma_mm: 5.0 }, Scale::Layout);
        f.cells_per_square = 50;
        let coarse = noise(&f, s, Noise::Smooth { sigma_mm: 5.0 }, Scale::Layout);
        for j in 0..=50 {
            for i in 0..=50 {
                assert!((coarse.at(i, j) - fine.at(2 * i, 2 * j)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn scatter_keeps_its_spacing_and_order() {
        let f = frame(35.0);
        let mut rng = Rng::new(Seed::new(1));
        let radii: Vec<f64> = (0..300).map(|k| 0.3 + f64::from(k % 7) * 0.2).collect();
        let spread = Spread {
            tries: 20,
            inset: [1.0, 1.0],
            confine: true,
            gap: 0.25,
            factor: 1.0,
        };
        let placed = scatter(
            &f,
            &mut rng,
            &radii,
            &spread,
            &|_, _| 1.0,
            &[[17.5, 17.5, 3.0]],
        );
        assert!(placed.len() > 50);
        for (a, p) in placed.iter().enumerate() {
            assert!(p.at[0] >= 1.0 + p.r && p.at[0] <= 34.0 - p.r);
            assert!(libm::hypot(p.at[0] - 17.5, p.at[1] - 17.5) >= 0.25 + p.r + 3.0);
            for q in &placed[..a] {
                assert!(q.r >= p.r, "largest first");
                assert!(libm::hypot(p.at[0] - q.at[0], p.at[1] - q.at[1]) >= 0.25 + p.r + q.r);
            }
        }
    }

    #[test]
    fn cap_is_the_lowest_plane() {
        let p = [
            Plane {
                z0: 1.0,
                gx: 0.5,
                gy: 0.0,
            },
            Plane {
                z0: 2.0,
                gx: -1.0,
                gy: 0.0,
            },
        ];
        assert_eq!(cap(&p, 0.0, 0.0), 1.0);
        assert_eq!(cap(&p, 2.0, 0.0), 0.0);
        assert_eq!(cap(&[], 0.0, 0.0), f64::INFINITY);
    }

    #[test]
    fn sample_is_bilinear_and_range_measures_relief() {
        let f = frame(35.0);
        let g = Field::from_fn(&f, |x, y| x + 2.0 * y);
        assert!((g.sample(1.234, 2.5) - (1.234 + 5.0)).abs() < 1e-9);
        let r = g.range(1.5);
        // a plane rising 1 mm/mm in x and 2 in y: range over a ~1.4 mm square
        assert!((r.at(50, 50) - 3.0 * 2.0 * libm::round(0.75 / g.step) * g.step).abs() < 1e-9);
    }
}
