//! 3 Finish, the column planning (rockpile.py :832-1080): stones sharing a
//! footprint stand in one column, cleaved by whole planes planned once on
//! its convex hull; open faces dressed off true.

use dt_kit::{Piece, Rng, Slot, Support};

use super::faces::{Column, Cut, OpenFace};
use super::form::{Form, GRID_MM, Zones};
use super::stack::Placed;
use super::util::{Field, P2, area, bounds, centroid, hull2, norm2};

pub const COLUMN_SHARE: f64 = 0.6;
const STONE_STEP_MM: (f64, f64) = (0.2, 1.0);
const CORNER_MM: (f64, f64) = (1.4, 2.2);
const RIM_SETBACK_MM: (f64, f64) = (0.0, 0.6);
pub const COLUMN_STEP_MAX: f64 = 0.3;
const DRESS_P: f64 = 0.85;
const DRESS_YAW_DEG: (f64, f64) = (3.0, 12.0);
const DRESS_BATTER_DEG: (f64, f64) = (-3.0, 10.0);
const DRESS_MAX_MM: f64 = 2.5;
const CLEAVE_MAX_LOSS: f64 = 0.15;
const CLEAVE_MIN_KEEP: f64 = 0.6;
const CLEAVE_MIN_MM3: f64 = 0.02;
const CLEAVE_SUPPORT_MM3: f64 = 1.0;
pub const SEAT_SLOPE_DEG: f64 = 90.0 - 55.0 + 3.0;
const Z_UP: [f64; 3] = [0.0, 0.0, 1.0];

