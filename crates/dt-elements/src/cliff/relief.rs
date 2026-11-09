//! The stone (or the core) cut back on its column's open faces, in two
//! passes: first to the sub-faces' planes along a shear (45 degrees below
//! the face's inward normal, so a height field along it leans nowhere past
//! the overhang limit), then the cracks sunk in straight along the normal
//! (thin: their ceilings reach under REACH).

use std::sync::Arc;

use dt_kit::rays::RayMesh;
use dt_kit::vec3::dot;
use dt_kit::{KitError, Piece, TriMesh, hung, tri_normal};

use super::faces::{Face, RELIEF_GRID_MM};
use super::util::{Cells, Field, Z_UP, cell_grid, edt_in};
use dt_kit::smooth01;

const CRACK_GRID_MM: f64 = 0.1;
const SHEAR_DEG: f64 = 45.0;
const CRACK_WALL_MM: f64 = 1.6;
const FACET_END_MM: f64 = 1.5;
const RELIEF_OFF_MM: f64 = 4.0;
const CUT_BLUR_MM: (f64, f64) = (0.08, 0.3);
const CUT_SHORT_MM: f64 = 0.1;

/// Bilinear sample at index coordinates; outside the grid: `cval` when
/// `constant`, else clamped to the edge.
fn sample(f: &Field, x: f64, y: f64, constant: bool) -> f64 {
    let (mx, my) = ((f.nx - 1) as f64, (f.ny - 1) as f64);
    if constant && (x < 0.0 || y < 0.0 || x > mx || y > my) {
        return 0.0;
    }
    let (x, y) = (x.clamp(0.0, mx), y.clamp(0.0, my));
    let (i, j) = (x.floor() as usize, y.floor() as usize);
    let (i1, j1) = ((i + 1).min(f.nx - 1), (j + 1).min(f.ny - 1));
    let (fx, fy) = (x - i as f64, y - j as f64);
    let a = f.at(i, j) * (1.0 - fx) + f.at(i1, j) * fx;
    let b = f.at(i, j1) * (1.0 - fx) + f.at(i1, j1) * fx;
    a * (1.0 - fy) + b * fy
}

fn fill_holes(m: &Cells) -> Cells {
    // flood the background from the border; whatever is not reached is filled
    let mut seen = cell_grid(m.nx, m.ny, false);
    let mut st: Vec<(usize, usize)> = Vec::new();
    for i in 0..m.nx {
        for j in [0, m.ny - 1] {
            if !m.at(i, j) && !seen.at(i, j) {
                seen.set(i, j, true);
                st.push((i, j));
            }
        }
    }
    for j in 0..m.ny {
        for i in [0, m.nx - 1] {
            if !m.at(i, j) && !seen.at(i, j) {
                seen.set(i, j, true);
                st.push((i, j));
            }
        }
    }
    while let Some((i, j)) = st.pop() {
        let mut go = |a: usize, b: usize| {
            if !m.at(a, b) && !seen.at(a, b) {
                seen.set(a, b, true);
                st.push((a, b));
            }
        };
        if i > 0 {
            go(i - 1, j);
        }
        if i + 1 < m.nx {
            go(i + 1, j);
        }
        if j > 0 {
            go(i, j - 1);
        }
        if j + 1 < m.ny {
            go(i, j + 1);
        }
    }
    seen.map(|s| !s)
}

