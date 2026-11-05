//! The masonry layout: a spine split into straight segments, courses ×
//! bays × quoins as cells, then battlements and ruin as edits of the cell
//! list.

use dt_kit::Rng;

/// A segment fails the layout's rules.
pub(crate) type LayoutError = String;

/// One straight run of the spine: from `a` along `d` for `len` mm, the
/// body to the left (`n`, the inward normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Seg {
    /// The outer-face start, world mm.
    pub a: [f64; 2],
    /// Unit run direction.
    pub d: [f64; 2],
    /// Unit inward normal (left of `d`).
    pub n: [f64; 2],
    /// Length, mm.
    pub len: f64,
}

impl Seg {
    /// World plan point at run `t`, depth `q`.
    pub fn at(&self, t: f64, q: f64) -> [f64; 2] {
        [
            self.a[0] + self.d[0] * t + self.n[0] * q,
            self.a[1] + self.d[1] * t + self.n[1] * q,
        ]
    }
}

/// `_segments`: the spine (world mm) as segments; corners must be right
/// angles (gen-1).
pub(crate) fn segments(spine: &[[f64; 2]]) -> Result<Vec<Seg>, LayoutError> {
    if spine.len() < 2 {
        return Err("a wall needs a path of at least one edge".into());
    }
    let mut segs = Vec::new();
    for w in spine.windows(2) {
        let v = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
        let len = libm::hypot(v[0], v[1]);
        if len < 1e-6 {
            return Err("the wall's path has a zero-length edge".into());
        }
        let d = [v[0] / len, v[1] / len];
        segs.push(Seg {
            a: w[0],
            d,
            n: [-d[1], d[0]],
            len,
        });
    }
    for w in segs.windows(2) {
        if (w[0].d[0] * w[1].d[0] + w[0].d[1] * w[1].d[1]).abs() > 1e-3 {
            return Err("a wall turns only at right angles".into());
        }
    }
    Ok(segs)
}

/// What a cell's end along the run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// A joint with the next unit.
    Joint,
    /// A visible, textured end (a free wall end, a corner arris, a merlon
    /// flank).
    Face,
}

/// One unit's cell: an interval on a segment × a course (wall-local z,
/// 0 at the seat).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cell {
    pub seg: usize,
    pub t0: f64,
    pub t1: f64,
    pub end0: End,
    pub end1: End,
    pub z0: f64,
    pub z1: f64,
    pub is_top: bool,
    pub is_bottom: bool,
    pub is_quoin: bool,
    /// (course, segment, bay, tags…): the unit's identity, keying its
    /// random stream.
    pub key: Vec<i64>,
    /// Fieldstone: the course bands a merged throughstone spans.
    pub side_bands: Option<Vec<(i64, f64, f64)>>,
    /// Fieldstone: an on-edge coping stone.
    pub coping: bool,
}

/// How the courses and bays are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bond {
    /// Jittered courses and bays, joints kept apart (R5).
    Irregular,
    /// Uniform courses and units in half-unit running bond.
    Running,
}

/// Bays shorter than this × the least bay are merged away.
pub(crate) const MIN_BAY_FRAC: f64 = 0.45;
/// Running bond: the least end closer, mm.
const CLOSER_MIN: f64 = 1.5;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn count(total: f64, lo: f64, hi: f64) -> usize {
    (libm::round(total / (0.5 * (lo + hi))) as usize).max(1)
}

/// `_course_heights`: varied heights summing exactly to `h`.
pub(crate) fn course_heights(h: f64, lo: f64, hi: f64, rng: &mut Rng) -> Vec<f64> {
    let n = count(h, lo, hi);
    let v: Vec<f64> = (0..n).map(|_| rng.uniform(lo, hi)).collect();
    let sum: f64 = v.iter().sum();
    v.into_iter().map(|x| x * h / sum).collect()
}

fn running_heights(h: f64, lo: f64, hi: f64) -> Vec<f64> {
    let n = count(h, lo, hi);
    #[allow(clippy::cast_precision_loss)]
    let each = h / n as f64;
    vec![each; n]
}

