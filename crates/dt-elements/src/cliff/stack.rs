//! 2 Stack: rocks resting on rocks. A support grid
//! S starts as the ground. Repeatedly take the lowest spot still short of
//! T, pick a kind, grow its footprint over the pocket there, rest it on its
//! highest supports (tilted where they are uneven) and raise S to its top.
//! No gaps by construction: every rock's bed is the pile.

use dt_kit::Rng;

use super::form::{FOOT, Form, GRID_MM, MIN_H_MM, ZONE_FLOOR, Zones};
use super::util::{
    Draw, Field, G, Mask, P2, bin_dilate, blur, edt_in, label, lerp, min_side, trace_outline,
};

/// kind: (p, along, depth, height (None: ground to top), bridge, tilt deg)
#[allow(clippy::type_complexity)]
pub const KINDS: [(
    &str,
    f64,
    (f64, f64),
    (f64, f64),
    Option<(f64, f64)>,
    f64,
    f64,
); 5] = [
    (
        "column",
        0.38,
        (8.0, 14.0),
        (7.0, 12.0),
        Some((15.0, 24.0)),
        1.2,
        8.0,
    ),
    (
        "block",
        0.38,
        (10.0, 18.0),
        (8.0, 14.0),
        Some((11.0, 18.0)),
        1.2,
        10.0,
    ),
    (
        "slab",
        0.12,
        (14.0, 24.0),
        (8.0, 14.0),
        Some((5.0, 8.0)),
        5.0,
        8.0,
    ),
    (
        "odd",
        0.06,
        (6.0, 10.0),
        (6.0, 10.0),
        Some((6.0, 11.0)),
        2.0,
        14.0,
    ),
    ("monolith", 0.06, (8.0, 16.0), (7.0, 12.0), None, 1.2, 3.0),
];
const BLOCK_ASPECT: [(f64, f64); 5] = [(1.6, 2.4), (1.4, 2.2), (0.6, 1.0), (1.0, 1.8), (3.0, 4.0)];

const STAGGER_MM: f64 = 2.0;
const STAGGER_SHARE: f64 = 0.25;
const COURSE_MM: f64 = 3.0;
const CAP_MIN_MM: f64 = 6.0;
const ZONE_SIZE: (f64, f64) = (1.45, 0.6);
const ZONE_SIZE_H_EXP: f64 = 0.5;
const MIN_SIDE_MM: f64 = 3.5;
const MIN_DEFICIT_MM: f64 = 1.0;
const OVERSHOOT_MM: (f64, f64) = (0.0, 2.5);
const TOP_TILT_DEG: f64 = 6.0;
pub const SINK_MM: f64 = 0.8;
/// How much lower than its plane a support is taken to be.
const SUPPORT_MARGIN_MM: f64 = 0.6;
const MIN_AREA_MM2: f64 = 8.0;
const THIN_AREA_MM2: f64 = 20.0;
const CRUMB_RETRY: i32 = 3;
const CRUMB_RETRY_MM: f64 = 6.0;
const SNAP_MM: f64 = 3.5;
const JUT_P: f64 = 0.5;
const JUT_MM: (f64, f64) = (1.5, 5.0);
const JUT_CLEAR_MM: f64 = 6.0;
const STONE_H_SHARE: (f64, f64) = (0.32, 0.55);
const TALL_P: f64 = 0.02;
const WIDTH_APART: f64 = 0.25;
const SIZE_LOW_GAIN: f64 = 0.5;
const ROUND_YAW_DEG: f64 = 8.0;
const ROUND_SMOOTH_MM: f64 = 3.0;

/// A resting rock: footprint cells, underside, top plane.
#[derive(Clone)]
pub struct Placed {
    pub cells: Mask,
    pub z0: f64,
    /// z = c + a x + b y
    pub top: (f64, f64, f64),
    pub d: f64,
    /// The support under each footprint cell (NaN off it).
    pub bed: Field,
}

impl Placed {
    pub fn top_z(&self, x: f64, y: f64) -> f64 {
        self.top.0 + self.top.1 * x + self.top.2 * y
    }
    pub fn poly(&self) -> Vec<P2> {
        footprint_polygon(&self.cells)
    }
}

const FOOTPRINT_SIMPLIFY_MM: f64 = 0.45;
const MIN_WALL_MM: f64 = 1.2;

