//! The block: one cut-stone, brick or slab unit. Port of
//! `walls/masonry.py:_block_mesh` (:366): a stack of two jittered planar
//! rings (chipped arrises) whose six bounding planes are combined by a
//! smooth maximum (log-sum-exp) and meshed as a level set, so flat faces
//! are exact planes and the arrises are rounded analytically; then the
//! common stone relief.
//!
//! Printable by construction (walls R10, Rust port):
//!
//! - the ring planes carry the per-block attitude (yaw in plan, a lean of
//!   the faces) instead of a rigid rotation, so every bed stays level and
//!   a joint never opens into a wedge wider than the printer's gap;
//! - a covered top (any course but the top one) keeps its full height (no
//!   settle) and no relief within the fade band, so the bed joint stays
//!   thinner than the gap and no carved pocket is roofed by the block above;
//! - [`hold_proud`] draws each block's bottom ring so its faces never
//!   stand more than [`MAX_PROUD_MM`] proud of the top of the block below
//!   in the same face plane (per-stone proudness stays for the drybrush,
//!   and every edge keeps the same chipped profile top and bottom).

use dt_kit::form::{Relief, level_set, relief_weighted};
use dt_kit::{KitError, Rng, Seed, TriMesh};

use super::layout::Seg;

/// The most a block's bottom may stand proud of the face under it, mm
/// (true mm: a printability floor, under the printer's 0.6 mm layer reach).
pub(crate) const MAX_PROUD_MM: f64 = 0.45;

/// One block, in its segment's frame: `x` along the run from the
/// segment start, `y` along the inward normal, `z` world up.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub seg: usize,
    pub x: [f64; 2],
    pub y: [f64; 2],
    pub z: [f64; 2],
    /// Inward insets of the bottom ring's sides: x0, x1, y0, y1.
    pub bot: [f64; 4],
    /// The same for the top ring.
    pub top: [f64; 4],
    /// The top ring sits this far under the box top.
    pub drop: f64,
    /// The y faces' slope along x (yaw in plan).
    pub yaw: f64,
    /// How far the y faces' top edge leans along +y past the bottom edge.
    pub lean: f64,
    /// The top plane's slope along x and y (a tilted exposed top).
    pub tilt: [f64; 2],
    /// Which sides are seen (x0, x1, y0, y1): a face end or a wall face.
    pub exposed: [bool; 4],
    pub rounding: f64,
    /// The relief's carve depth, mm (0: none).
    pub carve: f64,
    pub dish: f64,
    /// The relief fades over this band at the bottom, and at the top when
    /// the top is covered.
    pub fade: f64,
    pub cover_top: bool,
    /// The relief's noise scale, mm.
    pub noise_mm: f64,
    pub seed: Seed,
}

impl Block {
    fn size(&self) -> [f64; 3] {
        [
            self.x[1] - self.x[0],
            self.y[1] - self.y[0],
            self.z[1] - self.z[0],
        ]
    }

    /// The outward offset of side `s` (x0, x1, y0, y1) at the bottom or
    /// top ring, along the side's outward direction in the segment frame,
    /// at its most outward point (`worst` true) or least.
    fn out(&self, s: usize, top: bool, worst: bool) -> f64 {
        let [lx, _, _] = self.size();
        let ins = if top { self.top[s] } else { self.bot[s] };
        let swing = if s >= 2 {
            self.yaw.abs() * lx / 2.0
        } else {
            0.0
        };
        let swing = if worst { swing } else { -swing };
        let lean = if top && s >= 2 { self.lean } else { 0.0 };
        match s {
            0 => -(self.x[0] + ins),
            1 => self.x[1] - ins,
            2 => -(self.y[0] + ins + lean) + swing,
            _ => self.y[1] - ins + lean + swing,
        }
    }

    /// The six planes (unit outward normal, offset) in the block's local
    /// frame (origin at the box's low corner).
    fn planes(&self) -> Vec<([f64; 3], f64)> {
        let [lx, ly, lz] = self.size();
        let zt = lz - self.drop;
        let (xc, yc) = (lx / 2.0, ly / 2.0);
        let s = |k: usize| (self.top[k] - self.bot[k]) / zt;
        let k = self.yaw;
        let unit = |n: [f64; 3], c: f64| {
            let l = libm::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
            (n.map(|v| v / l), c / l)
        };
        // each plane as n·p − c (positive outside)
        let ly0 = s(2) + self.lean / zt;
        let ly1 = self.lean / zt - s(3);
        vec![
            unit([0.0, 0.0, -1.0], 0.0),
            unit(
                [-self.tilt[0], -self.tilt[1], 1.0],
                zt - self.tilt[0] * xc - self.tilt[1] * yc,
            ),
            unit([-1.0, 0.0, s(0)], -self.bot[0]),
            unit([1.0, 0.0, s(1)], lx - self.bot[1]),
            // y ≥ bot2 + ly0 z + k (x − xc)
            unit([k, -1.0, ly0], k * xc - self.bot[2]),
            // y ≤ ly − bot3 + ly1 z + k (x − xc)
            unit([-k, 1.0, -ly1], ly - self.bot[3] - k * xc),
        ]
    }