/// `_bay_cuts`: interior cuts in `[t0, t1]`, each kept `min_bond` from
/// the cuts of the course below.
fn bay_cuts(
    t0: f64,
    t1: f64,
    (lo, hi): (f64, f64),
    prev: &[f64],
    min_bond: f64,
    rng: &mut Rng,
) -> Vec<f64> {
    let len = t1 - t0;
    let n = count(len, lo, hi);
    let w: Vec<f64> = (0..n).map(|_| rng.uniform(lo, hi)).collect();
    let sum: f64 = w.iter().sum();
    let mut cuts = Vec::with_capacity(n);
    let mut acc = 0.0;
    for x in &w[..n - 1] {
        acc += x;
        cuts.push(t0 + acc * len / sum);
    }
    for c in &mut cuts {
        let Some(&p) = prev
            .iter()
            .min_by(|a, b| (*a - *c).abs().total_cmp(&(*b - *c).abs()))
        else {
            break;
        };
        let dv = *c - p;
        if dv.abs() < min_bond {
            *c = p + if dv >= 0.0 { min_bond } else { -min_bond };
        }
    }
    let (a, b) = (t0 + MIN_BAY_FRAC * lo, t1 - MIN_BAY_FRAC * lo);
    let mut cuts: Vec<f64> = cuts
        .into_iter()
        .map(|c| c.clamp(a.min(b), b.max(a)))
        .collect();
    cuts.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::with_capacity(cuts.len());
    for c in cuts {
        if out.last().is_none_or(|&l| c - l > MIN_BAY_FRAC * lo) {
            out.push(c);
        }
    }
    out
}

/// `_running_cuts`: a grid of `brick` mm phase-shifted by `offset`.
#[allow(clippy::cast_possible_truncation)]
fn running_cuts(t0: f64, t1: f64, brick: f64, offset: f64) -> Vec<f64> {
    let lo = libm::floor((t0 - offset) / brick) as i64;
    let hi = libm::ceil((t1 - offset) / brick) as i64;
    #[allow(clippy::cast_precision_loss)]
    (lo..=hi)
        .map(|k| offset + k as f64 * brick)
        .filter(|&c| c > t0 + CLOSER_MIN && c < t1 - CLOSER_MIN)
        .collect()
}

/// The layout's inputs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Courses {
    pub thickness: f64,
    pub height: f64,
    pub course: (f64, f64),
    pub bay: (f64, f64),
    pub min_bond: f64,
    pub bond: Bond,
}

/// `_layout`: courses × bays × quoin alternation → cells (R5, R7). Wall
/// ends are always textured faces.
pub(crate) fn layout(segs: &[Seg], c: &Courses, rng: &mut Rng) -> Vec<Cell> {
    let t = c.thickness;
    let running = c.bond == Bond::Running;
    let heights = if running {
        running_heights(c.height, c.course.0, c.course.1)
    } else {
        course_heights(c.height, c.course.0, c.course.1, rng)
    };
    let brick = 0.5 * (c.bay.0 + c.bay.1);
    let mut z_edges = vec![0.0];
    for h in &heights {
        z_edges.push(z_edges.last().copied().unwrap_or(0.0) + h);
    }
    let n_joints = segs.len() - 1;
    let mut cells = Vec::new();
    let mut prev: Vec<Vec<f64>> = vec![Vec::new(); segs.len()];
    for (ci, w) in z_edges.windows(2).enumerate() {
        let (z0, z1) = (w[0], w[1]);
        let is_top = ci == heights.len() - 1;
        let is_bottom = ci == 0;
        for (k, seg) in segs.iter().enumerate() {
            let owns_start = k > 0 && (ci + k - 1) % 2 != 0;
            let owns_end = k < n_joints && (ci + k) % 2 == 0;
            let t0 = if k == 0 || owns_start { 0.0 } else { t };
            let t1 = if k == n_joints || owns_end {
                seg.len
            } else {
                seg.len - t
            };
            if t1 - t0 < MIN_BAY_FRAC * c.bay.0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let mut cuts = if running {
                running_cuts(t0, t1, brick, (ci % 2) as f64 * 0.5 * brick)
            } else {
                bay_cuts(t0, t1, c.bay, &prev[k], c.min_bond, rng)
            };
            if k > 0 && owns_start {
                cuts.retain(|&x| x >= t + 2.0);
            }
            if k < n_joints && owns_end {
                cuts.retain(|&x| x <= seg.len - t - 2.0);
            }
            prev[k].clone_from(&cuts);
            let mut edges = vec![t0];
            edges.extend(&cuts);
            edges.push(t1);
            for (b, e) in edges.windows(2).enumerate() {
                let (ta, tb) = (e[0], e[1]);
                #[allow(clippy::float_cmp)]
                cells.push(Cell {
                    seg: k,
                    t0: ta,
                    t1: tb,
                    end0: if ta > t0 || (k > 0 && !owns_start) {
                        End::Joint
                    } else {
                        End::Face
                    },
                    end1: if tb < t1 || (k < n_joints && !owns_end) {
                        End::Joint
                    } else {
                        End::Face
                    },
                    z0,
                    z1,
                    is_top,
                    is_bottom,
                    is_quoin: (ta == t0 && k > 0) || (tb == t1 && k < n_joints),
                    key: vec![ci as i64, k as i64, b as i64],
                    side_bands: None,
                    coping: false,
                });
            }
        }
    }
    cells
}