/// The footprint's outline, straightened.
pub fn footprint_polygon(cells: &Mask) -> Vec<P2> {
    // open away arms narrower than a wall (they would stand as fins)
    let n = cells.v.iter().filter(|&&b| b).count();
    let mut m = cells.clone();
    let r = (0.5 * MIN_WALL_MM / GRID_MM).round() as usize;
    if r > 0 && n > 0 {
        let inv = G {
            nx: m.nx,
            ny: m.ny,
            v: m.v.iter().map(|&b| !b).collect(),
        };
        let grown = bin_dilate_sq(&inv, r);
        let eroded = G {
            nx: m.nx,
            ny: m.ny,
            v: grown.v.iter().map(|&b| !b).collect(),
        };
        let opened = bin_dilate_sq(&eroded, r);
        let k = opened.v.iter().filter(|&&b| b).count();
        // the opening can only remove cells of the footprint
        let opened = G {
            nx: m.nx,
            ny: m.ny,
            v: opened
                .v
                .iter()
                .zip(&cells.v)
                .map(|(&a, &b)| a && b)
                .collect(),
        };
        if k as f64 > 0.5 * n as f64 {
            m = opened;
        }
    }
    let ring = trace_outline(&m, GRID_MM);
    let s = super::util::simplify_ring(&ring, FOOTPRINT_SIMPLIFY_MM);
    super::util::ccw(s)
}

fn bin_dilate_sq(m: &Mask, r: usize) -> Mask {
    // square structuring element: separable max
    let mut a = m.clone();
    for j in 0..m.ny {
        for i in 0..m.nx {
            let lo = i.saturating_sub(r);
            let hi = (i + r).min(m.nx - 1);
            a.set(i, j, (lo..=hi).any(|k| m.at(k, j)));
        }
    }
    let mut b = a.clone();
    for j in 0..m.ny {
        for i in 0..m.nx {
            let lo = j.saturating_sub(r);
            let hi = (j + r).min(m.ny - 1);
            b.set(i, j, (lo..=hi).any(|k| a.at(i, k)));
        }
    }
    b
}

pub fn sunk(sv_min: f64) -> f64 {
    sv_min - if sv_min > FOOT { SINK_MM } else { 0.0 }
}

fn cells_min_side(cells: &Mask) -> f64 {
    if !cells.v.iter().any(|&b| b) {
        return 0.0;
    }
    min_side(&trace_outline(cells, GRID_MM))
}

fn count(m: &Mask) -> usize {
    m.v.iter().filter(|&&b| b).count()
}

/// Rectangle grown from the site one row at a time, only into `ok`.
fn grow(rng: &mut Rng, ok: &Mask, ij: (usize, usize), w: f64, d: f64) -> Mask {
    let (nx, ny) = (ok.nx, ok.ny);
    let mut bd = [ij.0 as i64, ij.0 as i64 + 1, ij.1 as i64, ij.1 as i64 + 1];
    let goal = [w / GRID_MM, d / GRID_MM];
    let snap = SNAP_MM / GRID_MM;
    let free = |bd: &[i64; 4], side: usize, k: i64| -> bool {
        let [i0, i1, j0, j1] = *bd;
        match side {
            0 => i0 - 1 - k >= 0 && (j0..j1).all(|j| ok.at((i0 - 1 - k) as usize, j as usize)),
            1 => i1 + k < nx as i64 && (j0..j1).all(|j| ok.at((i1 + k) as usize, j as usize)),
            2 => j0 - 1 - k >= 0 && (i0..i1).all(|i| ok.at(i as usize, (j0 - 1 - k) as usize)),
            _ => j1 + k < ny as i64 && (i0..i1).all(|i| ok.at(i as usize, (j1 + k) as usize)),
        }
    };
    for limit in 0..2 {
        for side in rng.perm(4) {
            while free(&bd, side, 0) {
                if limit == 1 {
                    let mut k = 0;
                    while (k as f64) <= snap && free(&bd, side, k) {
                        k += 1;
                    }
                    if k as f64 > snap {
                        break;
                    }
                } else if ((bd[2 * (side / 2) + 1] - bd[2 * (side / 2)]) as f64) >= goal[side / 2] {
                    break;
                }
                bd[side] += if side % 2 == 1 { 1 } else { -1 };
            }
        }
    }
    let mut cells = Mask::new(nx, ny, false);
    for j in bd[2]..bd[3] {
        for i in bd[0]..bd[1] {
            cells.set(i as usize, j as usize, true);
        }
    }
    cells
}

