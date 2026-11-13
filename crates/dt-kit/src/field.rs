//! Fields: values over the tile on the one grid convention, and the field
//! operations elements are written in (noise, distance, morphology with the
//! smooth maximum, scatter, the half-space cap).
//!
//! The grid convention (the only one): a field
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

use dt_geom::{Grid, SpatialHash};
use dt_value::{Rng, Seed};
use rayon::prelude::*;

use crate::Frame;

/// Values at every node of a frame's lattice, in world mm: dt-geom's
/// [`Grid`] of numbers, with its morphology (blur, dilate, erode, range,
/// quantile, gradient) and sampling.
pub type Field = Grid<f64>;

/// A field made on a frame's lattice: `Field::new(&frame, 0.0)`.
pub trait OnFrame: Sized {
    /// A field of `value` on the frame's lattice.
    #[must_use]
    fn new(frame: &Frame, value: f64) -> Self;

    /// A field from a function of position (mm).
    #[must_use]
    fn from_fn(frame: &Frame, f: impl Fn(f64, f64) -> f64 + Sync) -> Self;
}

impl OnFrame for Field {
    fn new(frame: &Frame, value: f64) -> Self {
        let (nx, ny) = frame.nodes();
        Grid::filled(nx, ny, frame.step_mm(), value)
    }

    fn from_fn(frame: &Frame, f: impl Fn(f64, f64) -> f64 + Sync) -> Self {
        let mut out = <Self as OnFrame>::new(frame, 0.0);
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
}

/// **morphology**: the polynomial smooth maximum, a soft fillet of width
/// about `k` where `a` and `b` meet. `k <= 0` is
/// the plain maximum.
#[must_use]
pub fn smax(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.max(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    a * (1.0 - h) + b * h + k * h * (1.0 - h)
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
    /// (the keyed noise).
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

/// **noise at points**: the smooth noise (unit variance, Gaussian of
/// `sigma_mm`) at the grid `xs × ys`, in mm of the keyed space (row-major,
/// `y` outer). For fields that reach past the tile, such as grass's flow.
#[must_use]
pub fn noise_at(seed: Seed, sigma_mm: f64, xs: &[f64], ys: &[f64]) -> Vec<f64> {
    dt_geom::noise::smooth(seed.value(), sigma_mm, xs, ys)
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

/// A plane over the ground, `z = z0 + gx·x + gy·y` in an element's local
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
/// the one-facet-per-plane idea in 2.5D.
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