/// Battlements: where the parapet stands, per segment.
#[derive(Clone, Debug, Default)]
pub(crate) struct Battlements {
    /// Wall-local z of the crenel floor.
    pub floor_z: f64,
    /// Merlon intervals along each segment.
    pub merlons: Vec<Vec<(f64, f64)>>,
}

/// `_crenellate`: the crenel floor lands on the course boundary nearest
/// `height − depth`; parapet cells survive only inside merlons, which
/// stand at every segment end (every corner is a merlon).
pub(crate) fn crenellate(
    cells: Vec<Cell>,
    segs: &[Seg],
    height: f64,
    (merlon, crenel, depth): ((f64, f64), (f64, f64), f64),
    rng: &mut Rng,
) -> (Vec<Cell>, Option<Battlements>) {
    let mut zs: Vec<f64> = cells
        .iter()
        .map(|c| c.z1)
        .filter(|&z| z < height - 1e-6)
        .collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let Some(&zf) = zs.iter().min_by(|a, b| {
        (*a - (height - depth))
            .abs()
            .total_cmp(&(*b - (height - depth)).abs())
    }) else {
        return (cells, None);
    };
    let wm_avg = 0.5 * (merlon.0 + merlon.1);
    let wc_avg = 0.5 * (crenel.0 + crenel.1);
    let mut all = Vec::new();
    for seg in segs {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n_c = (libm::round((seg.len - wm_avg) / (wm_avg + wc_avg)) as usize).max(1);
        let wm: Vec<f64> = (0..=n_c).map(|_| rng.uniform(merlon.0, merlon.1)).collect();
        let wc: Vec<f64> = (0..n_c).map(|_| rng.uniform(crenel.0, crenel.1)).collect();
        let scale = seg.len / (wm.iter().sum::<f64>() + wc.iter().sum::<f64>());
        let mut t = 0.0;
        let mut m = Vec::new();
        for i in 0..=n_c {
            m.push((t, t + wm[i] * scale));
            t += wm[i] * scale + if i < n_c { wc[i] * scale } else { 0.0 };
        }
        all.push(m);
    }
    let mut out = Vec::new();
    for c in cells {
        if c.z0 < zf - 1e-6 {
            out.push(c);
            continue;
        }
        for (mi, &(m0, m1)) in all[c.seg].iter().enumerate() {
            let (lo, hi) = (c.t0.max(m0), c.t1.min(m1));
            if hi - lo < 1.5 {
                continue;
            }
            let mut key = c.key.clone();
            #[allow(clippy::cast_possible_wrap)]
            key.push(mi as i64);
            out.push(Cell {
                t0: lo,
                t1: hi,
                end0: if lo > c.t0 + 1e-6 { End::Face } else { c.end0 },
                end1: if hi < c.t1 - 1e-6 { End::Face } else { c.end1 },
                is_bottom: false,
                key,
                ..c.clone()
            });
        }
    }
    (
        out,
        Some(Battlements {
            floor_z: zf,
            merlons: all,
        }),
    )
}

/// The ruin's break line along one segment: two slow sines.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Envelope {
    wl: [f64; 2],
    ph: [f64; 2],
    height: f64,
    ruin: f64,
}

/// The envelope drop is `ruin × H × ` this range.
const RUIN_DROP: (f64, f64) = (0.20, 0.80);
/// The chance a block straddling the break survives.
const RUIN_KEEP_P: f64 = 0.5;
/// Envelope wavelengths, mm (longer than a segment: the break undulates
/// slowly).
const RUIN_WOBBLE_WL: (f64, f64) = (45.0, 110.0);

impl Envelope {
    /// Wall-local z of the break at run `t`.
    pub fn at(&self, t: f64) -> f64 {
        let s: f64 = (0..2)
            .map(|k| libm::sin(2.0 * core::f64::consts::PI * t / self.wl[k] + self.ph[k]))
            .sum();
        let n01 = 0.5 + s / 4.0;
        self.height * (1.0 - self.ruin * (RUIN_DROP.0 + (RUIN_DROP.1 - RUIN_DROP.0) * n01))
    }

    /// The lowest point of the break along `[0, len]` (25 samples).
    pub fn min(&self, len: f64) -> f64 {
        (0..25)
            .map(|i| self.at(len * f64::from(i) / 24.0))
            .fold(f64::INFINITY, f64::min)
    }
}

/// A cell's plan rectangle `[x0, y0, x1, y1]`.
fn plan(segs: &[Seg], c: &Cell, thickness: f64) -> [f64; 4] {
    let g = &segs[c.seg];
    let p = [g.at(c.t0, 0.0), g.at(c.t1, thickness)];
    [
        p[0][0].min(p[1][0]),
        p[0][1].min(p[1][1]),
        p[0][0].max(p[1][0]),
        p[0][1].max(p[1][1]),
    ]
}