#[allow(clippy::too_many_arguments)]
fn grow_turned(
    ok: &Mask,
    site: (f64, f64),
    ij: (usize, usize),
    w: f64,
    d: f64,
    yaw: f64,
    e: f64,
) -> Mask {
    let (c, s) = (libm::cos(yaw), libm::sin(yaw));
    let o = e.min(0.5 * d);
    let mut rect = Mask::new(ok.nx, ok.ny, false);
    for j in 0..ok.ny {
        for i in 0..ok.nx {
            if !ok.at(i, j) {
                continue;
            }
            let (x, y) = (
                (i as f64 + 0.5) * GRID_MM - site.0,
                (j as f64 + 0.5) * GRID_MM - site.1,
            );
            let u = x * c + y * s;
            let v = -x * s + y * c;
            if u <= o + GRID_MM && u >= o - d && v.abs() <= 0.5 * w {
                rect.set(i, j, true);
            }
        }
    }
    let (labels, _) = label(&rect);
    let l = labels.at(ij.0, ij.1);
    if l == 0 {
        return Mask::new(ok.nx, ok.ny, false);
    }
    labels.map(|x| x == l)
}

fn jut(rng: &mut Rng, cells: &Mask, want: &Mask, s: &Field) -> Mask {
    let (nx, ny) = (cells.nx, cells.ny);
    let (mut i0, mut i1, mut j0, mut j1) = (usize::MAX, 0, usize::MAX, 0);
    for j in 0..ny {
        for i in 0..nx {
            if cells.at(i, j) {
                i0 = i0.min(i);
                i1 = i1.max(i + 1);
                j0 = j0.min(j);
                j1 = j1.max(j + 1);
            }
        }
    }
    if i0 == usize::MAX {
        return cells.clone();
    }
    let k = (rng.uniform(JUT_MM.0, JUT_MM.1) / GRID_MM).round() as usize;
    let c = k + (JUT_CLEAR_MM / GRID_MM).round() as usize;
    for side in rng.perm(4) {
        // (slice of the jut, slice of the clear ground beyond), as i/j ranges
        let (sl, cl) = match side {
            0 if j0 >= c => ((i0..i1, j0 - k..j0), (i0..i1, j0 - c..j0)),
            1 if j1 + c <= ny => ((i0..i1, j1..j1 + k), (i0..i1, j1..j1 + c)),
            2 if i0 >= c => ((i0 - k..i0, j0..j1), (i0 - c..i0, j0..j1)),
            3 if i1 + c <= nx => ((i1..i1 + k, j0..j1), (i1..i1 + c, j0..j1)),
            _ => continue,
        };
        let mut blocked = false;
        for j in cl.1.clone() {
            for i in cl.0.clone() {
                if want.at(i, j) || s.at(i, j) > FOOT {
                    blocked = true;
                }
            }
        }
        if blocked {
            continue;
        }
        let mut out = cells.clone();
        for j in sl.1.clone() {
            for i in sl.0.clone() {
                out.set(i, j, true);
            }
        }
        return out;
    }
    cells.clone()
}

/// Least squares plane fit z = c0 + c1 (x - xm) + c2 (y - ym).
fn fit_plane(pts: &[(f64, f64, f64)], xm: f64, ym: f64) -> [f64; 3] {
    if pts.len() <= 3 {
        return [0.0; 3];
    }
    let mut m = [[0.0; 4]; 3];
    for &(x, y, z) in pts {
        let a = [1.0, x - xm, y - ym];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += a[r] * a[c];
            }
            m[r][3] += a[r] * z;
        }
    }
    // gaussian elimination with partial pivoting
    for col in 0..3 {
        let p = (col..3)
            .max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))
            .unwrap_or(col);
        m.swap(col, p);
        if m[col][col].abs() < 1e-12 {
            return [
                pts.iter().map(|p| p.2).sum::<f64>() / pts.len() as f64,
                0.0,
                0.0,
            ];
        }
        for r in 0..3 {
            if r != col {
                let f = m[r][col] / m[col][col];
                for c in col..4 {
                    m[r][c] -= f * m[col][c];
                }
            }
        }
    }
    [m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]]
}