pub fn top_z_poly(r: &Placed, poly: &[P2]) -> f64 {
    poly.iter()
        .map(|p| r.top_z(p[0], p[1]))
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Is each point inside the finished pile (or below the ground)?
pub fn occupied(form: &Form, s: &Field, p: [f64; 3]) -> bool {
    let i = (p[0] / GRID_MM).floor() as i64;
    let j = (p[1] / GRID_MM).floor() as i64;
    let mut out = false;
    if i >= 0 && j >= 0 && (i as usize) < s.nx && (j as usize) < s.ny {
        out = p[2] < s.at(i as usize, j as usize) - 0.05;
    }
    if let Some(zt) = form.ztop {
        out |= p[2] > zt - 0.1;
    }
    out
}

/// The footprint's sides that look into the open or onto the boundary.
pub fn open_faces(poly: &[P2], form: &Form, s: &Field, zlo: f64, zhi: f64) -> Vec<OpenFace> {
    let zs = [0.2, 0.5, 0.8].map(|f| zlo + (zhi - zlo) * f);
    let n = poly.len();
    let mut out = Vec::new();
    for k in 0..n {
        let (p0, p1) = (poly[k], poly[(k + 1) % n]);
        let e = [p1[0] - p0[0], p1[1] - p0[1]];
        let l = norm2(e);
        if l < 2.0 {
            continue;
        }
        let m2 = [e[1] / l, -e[0] / l];
        let mid = [
            0.5 * (p0[0] + p1[0]) + 0.5 * m2[0],
            0.5 * (p0[1] + p1[1]) + 0.5 * m2[1],
        ];
        let rim = !(0.0 < mid[0] && mid[0] < form.w && 0.0 < mid[1] && mid[1] < form.h);
        let occ = zs
            .iter()
            .filter(|&&z| occupied(form, s, [mid[0], mid[1], z]))
            .count();
        if rim || occ <= 1 {
            out.push(OpenFace {
                p0,
                p1,
                m: [m2[0], m2[1], 0.0],
                rim,
            });
        }
    }
    out
}

pub fn columns(
    rng: &mut Rng,
    placed: &[Placed],
    form: &Form,
    s: &Field,
    zone: &Zones,
) -> Vec<Column> {
    let n = placed.len();
    let polys: Vec<Vec<P2>> = placed.iter().map(Placed::poly).collect();
    let bb: Vec<[f64; 4]> = polys.iter().map(|p| bounds(p)).collect();
    let a: Vec<usize> = placed
        .iter()
        .map(|p| p.cells.v.iter().filter(|&&b| b).count())
        .collect();
    let mut par: Vec<usize> = (0..n).collect();
    fn find(par: &mut [usize], mut i: usize) -> usize {
        while par[i] != i {
            par[i] = par[par[i]];
            i = par[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            if bb[i][0] < bb[j][2]
                && bb[j][0] < bb[i][2]
                && bb[i][1] < bb[j][3]
                && bb[j][1] < bb[i][3]
            {
                let ov = (0..placed[i].cells.v.len())
                    .filter(|&q| placed[i].cells.v[q] && placed[j].cells.v[q])
                    .count();
                if ov as f64 >= COLUMN_SHARE * a[i].max(a[j]) as f64 {
                    let (x, y) = (find(&mut par, i), find(&mut par, j));
                    par[x] = y;
                }
            }
        }
    }
    let lab: Vec<usize> = (0..n).map(|i| find(&mut par, i)).collect();
    let mut order: Vec<usize> = Vec::new();
    for &l in &lab {
        if !order.contains(&l) {
            order.push(l);
        }
    }
    let mut cols = Vec::new();
    for l in order {
        let mut g: Vec<usize> = (0..n).filter(|&i| lab[i] == l).collect();
        g.sort_by(|&x, &y| placed[x].z0.total_cmp(&placed[y].z0));
        let all: Vec<P2> = g.iter().flat_map(|&i| polys[i].iter().copied()).collect();
        let poly = hull2(&all);
        let best = g
            .iter()
            .copied()
            .max_by(|&x, &y| {
                top_z_poly(&placed[x], &polys[x]).total_cmp(&top_z_poly(&placed[y], &polys[y]))
            })
            .unwrap_or(g[0]);
        let mut zhi = top_z_poly(&placed[best], &poly);
        if let Some(zt) = form.ztop {
            zhi = zhi.min(zt);
        }
        let zlo = g
            .iter()
            .map(|&i| placed[i].z0)
            .fold(f64::INFINITY, f64::min);
        let mut acc = 0.0;
        let back: Vec<(usize, f64)> = g
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                if k > 0 {
                    acc += rng.uniform(STONE_STEP_MM.0, STONE_STEP_MM.1);
                }
                (i, acc)
            })
            .collect();
        let c = centroid(&poly);
        let corner_mm = rng.uniform(CORNER_MM.0, CORNER_MM.1);
        let setback: Vec<((usize, bool), f64)> = form
            .rims
            .iter()
            .map(|&r| (r, rng.uniform(RIM_SETBACK_MM.0, RIM_SETBACK_MM.1)))
            .collect();
        let u = rng.random();
        let step = if u < 0.5 {
            u / 0.5 * 0.6
        } else {
            2.5 + (u - 0.5) / 0.5 * 2.0
        };
        let open = open_faces(&poly, form, s, zlo, zhi);
        cols.push(Column {
            stones: g,
            poly,
            zlo,
            zhi,
            zone: zone.at(c[0]),
            open,
            cuts: Vec::new(),
            setback,
            corner_mm,
            step,
            back,
            carries: false,
            faces: Vec::new(),
            reach: Vec::new(),
        });
    }
    cols
}

/// Extrude `poly` from z0 up h, cut to the top plane and the form's top.
pub fn prism(
    slot: Slot,
    poly: &[P2],
    z0: f64,
    h: f64,
    top: (f64, f64, f64),
    form: &Form,
) -> Result<Piece, dt_kit::KitError> {
    let (c, a, b) = top;
    let mut p = Piece::prism(slot, poly, z0, z0 + h)?;
    let na = libm::sqrt(a * a + b * b + 1.0);
    p = p.keep([-a / na, -b / na, 1.0 / na], c / na)?;
    if let Some(zt) = form.ztop {
        p = p.keep(Z_UP, zt)?;
    }
    Ok(p)
}