/// `_ruin_cells`: blocks above the break go; blocks straddling it
/// survive at random. A block stays only if every block under it stayed
/// (a ruin never leaves a stone hanging over a fallen one).
pub(crate) fn ruin_cells(
    cells: Vec<Cell>,
    segs: &[Seg],
    thickness: f64,
    height: f64,
    ruin: f64,
    scale: f64,
    rng: &mut Rng,
) -> (Vec<Cell>, Vec<Envelope>) {
    let env: Vec<Envelope> = segs
        .iter()
        .map(|_| Envelope {
            wl: [0, 1].map(|_| scale * rng.uniform(RUIN_WOBBLE_WL.0, RUIN_WOBBLE_WL.1)),
            ph: [0, 1].map(|_| rng.uniform(0.0, 2.0 * core::f64::consts::PI)),
            height,
            ruin,
        })
        .collect();
    let mut kept: Vec<Cell> = Vec::new();
    let mut gone: Vec<Cell> = Vec::new();
    let mut order: Vec<Cell> = cells;
    order.sort_by(|a, b| a.z0.total_cmp(&b.z0));
    for mut c in order {
        let e = env[c.seg].at(0.5 * (c.t0 + c.t1));
        let drop = c.z0 >= e || (c.z1 > e && rng.random() > RUIN_KEEP_P);
        // a fallen block under this one (overlapping in plan, on either
        // segment: corner blocks of one segment stand on the other's;
        // its top at this one's bottom)
        let unsupported = gone.iter().any(|g| {
            let (a, b) = (plan(segs, g, thickness), plan(segs, &c, thickness));
            (g.z1 - c.z0).abs() < 0.6
                && a[0].max(b[0]) < a[2].min(b[2]) - 0.5
                && a[1].max(b[1]) < a[3].min(b[3]) - 0.5
        });
        if drop || unsupported {
            gone.push(c);
            continue;
        }
        c.is_top = false;
        kept.push(c);
    }
    (kept, env)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn corner() -> Vec<Seg> {
        segments(&[[35.0, 35.0], [0.0, 35.0], [0.0, 0.0]]).unwrap()
    }

    #[test]
    fn courses_sum_to_the_height_and_joints_never_line_up() {
        let segs = corner();
        let mut rng = Rng::new(dt_kit::Seed::new(3));
        let c = Courses {
            thickness: 7.0,
            height: 28.0,
            course: (5.5, 8.5),
            bay: (11.0, 20.0),
            min_bond: 3.0,
            bond: Bond::Irregular,
        };
        let cells = layout(&segs, &c, &mut rng);
        let top = cells.iter().map(|c| c.z1).fold(0.0, f64::max);
        assert!((top - 28.0).abs() < 1e-9);
        // per course and segment the cells tile [t0, t1] without gaps
        for a in &cells {
            for b in &cells {
                if a.seg == b.seg
                    && a.key[0] + 1 == b.key[0]
                    && a.end1 == End::Joint
                    && b.end1 == End::Joint
                    && b.t1 < a.t1 + 20.0
                {
                    assert!((a.t1 - b.t1).abs() > 1.0 || a.t1 > segs[a.seg].len - 8.0);
                }
            }
        }
        // the corner alternates: each course one segment runs through
        for ci in 0..=cells.iter().map(|c| c.key[0]).max().unwrap() {
            let through = cells
                .iter()
                .filter(|c| {
                    c.key[0] == ci && c.is_quoin && c.end0 == End::Face
                        || c.key[0] == ci && c.is_quoin && c.end1 == End::Face
                })
                .count();
            assert_eq!(through, 1, "course {ci}");
        }
    }

    #[test]
    fn running_bond_offsets_half_a_unit() {
        let segs = segments(&[[35.0, 35.0], [0.0, 35.0]]).unwrap();
        let mut rng = Rng::new(dt_kit::Seed::new(1));
        let c = Courses {
            thickness: 7.0,
            height: 28.8,
            course: (3.2, 3.2),
            bay: (8.5, 8.5),
            min_bond: 0.0,
            bond: Bond::Running,
        };
        let cells = layout(&segs, &c, &mut rng);
        assert_eq!(cells.iter().map(|c| c.key[0]).max().unwrap(), 8);
        let cuts = |ci: i64| -> Vec<f64> {
            cells
                .iter()
                .filter(|c| c.key[0] == ci && c.end1 == End::Joint)
                .map(|c| c.t1)
                .collect()
        };
        for x in cuts(1) {
            assert!(cuts(0).iter().all(|y| (x - y).abs() > 4.0));
        }
    }
}