    /// The block's mesh in world mm: the level set of the smooth max of
    /// its planes, the relief, placed on `seg`.
    pub fn mesh(&self, seg: &Seg) -> Result<TriMesh, KitError> {
        let [lx, ly, lz] = self.size();
        let planes = self.planes();
        let pitch = (lx.max(ly).max(lz) / 56.0).clamp(0.18, 0.32);
        let tau = (0.7 * self.rounding)
            .min(0.22 * lx.min(ly).min(lz))
            .max(0.6 * pitch);
        let sdf = |p: [f64; 3]| {
            let f: Vec<f64> = planes
                .iter()
                .map(|(n, c)| (n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - c) / tau)
                .collect();
            let m = f.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let s: f64 = f.iter().map(|v| libm::exp(v - m)).sum();
            -tau * (m + libm::log(s))
        };
        let swing = self.yaw.abs() * lx / 2.0 + self.lean.abs();
        let pad = 2.5 * pitch;
        let tz = (self.tilt[0].abs() * lx + self.tilt[1].abs() * ly) / 2.0;
        let mut m = level_set(
            &sdf,
            [-pad, -pad - swing, -pad],
            [lx + pad, ly + pad + swing, lz + pad + tz],
            pitch,
        )?;
        if self.carve > 0.0 {
            let r = Relief {
                scale_mm: self.noise_mm,
                carve_mm: self.carve,
                dish_mm: self.dish,
                ..Relief::default()
            };
            let (fade, zt, cover) = (self.fade, lz - self.drop, self.cover_top);
            relief_weighted(&mut m, self.seed.key("relief").value(), &r, &|p, _| {
                let lo = (p[2] / fade).clamp(0.0, 1.0);
                let hi = if cover {
                    ((zt - p[2]) / fade).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                lo * hi
            });
        }
        let (x0, y0, z0) = (self.x[0], self.y[0], self.z[0]);
        for p in &mut m.positions {
            let w = seg.at(x0 + p[0], y0 + p[1]);
            *p = [w[0], w[1], z0 + p[2]];
        }
        Ok(m)
    }
}

/// A block's surface finish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Finish {
    pub chip: f64,
    pub rounding: f64,
    /// The preset's relief, mm (`relief_mm`).
    pub relief: f64,
    /// Settle of an exposed top below the cap plane.
    pub settle: f64,
    /// Yaw and lean bound, degrees.
    pub yaw_deg: f64,
    pub tilt_deg: f64,
    pub scale: f64,
}

/// `_block_mesh`'s draws for one block: the ring pulls (per ring, per
/// side, never per corner: faces stay planar), the settle of an exposed
/// top, the attitude, the relief depth. `mask` zeroes the pull on a side
/// (floor slabs run flush to the tile edge); the draws happen anyway.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    rng: &mut Rng,
    seg: usize,
    bx: ([f64; 2], [f64; 2], [f64; 2]),
    f: &Finish,
    exposed_top: bool,
    tilted_top: bool,
    mask: [bool; 4],
    seed: Seed,
) -> Block {
    let (x, y, z) = bx;
    let (lx, ly, lz) = (x[1] - x[0], y[1] - y[0], z[1] - z[0]);
    let px = f.chip.min(0.22 * lx);
    let py = f.chip.min(0.22 * ly);
    let ring = |rng: &mut Rng| -> [f64; 4] {
        let d = [
            rng.uniform(0.0, px),
            rng.uniform(0.0, px),
            rng.uniform(0.0, py),
            rng.uniform(0.0, py),
        ];
        [0, 1, 2, 3].map(|k| if mask[k] { d[k] } else { 0.0 })
    };
    let settle = rng.uniform(0.0, f.settle);
    let top = ring(rng);
    let bot = ring(rng);
    let yaw = libm::tan(rng.uniform(-f.yaw_deg, f.yaw_deg).to_radians());
    let lean = lz * libm::tan(rng.uniform(-f.tilt_deg, f.tilt_deg).to_radians());
    let tilt = [0, 1].map(|_| libm::tan(rng.uniform(-f.tilt_deg, f.tilt_deg).to_radians()));
    let mn = lx.min(ly).min(lz);
    let s = f.scale;
    let carve = if f.relief > 0.0 {
        (0.62 * s * f.relief / (0.10 * s))
            .min(0.62 * s)
            .min(0.14 * mn)
    } else {
        0.0
    };
    Block {
        seg,
        x,
        y,
        z,
        bot,
        top,
        drop: if exposed_top { settle } else { 0.0 },
        yaw,
        lean,
        tilt: if tilted_top { tilt } else { [0.0; 2] },
        exposed: [false; 4],
        rounding: f.rounding,
        carve,
        dish: (0.25 * s).min(0.06 * mn),
        fade: 1.2 * s,
        cover_top: !exposed_top,
        noise_mm: 7.0 * s,
        seed,
    }
}