/// Solve the 3-parameter least squares front ~ c0 + c1 U + c2 Z over `k`.
fn fit(us: &[f64], zs: &[f64], k: &[(usize, usize)], front: &Field) -> [f64; 3] {
    let mut m = [[0.0; 4]; 3];
    for &(i, j) in k {
        let a = [1.0, us[i], zs[j]];
        let y = front.at(i, j);
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += a[r] * a[c];
            }
            m[r][3] += a[r] * y;
        }
    }
    for col in 0..3 {
        let p = (col..3)
            .max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))
            .unwrap_or(col);
        m.swap(col, p);
        if m[col][col].abs() < 1e-12 {
            return [0.0; 3];
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

/// `coarse` scales both sampling grids: 1 for a print, more for a draft.
pub fn relief(
    mut rock: Piece,
    faces: &[Arc<Face>],
    core: Option<(f64, f64)>,
    progress: Option<(&dt_kit::Progress, f64, f64)>,
    coarse: f64,
) -> Result<Piece, KitError> {
    let passes: Vec<(f64, &Arc<Face>)> = [SHEAR_DEG.to_radians(), 0.0]
        .into_iter()
        .flat_map(|a| faces.iter().map(move |f| (a, f)))
        .filter(|(a, fc)| !(*a != 0.0 && fc.rim && core.is_some()))
        .collect();
    // the core's carve is one long serial loop: say how far through it is
    let ticks = progress.map(|(p, lo, hi)| p.counter("faces carved", passes.len(), lo, hi));
    for (a, fc) in passes {
        if let Some(t) = &ticks {
            t.tick();
        }
        let g = coarse
            * if a != 0.0 {
                RELIEF_GRID_MM
            } else {
                CRACK_GRID_MM
            };
        let t = rock.mesh();
        if t.positions.is_empty() {
            continue;
        }
        let q: Vec<[f64; 2]> = t
            .positions
            .iter()
            .map(|p| [p[0] - fc.p0[0], p[1] - fc.p0[1]])
            .collect();
        let u: Vec<f64> = q.iter().map(|q| q[0] * fc.t[0] + q[1] * fc.t[1]).collect();
        let s_: Vec<f64> = q.iter().map(|q| q[0] * fc.m[0] + q[1] * fc.m[1]).collect();
        let umax = u.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let umin = u.iter().copied().fold(f64::INFINITY, f64::min);
        if umax.min(fc.l) - umin.max(0.0) < 2.0 {
            continue;
        }
        let (u0, u1) = if core.is_some() {
            (umin.max(0.0), umax.min(fc.l))
        } else {
            (umin, umax)
        };
        let on: Vec<usize> = (0..u.len())
            .filter(|&k| u[k] > u0 - 1.0 && u[k] < u1 + 1.0)
            .collect();
        if on.is_empty() {
            continue;
        }
        let m3 = [fc.m[0], fc.m[1], 0.0];
        let t3 = [fc.t[0], fc.t[1], 0.0];
        let d3 = [-libm::cos(a) * m3[0], -libm::cos(a) * m3[1], -libm::sin(a)];
        let sm = on.iter().map(|&k| s_[k]).fold(f64::NEG_INFINITY, f64::max);
        let p0 = [fc.p0[0] + sm * m3[0], fc.p0[1] + sm * m3[1], 0.0];
        let ta = libm::tan(a);
        let zp: Vec<f64> = on
            .iter()
            .map(|&k| t.positions[k][2] + (sm - s_[k]) * ta)
            .collect();
        let zpmin = zp.iter().copied().fold(f64::INFINITY, f64::min);
        let zpmax = zp.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let zall = t
            .positions
            .iter()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let n_us = ((u1 + 1.0 + g - (u0 - 1.0)) / g).ceil() as usize;
        let zhi = zpmax.min(zall + 1.5) + g;
        let n_zs = ((zhi - zpmin) / g).ceil() as usize;
        if n_us < 3 || n_zs < 3 {
            continue;
        }
        let us: Vec<f64> = (0..n_us).map(|i| u0 - 1.0 + i as f64 * g).collect();
        let zs: Vec<f64> = (0..n_zs).map(|j| zpmin + j as f64 * g).collect();
        let mut origins = Vec::with_capacity(n_us * n_zs);
        for &z in &zs {
            for &uu in &us {
                origins.push([
                    p0[0] + uu * t3[0] - d3[0],
                    p0[1] + uu * t3[1] - d3[1],
                    z - d3[2],
                ]);
            }
        }
        let dirs = vec![d3; origins.len()];
        let rm = RayMesh::new(&t);
        let hits = rm.first_hits(&origins, &dirs);
        let nrm_of = |tri: u32| tri_normal(&t.positions, t.triangles[tri as usize]);
        let nn = origins.len();
        let mut front = cell_grid(n_us, n_zs, f64::NAN);
        let mut al = cell_grid(n_us, n_zs, 0.0_f64);
        let mut nzf = cell_grid(n_us, n_zs, 0.0_f64);
        let mut back = cell_grid(n_us, n_zs, f64::NAN);
        let mut o2 = Vec::new();
        let mut idx2 = Vec::new();
        for k in 0..nn {
            if let Some(h) = &hits[k] {
                let d = dot(
                    [
                        h.point[0] - origins[k][0],
                        h.point[1] - origins[k][1],
                        h.point[2] - origins[k][2],
                    ],
                    d3,
                );
                front.z[k] = d;
                let nrm = nrm_of(h.triangle);
                al.z[k] = dot(nrm, m3);
                nzf.z[k] = nrm[2];
                o2.push([
                    h.point[0] + d3[0] * 1e-4,
                    h.point[1] + d3[1] * 1e-4,
                    h.point[2] + d3[2] * 1e-4,
                ]);
                idx2.push(k);
            }
        }
        let hits2 = rm.first_hits(&o2, &vec![d3; o2.len()]);
        for (h, &k) in hits2.iter().zip(&idx2) {
            if let Some(h) = h
                && nrm_of(h.triangle)[2] > -0.5
            {
                {
                    let d = dot(
                        [
                            h.point[0] - origins[k][0],
                            h.point[1] - origins[k][1],
                            h.point[2] - origins[k][2],
                        ],
                        d3,
                    );
                    back.z[k] = d;
                }
            }
        }
        let hit = front.map(|v| !v.is_nan());
        let mut w = cell_grid(n_us, n_zs, 0.0_f64);
        for k in 0..nn {
            if hit.z[k] && nzf.z[k] > -0.85 {
                w.z[k] = smooth01((al.z[k] - 0.35) / 0.3);
            }
        }
        let m_raw = w.map(|v| v > 0.5);
        let mask = fill_holes(&m_raw);
        if mask.z.iter().filter(|&&b| b).count() < 50 {
            continue;
        }
        // distance in from its edges (padded by a cell of background)
        let mut padded = cell_grid(n_us + 2, n_zs + 2, false);
        for j in 0..n_zs {
            for i in 0..n_us {
                padded.set(i + 1, j + 1, mask.at(i, j));
            }
        }
        let ep = edt_in(&padded);
        let mut e = cell_grid(n_us, n_zs, 0.0_f64);
        for j in 0..n_zs {
            for i in 0..n_us {
                e.set(i, j, ep.at(i + 1, j + 1) * g);
            }
        }
        // G, F sampled from the face
        let mut gg = cell_grid(n_us, n_zs, 0.0_f64);
        let mut ff = cell_grid(n_us, n_zs, 0.0_f64);
        for j in 0..n_zs {
            for i in 0..n_us {
                let (cxu, cz) = (us[i] / RELIEF_GRID_MM, (zs[j] - fc.z0) / RELIEF_GRID_MM);
                gg.set(i, j, sample(&fc.g, cxu, cz, true));
                let mut fv = sample(&fc.f, cxu, cz, false);
                fv *= smooth01(us[i].min(fc.l - us[i]) / FACET_END_MM);
                ff.set(i, j, fv);
            }
        }
        if let Some((cw, ch)) = core {
            for j in 0..n_zs {
                for i in 0..n_us {
                    let q = j * n_us + i;
                    let fr = if front.z[q].is_nan() { 0.0 } else { front.z[q] };
                    let mut off = smooth01(RELIEF_OFF_MM - (sm + libm::cos(a) * (1.0 - fr)).abs());
                    if !fc.rim {
                        let x = origins[q][0] + fr * d3[0];
                        let y = origins[q][1] + fr * d3[1];
                        let bd = x.min(cw - x).min(y.min(ch - y));
                        off *= smooth01(bd - 1.0);
                    }
                    ff.z[q] *= off;
                    gg.z[q] *= off;
                }
            }
        }
        // nearest hit cell for every cell
        let (_, near) = hit.distance();
        let fnear = |f: &Field| -> Field {
            let mut o = f.clone();
            for q in 0..nn {
                if let Some(k) = near[q] {
                    o.z[q] = f.z[k];
                }
            }
            o
        };
        let front_n = fnear(&front);
        let k: Vec<(usize, usize)> = (0..n_zs)
            .flat_map(|j| (0..n_us).map(move |i| (i, j)))
            .filter(|&(i, j)| mask.at(i, j) && e.at(i, j) > 1.0)
            .collect();
        let base = if core.is_none() && k.len() > 50 {
            let c = fit(&us, &zs, &k, &front);
            let mut b = cell_grid(n_us, n_zs, 0.0_f64);
            for j in 0..n_zs {
                for i in 0..n_us {
                    b.set(i, j, c[0] + c[1] * us[i] + c[2] * zs[j]);
                }
            }
            b
        } else {
            front_n.blur(0.4 / g)
        };
        let room = {
            let mut r = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                let bf = back.z[q] - front.z[q];
                let v = if bf.is_nan() { f64::INFINITY } else { bf };
                r.z[q] = (v - CRACK_WALL_MM).max(0.0);
            }
            r
        };
        let cut = if a != 0.0 {
            let mut c0 = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                let fr = front.z[q];
                let v = (fr - (fr - base.z[q] + ff.z[q]).min(0.0).max(-room.z[q]) - CUT_SHORT_MM)
                    .max(f64::MIN);
                c0.z[q] = v;
            }
            let c0 = fnear(&Field {
                z: c0
                    .z
                    .iter()
                    .enumerate()
                    .map(|(q, &v)| if hit.z[q] { v } else { f64::NAN })
                    .collect(),
                ..c0
            });
            let edge = e.map(|v| if v < 1.0 { 1.0 } else { 0.0 });
            let wide = edge
                .blur(CUT_BLUR_MM.1 / g)
                .map(|v| (2.0 * v).clamp(0.0, 1.0));
            let fine = c0.blur(CUT_BLUR_MM.0 / g);
            let coarse = c0.blur(CUT_BLUR_MM.1 / g);
            let mut o = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                o.z[q] = (fine.z[q] + wide.z[q] * (coarse.z[q] - fine.z[q])).max(0.02);
            }
            o
        } else {
            let mut gw = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                gw.z[q] = -w.z[q] * gg.z[q];
            }
            let gb = gw.blur(CUT_BLUR_MM.0 / g);
            let mut crack = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                crack.z[q] = gb.z[q].min(room.z[q]);
            }
            let crack = fnear(&crack);
            let fr = &front_n;
            let mut o = cell_grid(n_us, n_zs, 0.0_f64);
            for q in 0..nn {
                o.z[q] = if crack.z[q] > 0.01 {
                    fr.z[q] + crack.z[q] - CUT_SHORT_MM * (1.0 - smooth01(crack.z[q] / 0.2))
                } else {
                    0.02
                };
            }
            o
        };
        let hm = hung(&Field { step: g, ..cut }, 0.01)?;
        let cutter = place_hung(&hm, (us[0], zs[0]), p0, t3, Z_UP, d3);
        let out = rock.minus(&[cutter])?;
        if !out.is_empty() {
            rock = out;
        }
    }
    Ok(rock)
}

/// A height-field solid z in [0, D] moved into the face frame: local
/// (x, y, z) -> p0 + (u0 + x) t3 + (z0 + y) Z + (1 - z) (-d3).
fn place_hung(
    hm: &TriMesh,
    (u0, z0): (f64, f64),
    p0: [f64; 3],
    t3: [f64; 3],
    zup: [f64; 3],
    d3: [f64; 3],
) -> TriMesh {
    let mut out = hm.clone();
    for p in &mut out.positions {
        let (u, zc, w) = (u0 + p[0], z0 + p[1], 1.0 - p[2]);
        *p = [
            p0[0] + u * t3[0] + zc * zup[0] - w * d3[0],
            p0[1] + u * t3[1] + zc * zup[1] - w * d3[1],
            p0[2] + u * t3[2] + zc * zup[2] - w * d3[2],
        ];
    }
    let c = [
        t3[1] * zup[2] - t3[2] * zup[1],
        t3[2] * zup[0] - t3[0] * zup[2],
        t3[0] * zup[1] - t3[1] * zup[0],
    ];
    // x -> t3, y -> zup, z -> d3
    if dot(c, d3) < 0.0 {
        out.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    out
}