fn merge_crumb(
    rocks: &mut [Placed],
    cells: &Mask,
    s: &mut Field,
    ztop: Option<f64>,
    min_h: f64,
) -> bool {
    let ring = {
        let d = bin_dilate(cells, 2);
        G {
            nx: d.nx,
            ny: d.ny,
            v: d.v.iter().zip(&cells.v).map(|(&a, &b)| a && !b).collect(),
        }
    };
    let idx: Vec<(usize, usize)> = (0..cells.ny)
        .flat_map(|j| (0..cells.nx).map(move |i| (i, j)))
        .filter(|&(i, j)| cells.at(i, j))
        .collect();
    let smax = idx
        .iter()
        .map(|&(i, j)| s.at(i, j))
        .fold(f64::NEG_INFINITY, f64::max);
    let smean = idx.iter().map(|&(i, j)| s.at(i, j)).sum::<f64>() / idx.len().max(1) as f64;
    let mut best: Option<(usize, f64)> = None;
    for (k, r) in rocks.iter().enumerate().rev() {
        let touches = idx.iter().any(|_| false)
            || (0..cells.ny).any(|j| (0..cells.nx).any(|i| ring.at(i, j) && r.cells.at(i, j)));
        if !touches {
            continue;
        }
        let zs: Vec<f64> = idx
            .iter()
            .map(|&(i, j)| {
                let z = r.top_z(super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM));
                ztop.map_or(z, |t| z.min(t))
            })
            .collect();
        let zmin = zs.iter().copied().fold(f64::INFINITY, f64::min);
        let zmean = zs.iter().sum::<f64>() / zs.len() as f64;
        let bz = best.map_or(f64::INFINITY, |b| b.1);
        if zmin > smax + min_h && (zmean - smean).abs() < bz {
            best = Some((k, (zmean - smean).abs()));
        }
    }
    let Some((k, _)) = best else { return false };
    let r = &mut rocks[k];
    let svmin = idx
        .iter()
        .map(|&(i, j)| s.at(i, j))
        .fold(f64::INFINITY, f64::min);
    for &(i, j) in &idx {
        r.cells.set(i, j, true);
        r.bed.set(i, j, s.at(i, j));
        let z = r.top_z(super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM));
        let z = ztop.map_or(z, |t| z.min(t));
        s.set(i, j, s.at(i, j).max(z));
    }
    r.z0 = r.z0.min(sunk(svmin));
    true
}

pub struct Stacked {
    pub rocks: Vec<Placed>,
    pub s: Field,
}