/// Bring each block's bottom ring in so that, on every seen side, it
/// stands at most [`MAX_PROUD_MM`] proud of the top ring of any block
/// directly under it in the same face plane. Blocks are taken bottom up,
/// so a block is held by the final top of the block under it.
pub(crate) fn hold_proud(blocks: &mut [Block], segs: &[Seg]) {
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|&a, &b| blocks[a].z[0].total_cmp(&blocks[b].z[0]).then(a.cmp(&b)));
    // world outward direction and plane offset of side s of block b
    let side = |b: &Block, s: usize| -> ([f64; 2], [f64; 2]) {
        let g = &segs[b.seg];
        let dir = match s {
            0 => [-g.d[0], -g.d[1]],
            1 => g.d,
            2 => [-g.n[0], -g.n[1]],
            _ => g.n,
        };
        // the side's lateral extent along the perpendicular
        let perp = [-dir[1], dir[0]];
        let corners = [
            g.at(b.x[0], b.y[0]),
            g.at(b.x[1], b.y[0]),
            g.at(b.x[0], b.y[1]),
            g.at(b.x[1], b.y[1]),
        ];
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for c in corners {
            let v = perp[0] * c[0] + perp[1] * c[1];
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (dir, [lo, hi])
    };
    // offset of the segment frame along a world direction
    let base = |b: &Block, dir: [f64; 2]| {
        let g = &segs[b.seg];
        dir[0] * g.a[0] + dir[1] * g.a[1]
    };
    for (n, &u) in order.iter().enumerate() {
        for s in 0..4 {
            if !blocks[u].exposed[s] {
                continue;
            }
            let (dir, lat) = side(&blocks[u], s);
            let mine = base(&blocks[u], dir) + blocks[u].out(s, false, true);
            let mut need: f64 = 0.0;
            for &l in &order[..n] {
                let lb = &blocks[l];
                if (lb.z[1] - blocks[u].z[0]).abs() > 0.6 {
                    continue;
                }
                for t in 0..4 {
                    if !lb.exposed[t] {
                        continue;
                    }
                    let (d2, lat2) = side(lb, t);
                    if dir[0] * d2[0] + dir[1] * d2[1] < 0.99
                        || lat[0].max(lat2[0]) > lat[1].min(lat2[1]) - 0.05
                    {
                        continue;
                    }
                    let theirs = base(lb, d2) + lb.out(t, true, false);
                    if (theirs - mine).abs() > 2.0 {
                        continue;
                    }
                    need = need.max(mine - (theirs + MAX_PROUD_MM));
                }
            }
            if need > 0.0 {
                blocks[u].bot[s] += need;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn seg() -> Seg {
        Seg {
            a: [0.0, 0.0],
            d: [1.0, 0.0],
            n: [0.0, 1.0],
            len: 40.0,
        }
    }

    fn finish() -> Finish {
        Finish {
            chip: 0.55,
            rounding: 0.22,
            relief: 0.07,
            settle: 0.2,
            yaw_deg: 1.2,
            tilt_deg: 0.7,
            scale: 1.0,
        }
    }

    #[test]
    fn a_block_is_closed_and_fills_its_box() {
        let mut rng = Rng::new(Seed::new(5));
        let b = draw(
            &mut rng,
            0,
            ([2.0, 17.0], [0.3, 6.8], [5.0, 12.0]),
            &finish(),
            false,
            false,
            [true; 4],
            Seed::new(9),
        );
        let m = b.mesh(&seg()).unwrap();
        assert!(m.is_watertight());
        let v = dt_kit::signed_volume(&m);
        assert!(
            v > 0.8 * 15.0 * 6.5 * 7.0 && v < 15.0 * 6.5 * 7.0 * 1.02,
            "{v}"
        );
        // a covered top is flat at the box top (no settle, no relief)
        let top = m.positions.iter().map(|p| p[2]).fold(0.0, f64::max);
        assert!((top - 12.0).abs() < 0.02, "{top}");
    }

    #[test]
    fn a_block_never_stands_far_proud_of_the_block_below() {
        let f = finish();
        let mut blocks = Vec::new();
        for k in 0..2 {
            let mut rng = Rng::new(Seed::new(k));
            let z = if k == 0 { [0.0, 7.0] } else { [7.1, 14.0] };
            let mut b = draw(
                &mut rng,
                0,
                ([2.0, 17.0], [0.0, 7.0], z),
                &f,
                false,
                false,
                [true; 4],
                Seed::new(k),
            );
            b.exposed = [false, false, true, true];
            blocks.push(b);
        }
        // the lower block's top recessed, the upper's bottom proud
        blocks[0].top[2] = 0.9;
        blocks[1].bot[2] = 0.0;
        hold_proud(&mut blocks, &[seg()]);
        let upper = -(blocks[1].y[0] + blocks[1].bot[2]) + blocks[1].yaw.abs() * 7.5;
        let lower =
            -(blocks[0].y[0] + blocks[0].top[2] + blocks[0].lean) - blocks[0].yaw.abs() * 7.5;
        assert!(upper <= lower + MAX_PROUD_MM + 1e-9, "{upper} {lower}");
    }
}
