//! Fit: a stone's top plane is made to fit the body's band as the stone is
//! made ([`dt_kit::pieces::Pieces`]), never trimmed after. A stone that
//! reaches the band takes the plane nearest what the stack drew that is
//! inside the band over every node of the region it stands on: a single
//! height is one plane every top touches, a wide band lets each stone tilt
//! and stand at its own height. A stone that stops short of the band (a
//! course lower down) is left as it is.

use dt_kit::pieces::{Pieces, Plane};
use dt_kit::{Band, Frame, Mask, Refusal};

use super::form::GRID_MM;
use super::util::Cells;

/// The band's fitter on the tile's lattice.
pub struct Fit<'a> {
    pieces: Pieces<'a>,
    band: &'a Band,
    mask: &'a Mask,
    nx: usize,
    ny: usize,
    step: f64,
    /// The foot the stone heights are counted from, world mm.
    foot: f64,
}

impl<'a> Fit<'a> {
    pub fn new(band: &'a Band, frame: &Frame, mask: &'a Mask, foot: f64) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            pieces: Pieces::new(band, frame),
            band,
            mask,
            nx,
            ny,
            step: frame.step_mm(),
            foot,
        }
    }

    fn pos(&self, k: usize) -> [f64; 2] {
        [
            (k % self.nx) as f64 * self.step,
            (k / self.nx) as f64 * self.step,
        ]
    }

    /// The region's nodes a footprint touches: every node inside the extent
    /// of a cell of it (the nodes at or beside the cell's edges included),
    /// so a plane under the band at all of them is under it at every point
    /// of the footprint.
    fn nodes(&self, cells: &Cells) -> Vec<usize> {
        let mut seen = vec![false; self.nx * self.ny];
        let mut out = Vec::new();
        for cj in 0..cells.ny {
            for ci in 0..cells.nx {
                if !cells.at(ci, cj) {
                    continue;
                }
                let (is, js) = touching(ci, cj, self.step, self.nx, self.ny);
                for nj in js {
                    for ni in is.clone() {
                        let k = nj * self.nx + ni;
                        if self.mask.bits[k] && !seen[k] {
                            seen[k] = true;
                            out.push(k);
                        }
                    }
                }
            }
        }
        out
    }

    /// Whether a stone's top plane is under the band's high over `cells`
    /// as it is: what a crumb merged into the stone must keep.
    pub fn under(&self, cells: &Cells, (c, a, b): (f64, f64, f64)) -> bool {
        self.nodes(cells).iter().all(|&k| {
            let p = self.pos(k);
            c + a * p[0] + b * p[1] + self.foot <= self.band.at(k).hi + 1e-9
        })
    }

    /// A stone's top plane `z = c + a x + b y` (above the foot), fitted.
    ///
    /// # Errors
    /// None now: a stone the band jumps across is cut flat at its lowest high.
    pub fn top(
        &self,
        cells: &Cells,
        (c, a, b): (f64, f64, f64),
    ) -> Result<(f64, f64, f64), Refusal> {
        let nodes = self.nodes(cells);
        if nodes.is_empty() {
            return Ok((c, a, b));
        }
        let z = |p: [f64; 2]| c + a * p[0] + b * p[1] + self.foot;
        let reaches = nodes
            .iter()
            .any(|&k| z(self.pos(k)) >= self.band.at(k).lo - 1e-9);
        if !reaches {
            return Ok((c, a, b));
        }
        let n = nodes.len() as f64;
        let at = nodes.iter().fold([0.0, 0.0], |s, &k| {
            let p = self.pos(k);
            [s[0] + p[0] / n, s[1] + p[1] / n]
        });
        let got = match self.pieces.plane(
            &nodes,
            Plane {
                at,
                z: z(at),
                g: [a, b],
            },
        ) {
            Ok(p) => p,
            // the band jumps across the stone: it is cut flat at the lowest
            // of its highs (stood over the lower side, short of the higher)
            Err(_) => Plane {
                at,
                z: nodes
                    .iter()
                    .map(|&k| self.band.at(k).hi)
                    .fold(f64::INFINITY, f64::min),
                g: [0.0, 0.0],
            },
        };
        Ok((
            got.z - self.foot - got.g[0] * at[0] - got.g[1] * at[1],
            got.g[0],
            got.g[1],
        ))
    }
}

/// The lattice nodes touching cell `(ci, cj)` of the pile's grid: those in
/// its extent, and the nearest beyond it (so every point of the cell is
/// between touching nodes).
pub fn touching(
    ci: usize,
    cj: usize,
    step: f64,
    nx: usize,
    ny: usize,
) -> (
    std::ops::RangeInclusive<usize>,
    std::ops::RangeInclusive<usize>,
) {
    let range = |c: usize, n: usize| {
        let lo = ((c as f64 * GRID_MM) / step).floor() as usize;
        let hi = ((((c + 1) as f64) * GRID_MM) / step).ceil() as usize;
        lo.min(n - 1)..=hi.min(n - 1)
    };
    (range(ci, nx), range(cj, ny))
}