#[allow(clippy::too_many_lines)]
pub fn stack(rng: &mut Rng, form: &Form, zone: &Zones, s_scale: f64, max_rocks: usize) -> Stacked {
    let (nx, ny) = (form.t.nx, form.t.ny);
    let t = &form.t;
    let mut s = Field::new(nx, ny, FOOT - 1.0);
    let mut want = G {
        nx,
        ny,
        v: t.v.iter().map(|&z| z > FOOT).collect(),
    };
    let mut rocks: Vec<Placed> = Vec::new();
    let mut probs: Vec<f64> = (0..5)
        .map(|k| form.kind_p.map_or(KINDS[k].1, |p| p[k]))
        .collect();
    let ps: f64 = probs.iter().sum();
    probs.iter_mut().for_each(|p| *p /= ps);
    let mut noise = Field::new(nx, ny, 0.0);
    for v in &mut noise.v {
        *v = rng.random() * 1.5;
    }
    let mut retry = G::new(nx, ny, 0i32);
    let h_cliff = t.v.iter().copied().fold(f64::NEG_INFINITY, f64::max) - FOOT;
    let mut last_w = 0.0f64;
    let (edt, phi) = if form.round {
        let e = edt_in(&want);
        let e = Field {
            nx,
            ny,
            v: e.v.iter().map(|x| x * GRID_MM).collect(),
        };
        let sm = blur(&e, ROUND_SMOOTH_MM / GRID_MM);
        let mut phi = Field::new(nx, ny, 0.0);
        for j in 0..ny {
            for i in 0..nx {
                let g = |a: usize, b: usize| sm.at(a, b);
                let gx = (g((i + 1).min(nx - 1), j) - g(i.saturating_sub(1), j))
                    / (((i + 1).min(nx - 1) - i.saturating_sub(1)) as f64 * GRID_MM).max(1e-9);
                let gy = (g(i, (j + 1).min(ny - 1)) - g(i, j.saturating_sub(1)))
                    / (((j + 1).min(ny - 1) - j.saturating_sub(1)) as f64 * GRID_MM).max(1e-9);
                phi.set(i, j, libm::atan2(-gy, -gx));
            }
        }
        (Some(e), Some(phi))
    } else {
        (None, None)
    };
    for _ in 0..max_rocks {
        // the lowest spot still short of T
        let mut site: Option<(usize, usize)> = None;
        let mut key = f64::INFINITY;
        for j in 0..ny {
            for i in 0..nx {
                let q = j * nx + i;
                if want.v[q] && t.v[q] - s.v[q] > MIN_DEFICIT_MM {
                    let k = s.v[q] + noise.v[q];
                    if k < key {
                        key = k;
                        site = Some((i, j));
                    }
                }
            }
        }
        let Some((si, sj)) = site else { break };
        let sp = [super::util::cx(si, GRID_MM), super::util::cx(sj, GRID_MM)];
        let mut kind = rng.pick(&probs);
        let is_top = form.ztop.is_some_and(|zt| t.at(si, sj) >= zt - 1e-6);
        if kind == 4 && !(is_top && s.at(si, sj) <= FOOT) {
            kind = 0;
        }
        let (_, _, al, dp, hr, bridge, tilt) = KINDS[kind];
        let lo = 1.0
            + SIZE_LOW_GAIN * (0.5 - ((s.at(si, sj) - FOOT) / h_cliff.max(1.0)).clamp(0.0, 1.0));
        let mut w = rng.uniform(al.0, al.1) * s_scale * form.mass * lo;
        let mut d = rng.uniform(dp.0, dp.1) * s_scale * form.mass * lo;
        let u = (zone.at(sp[0]) - ZONE_FLOOR) / (1.0 - ZONE_FLOOR);
        let sz = lerp(ZONE_SIZE, u);
        w *= sz;
        if (w / last_w.max(1e-6) - 1.0).abs() < WIDTH_APART {
            w *= if rng.random() < 0.5 {
                1.0 - 1.5 * WIDTH_APART
            } else {
                1.0 + 2.0 * WIDTH_APART
            };
        }
        last_w = w;
        if form.along_y.as_ref().is_some_and(|m| m.at(si, sj)) {
            std::mem::swap(&mut w, &mut d);
        }
        let yaw = phi.as_ref().map_or(0.0, |p| {
            p.at(si, sj) + ROUND_YAW_DEG.to_radians() * rng.normal(0.0, 1.0)
        });
        let mut cells = Mask::new(nx, ny, false);
        let (mut area, mut side) = (0.0, 0.0);
        for attempt in 0..4 {
            let mut room = Mask::new(nx, ny, false);
            for q in 0..nx * ny {
                room.v[q] = want.v[q]
                    && s.v[q] <= s.at(si, sj) + bridge + 3.0 * f64::from(attempt)
                    && t.v[q] - s.v[q] > 0.3;
            }
            cells = if let Some(e) = &edt {
                grow_turned(&room, (sp[0], sp[1]), (si, sj), w, d, yaw, e.at(si, sj))
            } else {
                grow(rng, &room, (si, sj), w, d)
            };
            area = count(&cells) as f64 * GRID_MM * GRID_MM;
            side = cells_min_side(&cells);
            if area >= MIN_AREA_MM2 && side >= MIN_SIDE_MM {
                break;
            }
        }
        if area == 0.0 {
            s.set(si, sj, t.at(si, sj));
            continue;
        }
        if (area < MIN_AREA_MM2 || side < MIN_SIDE_MM) && !rocks.is_empty() {
            let mut ok = merge_crumb(&mut rocks, &cells, &mut s, form.ztop, 0.5 * MIN_H_MM);
            if !ok {
                let rmax = (0..nx * ny)
                    .filter(|&q| cells.v[q])
                    .map(|q| retry.v[q])
                    .max()
                    .unwrap_or(0);
                if is_top && rmax < CRUMB_RETRY {
                    for q in 0..nx * ny {
                        if cells.v[q] {
                            retry.v[q] += 1;
                            noise.v[q] += CRUMB_RETRY_MM;
                        }
                    }
                    continue;
                }
            }
            if !ok {
                ok = merge_crumb(&mut rocks, &cells, &mut s, form.ztop, 0.3);
            }
            if !ok && (area < THIN_AREA_MM2 || side < MIN_SIDE_MM * 0.6) {
                for q in 0..nx * ny {
                    if cells.v[q] {
                        want.v[q] = false;
                    }
                }
                continue;
            }
            if ok {
                continue;
            }
        }
        if !form.round {
            let cmax = (0..nx * ny)
                .filter(|&q| cells.v[q])
                .map(|q| s.v[q])
                .fold(f64::NEG_INFINITY, f64::max);
            if cmax <= FOOT && rng.random() < JUT_P {
                cells = jut(rng, &cells, &want, &s);
            }
        }
        // the supports under the footprint
        let mut pts: Vec<(f64, f64, f64)> = Vec::new();
        let mut sv_all = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                if cells.at(i, j) {
                    let z = s.at(i, j);
                    pts.push((super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM), z));
                    sv_all.push(z);
                }
            }
        }
        let xm = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
        let ym = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
        let mut coef = fit_plane(&pts, xm, ym);
        let g = libm::hypot(coef[1], coef[2]);
        let gmax = tilt.to_radians().tan();
        if g > gmax {
            coef[1] *= gmax / g;
            coef[2] *= gmax / g;
        }
        let t2 = [
            TOP_TILT_DEG.to_radians() * rng.normal(0.0, 1.0) * 0.6,
            TOP_TILT_DEG.to_radians() * rng.normal(0.0, 1.0) * 0.6,
        ];
        let (a, b) = (coef[1] + t2[0], coef[2] + t2[1]);
        let c = pts
            .iter()
            .map(|p| p.2 - a * (p.0 - xm) - b * (p.1 - ym))
            .fold(f64::NEG_INFINITY, f64::max);
        let (sx, sy) = (super::util::cx(si, GRID_MM), super::util::cx(sj, GRID_MM));
        let tgt = t.at(si, sj) - (c + a * (sx - xm) + b * (sy - ym));
        let lim = if is_top { CAP_MIN_MM } else { MIN_H_MM };
        let finish = |rng: &mut Rng, h: f64| -> f64 {
            let mut h = h;
            if tgt - h < lim {
                h = if is_top {
                    tgt + 1.0
                } else {
                    tgt + rng.uniform(OVERSHOOT_MM.0, OVERSHOOT_MM.1)
                };
            }
            h.max(MIN_H_MM)
        };
        let mut h = match hr {
            None => finish(rng, tgt + 1.0),
            Some(hr) => {
                let v =
                    rng.uniform(hr.0, hr.1) * s_scale * (sz * form.mass * lo).powf(ZONE_SIZE_H_EXP);
                finish(rng, v)
            }
        };
        let mut hc = (rng.uniform(BLOCK_ASPECT[kind].0, BLOCK_ASPECT[kind].1)
            * cells_min_side(&cells))
        .min(tgt - lim);
        if kind != 4 && rng.random() > TALL_P {
            hc = hc.min(rng.uniform(STONE_H_SHARE.0, STONE_H_SHARE.1) * h_cliff);
        }
        if MIN_H_MM <= hc && hc < h {
            h = hc;
        }
        // stagger: no top level with its neighbours' (that is how courses appear)
        let dil2 = bin_dilate(&cells, 2);
        let ring: Vec<usize> = (0..nx * ny)
            .filter(|&q| dil2.v[q] && !cells.v[q] && s.v[q] > FOOT + 0.5)
            .collect();
        let dilc = bin_dilate(&cells, (COURSE_MM / GRID_MM) as usize);
        let near: Vec<bool> = (0..nx * ny).map(|q| dilc.v[q] && !cells.v[q]).collect();
        let mut joints: Vec<Vec<(usize, f64)>> = Vec::new();
        for r in &rocks {
            let q: Vec<usize> = (0..nx * ny).filter(|&q| r.cells.v[q] && near[q]).collect();
            if q.len() >= 4 {
                let zr: Vec<(usize, f64)> = q
                    .iter()
                    .map(|&k| {
                        (
                            k,
                            r.top_z(
                                super::util::cx(k % nx, GRID_MM),
                                super::util::cx(k / nx, GRID_MM),
                            ),
                        )
                    })
                    .collect();
                let mean = zr.iter().map(|z| z.1).sum::<f64>() / zr.len() as f64;
                if form.ztop.is_none_or(|zt| mean < zt - 0.5) {
                    joints.push(zr);
                }
            }
        }
        let top_at = |hh: f64, k: usize| {
            c + hh
                + a * (super::util::cx(k % nx, GRID_MM) - xm)
                + b * (super::util::cx(k / nx, GRID_MM) - ym)
        };
        let level = |hh: f64| -> bool {
            if !ring.is_empty() {
                let near_n = ring
                    .iter()
                    .filter(|&&k| (top_at(hh, k) - s.v[k]).abs() < STAGGER_MM)
                    .count();
                if near_n as f64 / ring.len() as f64 > STAGGER_SHARE {
                    return true;
                }
            }
            joints.iter().any(|zr| {
                zr.iter()
                    .filter(|(k, z)| (top_at(hh, *k) - z).abs() < STAGGER_MM)
                    .count() as f64
                    / zr.len() as f64
                    > 0.5
            })
        };
        for _ in 0..6 {
            if tgt - h < lim || !level(h) {
                break;
            }
            let dv = rng.signed((STAGGER_MM, 2.5 * STAGGER_MM));
            h = finish(rng, h + dv);
        }
        let c_top = c + h - a * xm - b * ym;
        let mut bed = Field::new(nx, ny, f64::NAN);
        let mut svmin = f64::INFINITY;
        for j in 0..ny {
            for i in 0..nx {
                if !cells.at(i, j) {
                    continue;
                }
                let (x, y) = (super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM));
                let mut topz = c_top + a * x + b * y;
                if let Some(zt) = form.ztop {
                    topz = topz.min(zt);
                }
                bed.set(i, j, s.at(i, j));
                svmin = svmin.min(s.at(i, j));
                s.set(i, j, s.at(i, j).max(topz));
            }
        }
        let mut dims = [w, d, h];
        dims.sort_by(f64::total_cmp);
        rocks.push(Placed {
            cells,
            z0: sunk(svmin),
            top: (c_top, a, b),
            d: dims[1],
            bed,
        });
    }
    Stacked { rocks, s }
}