/// A solid cut plane by plane: each half-space `n . x >= off` taken unless
/// it would take it all, undercut `support`, or (`strict`) take more than
/// CLEAVE_MAX_LOSS or leave less than CLEAVE_MIN_KEEP.
pub struct Cutter<'a> {
    pub solid: Piece,
    vol0: f64,
    support: Option<&'a [&'a Support]>,
    pub cuts: Vec<Cut>,
}

impl<'a> Cutter<'a> {
    pub fn new(solid: Piece, support: Option<&'a [&'a Support]>) -> Self {
        let vol0 = solid.volume();
        Self {
            solid,
            vol0,
            support,
            cuts: Vec::new(),
        }
    }

    pub fn take(&mut self, n: [f64; 3], off: f64, d: Option<[f64; 3]>, strict: bool) -> bool {
        let Ok(cut) = self.solid.keep(n, off) else {
            return false;
        };
        if cut.is_empty() {
            return false;
        }
        let (v, v0) = (cut.volume(), self.solid.volume());
        if v0 - v < CLEAVE_MIN_MM3 {
            return true;
        }
        if strict && (v < (1.0 - CLEAVE_MAX_LOSS) * v0 || v < CLEAVE_MIN_KEEP * self.vol0) {
            return false;
        }
        if let Some(sup) = self.support {
            let u: f64 = sup.iter().map(|s| self.solid.undercut(&cut, s)).sum();
            if u > CLEAVE_SUPPORT_MM3 {
                return false;
            }
        }
        self.solid = cut;
        self.cuts.push(Cut {
            n,
            off,
            d: d.unwrap_or([0.0; 3]),
        });
        true
    }

    /// The plane normal to `n` through `p`, moved out (same angle) to take
    /// no deeper than `lim + extra` from the solid's reach along `n`.
    pub fn take_back(
        &mut self,
        n: [f64; 3],
        p: [f64; 3],
        lim: f64,
        extra: f64,
        d: Option<[f64; 3]>,
    ) -> bool {
        let reach = self
            .solid
            .mesh()
            .positions
            .iter()
            .map(|v| v[0] * n[0] + v[1] * n[1] + v[2] * n[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let np = n[0] * p[0] + n[1] * p[1] + n[2] * p[2];
        let depth = reach - np;
        if depth >= 0.3 {
            let k = libm::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
            let un = n.map(|x| x / k);
            let back = (depth - lim - extra).max(0.0);
            let pp = [p[0] + n[0] * back, p[1] + n[1] * back, p[2] + n[2] * back];
            let off = (n[0] * pp[0] + n[1] * pp[1] + n[2] * pp[2]) / k;
            return self.take(un, off, d, true);
        }
        false
    }
}

/// Plan the column's planes on its hull: faces stood back and dressed off
/// true (turned a few degrees in plan and battered back), cut while still
/// convex. The planes kept go in `c.cuts` for every stone of the column.
pub fn plan_column(
    rng: &mut Rng,
    c: &mut Column,
    placed: &[Placed],
    form: &Form,
    slot: Slot,
) -> Result<(), dt_kit::KitError> {
    let hgt = c.zhi - c.zlo;
    if hgt < 2.0 {
        return Ok(());
    }
    let best = c
        .stones
        .iter()
        .copied()
        .max_by(|&x, &y| {
            top_z_poly(&placed[x], &placed[x].poly())
                .total_cmp(&top_z_poly(&placed[y], &placed[y].poly()))
        })
        .unwrap_or(c.stones[0]);
    let hull = prism(slot, &c.poly, c.zlo, hgt + 1.0, placed[best].top, form)?;
    if hull.is_empty() {
        return Ok(());
    }
    c.reach = hull.mesh().positions;
    let step = c.step;
    c.step = 0.0;
    if form.round {
        c.back.iter_mut().for_each(|b| b.1 = 0.0);
    }
    if c.carries {
        return Ok(());
    }
    let mut hull = Cutter::new(hull, None);
    let ptp = |m: [f64; 3], reach: &[[f64; 3]]| {
        let v: Vec<f64> = reach
            .iter()
            .map(|r| r[0] * m[0] + r[1] * m[1] + r[2] * m[2])
            .collect();
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    for of in c.open.clone() {
        let (mut p0, mut p1, m, rim) = (of.p0, of.p1, of.m, of.rim);
        let l = norm2([p1[0] - p0[0], p1[1] - p0[1]]);
        if rim {
            continue;
        }
        let sb = if form.round {
            0.0
        } else {
            step.min(COLUMN_STEP_MAX * (ptp(m, &c.reach) - 4.0))
                .max(0.0)
        };
        let p = [
            p0[0] - 10.0 * m[0],
            p0[1] - 10.0 * m[1],
            c.zlo - 10.0 * m[2],
        ];
        if hull.take_back(m, p, 0.0, sb, Some(m)) {
            c.step = c.step.max(sb);
        }
        p0 = [p0[0] - sb * m[0], p0[1] - sb * m[1]];
        p1 = [p1[0] - sb * m[0], p1[1] - sb * m[1]];
        if l < 3.0 || rng.random() > DRESS_P {
            continue;
        }
        let (piv, far) = if rng.random() < 0.5 {
            (p0, p1)
        } else {
            (p1, p0)
        };
        let t = [(far[0] - piv[0]) / l, (far[1] - piv[1]) / l, 0.0];
        let psi = rng
            .uniform(DRESS_YAW_DEG.0, DRESS_YAW_DEG.1)
            .min((DRESS_MAX_MM / l).min(1.0).asin().to_degrees())
            .to_radians();
        let nh = [
            m[0] * libm::cos(psi) + t[0] * libm::sin(psi),
            m[1] * libm::cos(psi) + t[1] * libm::sin(psi),
            0.0,
        ];
        let be = rng
            .uniform(DRESS_BATTER_DEG.0, DRESS_BATTER_DEG.1)
            .to_radians();
        let n = [nh[0] * libm::cos(be), nh[1] * libm::cos(be), libm::sin(be)];
        let pz = [piv[0], piv[1], c.zlo + hgt * rng.uniform(0.0, 0.3)];
        hull.take_back(n, pz, DRESS_MAX_MM, 0.0, Some(m));
    }
    c.cuts = hull.cuts;
    Ok(())
}

/// Polygon offset by `d` (outward positive), mitred, of a counter-clockwise ring.
pub fn offset_poly(poly: &[P2], d: f64) -> Vec<P2> {
    let n = poly.len();
    let mut lines: Vec<(P2, P2)> = Vec::new(); // point, direction
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let l = norm2(e).max(1e-12);
        let nrm = [e[1] / l, -e[0] / l];
        lines.push(([a[0] + nrm[0] * d, a[1] + nrm[1] * d], [e[0] / l, e[1] / l]));
    }
    (0..n)
        .map(|k| {
            let (p, u) = lines[(k + n - 1) % n];
            let (q, v) = lines[k];
            let den = u[0] * v[1] - u[1] * v[0];
            if den.abs() < 1e-9 {
                q
            } else {
                let t = ((q[0] - p[0]) * v[1] - (q[1] - p[1]) * v[0]) / den;
                [p[0] + t * u[0], p[1] + t * u[1]]
            }
        })
        .collect()
}

/// The wedge under a resting rock: its footprint at its underside,
/// narrowing at the seat slope to a ridge: no flat bottom.
pub fn support_cone(r: &Placed) -> Option<dt_kit::TriMesh> {
    let poly = r.poly();
    if poly.len() < 3 {
        return None;
    }
    let slope = SEAT_SLOPE_DEG.to_radians().tan();
    let b = bounds(&poly);
    let (mut lo, mut hi) = (0.0, 0.5 * (b[2] - b[0]).min(b[3] - b[1]));
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        let inner = offset_poly(&poly, -mid);
        if area(&inner) > 1e-3 && area(&inner) < area(&poly) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let inner = offset_poly(&poly, -lo);
    let mut pts: Vec<[f64; 3]> = poly.iter().map(|p| [p[0], p[1], r.z0]).collect();
    pts.extend(inner.iter().map(|p| [p[0], p[1], r.z0 - slope * lo]));
    dt_kit::form::hull(&pts).ok()
}