/// The stack claims support over every cell it gave a stone, but a
/// stone's finished outline (opened, simplified, largest piece only) can
/// leave some of those cells bare: a merged crumb, a thin arm. A stone
/// resting on such a claim would hang over air. So the support is rebuilt
/// from the outlines themselves, in placement order: each stone's cells
/// are what its outline covers, its bed is what is really under them, and
/// the support grid rises by its top only there.
pub fn reconcile(rocks: &mut [Placed], form: &Form, s: &mut Field) {
    let (nx, ny) = (s.nx, s.ny);
    s.v.fill(FOOT - 1.0);
    for r in rocks.iter_mut() {
        let poly = r.poly();
        if poly.len() < 3 {
            continue;
        }
        let b = super::util::bounds(&poly);
        let (i0, i1) = (
            ((b[0] / GRID_MM).floor().max(0.0)) as usize,
            ((b[2] / GRID_MM).ceil() as usize).min(nx),
        );
        let (j0, j1) = (
            ((b[1] / GRID_MM).floor().max(0.0)) as usize,
            ((b[3] / GRID_MM).ceil() as usize).min(ny),
        );
        let mut cells = Mask::new(nx, ny, false);
        for j in j0..j1 {
            for i in i0..i1 {
                if r.cells.at(i, j)
                    && super::util::point_in(
                        &poly,
                        [super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM)],
                    )
                {
                    cells.set(i, j, true);
                }
            }
        }
        if !cells.v.iter().any(|&c| c) {
            continue;
        }
        let mut bed = Field::new(nx, ny, f64::NAN);
        let mut svmin = f64::INFINITY;
        for j in j0..j1 {
            for i in i0..i1 {
                if !cells.at(i, j) {
                    continue;
                }
                let (x, y) = (super::util::cx(i, GRID_MM), super::util::cx(j, GRID_MM));
                let mut topz = r.top_z(x, y);
                if let Some(zt) = form.ztop {
                    topz = topz.min(zt);
                }
                // the support's real top sits a little under its plane (grain, chips): a
                // stone must still reach into it, however narrow
                let under = if s.at(i, j) > FOOT {
                    s.at(i, j) - SUPPORT_MARGIN_MM
                } else {
                    s.at(i, j)
                };
                bed.set(i, j, under);
                svmin = svmin.min(under);
                s.set(i, j, s.at(i, j).max(topz));
            }
        }
        r.cells = cells;
        r.bed = bed;
        r.z0 = sunk(svmin);
    }
}
