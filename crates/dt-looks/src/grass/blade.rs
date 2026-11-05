//! One blade: a print-native stack of strokes, smoothed.
//!
//! A blade is a stack of 0.2 mm layers, each a stroke along its spine a
//! whole number of 0.45 mm lines wide:
//!
//! - cross-section: a lens of line counts, 2 at the bottom, widest in the
//!   middle, 1 on top, growing at most one line a layer;
//! - profile: a convex rise, each layer starting where the crest reaches it;
//! - tip: only the top layer reaches the tip, each layer below stops
//!   0.2 mm short (the raked bow); and the stroke loses a line every
//!   0.45 mm toward each end, so the ends are points made of line ends.
//!
//! The blade is a smooth loft built directly, not a signed-distance
//! stack marched into cubes and decimated: one ring per layer, each the outline of
//! its stroke as a half-width profile along the spine, so a blade is a few
//! hundred faces, watertight by construction, and meshing is a pure
//! function of the frond (blades are built in parallel, gathered in order).

use dt_kit::TriMesh;

use super::fronds::{Frond, TRACE_STEP_MM};

const LINE_W_MM: f64 = 0.45;
const LAYER_MM: f64 = 0.2;
const KEEL_LINES: i64 = 2;
const TOP_LINES: i64 = 1;
const MAX_LINES: (i64, i64) = (2, 4);
const LENS_EXP: f64 = 0.6;
const RAKE_MM: f64 = 0.2;
const RISE_EXP: f64 = 2.0;
/// Blades reach this far below the ground.
pub const EMBED_MM: f64 = 0.3;
/// Stations along each side of a layer's outline, tip to tip.
const STATIONS: usize = 17;
/// A layer's end: the half-width at its first full station, and the
/// slope of the ramp up to the plateau (a line is added every 0.45 mm of
/// stroke, half a line per side).
const RAMP: f64 = 0.5;
/// Closing each layer's outline by 0.3 mm and blurring it fills
/// the notches between lines and rounds the ends and the shoulder where a
/// ramp meets the plateau: the ends are rounder than one line's cap, and
/// the minimum is soft.
const CAP_MM: f64 = 0.3;
const CREST_EASE_MM: f64 = 0.1;
const SOFT_MM: f64 = 0.12;

fn softmin(a: f64, b: f64) -> f64 {
    let k = SOFT_MM;
    -k * libm::log(libm::exp(-a / k) + libm::exp(-b / k))
}

/// Line counts per layer: a lens, 2 at the keel, 1 on top, +1 a layer at most.
fn lines(n_layers: usize, half_w: f64) -> Vec<i64> {
    #[allow(clippy::cast_possible_truncation)]
    let n_max = (libm::round(2.0 * half_w / LINE_W_MM) as i64).clamp(MAX_LINES.0, MAX_LINES.1);
    let mut n: Vec<i64> = (0..n_layers)
        .map(|k| {
            let a = core::f64::consts::PI * (k as f64 + 0.5) / n_layers as f64;
            #[allow(clippy::cast_possible_truncation)]
            let v = libm::round(n_max as f64 * libm::pow(libm::sin(a), LENS_EXP)) as i64;
            v.max(1)
        })
        .collect();
    n[0] = n[0].max(KEEL_LINES);
    n[n_layers - 1] = TOP_LINES;
    for k in 1..n_layers {
        n[k] = n[k].min(n[k - 1] + 1);
    }
    n
}

/// The spine at arc length `u` and its unit left normal.
fn spine(path: &[[f64; 2]], u: f64) -> ([f64; 2], [f64; 2]) {
    let f = (u / TRACE_STEP_MM).clamp(0.0, (path.len() - 1) as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (f as usize).min(path.len() - 2);
    let t = f - i as f64;
    let (a, b) = (path[i], path[i + 1]);
    let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let d = [b[0] - a[0], b[1] - a[1]];
    let n = libm::hypot(d[0], d[1]).max(1e-12);
    (p, [-d[1] / n, d[0] / n])
}

/// The radius of curvature of the spine at arc length `u`, from three
/// points a `d` either side (infinite on a straight run).
fn bend_radius(path: &[[f64; 2]], u: f64) -> f64 {
    let d = 0.4;
    let (a, _) = spine(path, (u - d).max(0.0));
    let (b, _) = spine(path, u);
    let (c, _) = spine(path, u + d);
    let ab = libm::hypot(b[0] - a[0], b[1] - a[1]);
    let bc = libm::hypot(c[0] - b[0], c[1] - b[1]);
    let ca = libm::hypot(a[0] - c[0], a[1] - c[1]);
    let area2 = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
    if area2 < 1e-9 {
        f64::INFINITY
    } else {
        ab * bc * ca / (2.0 * area2)
    }
}

/// How much of the bend radius a half-width may take before the inside
/// offset curve folds over itself.
const BEND_FRACTION: f64 = 0.8;

/// One layer's ring: `2·STATIONS − 2` points in the plan (tail tip, left
/// side tip to tip, front tip, right side back), each with the spine
/// position it stands at.
fn outline_once(path: &[[f64; 2]], ub: f64, uf: f64, hw_max: f64) -> Vec<[f64; 2]> {
    let cap = CAP_MM;
    let len = uf - ub;
    // stations from the tail tip to the front tip, denser at the ends
    let st: Vec<f64> = (0..STATIONS)
        .map(|i| {
            let t =
                0.5 * (1.0 - libm::cos(core::f64::consts::PI * i as f64 / (STATIONS - 1) as f64));
            -cap + (len + 2.0 * cap) * t
        })
        .collect();
    let capr = cap.min(hw_max);
    let half = |e: f64| -> f64 {
        // e from the tail end of the stroke (negative: behind it)
        let (dt, df) = (e, len - e);
        if dt < 0.0 {
            // the round cap of the shortest stroke
            return capr * libm::sqrt((1.0 - dt * dt / (cap * cap)).max(0.0));
        }
        if df < 0.0 {
            return capr * libm::sqrt((1.0 - df * df / (cap * cap)).max(0.0));
        }
        softmin(softmin(hw_max, capr + RAMP * dt), capr + RAMP * df)
    };
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (i, &e) in st.iter().enumerate() {
        let (p, n) = spine(path, (ub + e).max(0.0));
        let hw = if i == 0 || i == STATIONS - 1 {
            0.0
        } else {
            // never wider than the spine's bend allows: the inside offset
            // would fold and the blade would cross itself
            half(e)
                .min(BEND_FRACTION * bend_radius(path, (ub + e).max(0.0)))
                .max(0.03)
        };
        left.push([p[0] + n[0] * hw, p[1] + n[1] * hw]);
        right.push([p[0] - n[0] * hw, p[1] - n[1] * hw]);
    }
    // tail tip, left 1..S-2, front tip, right S-2..1
    let mut ring = Vec::with_capacity(2 * STATIONS - 2);
    ring.push(left[0]);
    ring.extend_from_slice(&left[1..STATIONS - 1]);
    ring.push(left[STATIONS - 1]);
    ring.extend(right[1..STATIONS - 1].iter().rev());
    ring
}

/// The steepest slope (rise per run) a blade's layers follow. A blade is
/// printed in flat layers, so each layer's reach past the one below (the
/// raked bow, a line more in width) is read against the ground it climbs or
/// falls away from; on a steeper slope the draped layers would overhang
/// past the printer's limit. Steeper ground is followed only this much: the
/// blade stands straight in it, buried on the uphill side.
const DRAPE_SLOPE: f64 = 0.12;

/// Whether a closed polygon crosses itself.
fn simple(ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (c, d) = (ring[j], ring[(j + 1) % n]);
            let (d1, d2) = (cross(a, b, c), cross(a, b, d));
            let (d3, d4) = (cross(c, d, a), cross(c, d, b));
            if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                return false;
            }
        }
    }
    true
}

/// A layer's outline that does not cross itself: narrower, if the bend
/// of the spine makes the first one fold.
fn outline(path: &[[f64; 2]], ub: f64, uf: f64, hw_max: f64) -> Vec<[f64; 2]> {
    let mut hw = hw_max;
    for _ in 0..6 {
        let ring = outline_once(path, ub, uf, hw);
        if simple(&ring) {
            return ring;
        }
        hw *= 0.7;
    }
    outline_once(path, ub, uf, 0.03)
}

/// A blade as a closed mesh, seated on the ground: `seat(x, y)` is the
/// smooth ground height under a point (the layers drape over it) and `raw`
/// the ground as it is (the flat foot goes `EMBED_MM` below the lowest of
/// it under the blade).
pub fn mesh(
    f: &Frond,
    seat: &dyn Fn(f64, f64) -> f64,
    raw: &dyn Fn(f64, f64) -> f64,
    jitter: [f64; 3],
) -> Option<TriMesh> {
    // Streamlines that merge (where the flow parts round a rock or a
    // clearing) would carry two blades on one spine, and a union cannot
    // resolve two near-identical surfaces cleanly. Each blade stands a
    // hair (up to 50 µm) off its streamline, and a hair high or low.
    let shifted: Vec<[f64; 2]> = f
        .path
        .iter()
        .map(|p| [p[0] + jitter[0], p[1] + jitter[1]])
        .collect();
    let f = &Frond {
        path: shifted,
        ..f.clone()
    };
    let n_layers = ((libm::round(f.height / LAYER_MM)) as usize).max(2);
    if f.height < 2.0 * LAYER_MM || f.length < 0.6 {
        return None;
    }
    let n = lines(n_layers, f.half_w);
    let l = f.length;
    let mut rings: Vec<(Vec<[f64; 2]>, f64)> = Vec::new(); // plan, height over the ground
    for k in 0..n_layers {
        let u_back =
            l * (1.0 - libm::pow(1.0 - (k as f64 + 0.5) / n_layers as f64, 1.0 / RISE_EXP));
        let u_front = l - (n_layers - 1 - k) as f64 * RAKE_MM;
        if u_front - u_back < 0.3 {
            continue;
        }
        let hw = n[k] as f64 * LINE_W_MM / 2.0;
        // the blur (0.15 mm across layers) takes about a tenth of a
        // millimetre off a one-line ridge: the layers close up a little
        // toward the top
        let ease = CREST_EASE_MM * k as f64 / (n_layers - 1) as f64;
        rings.push((
            outline(&f.path, u_back, u_front, hw),
            (k as f64 + 0.5) * LAYER_MM - ease,
        ));
    }
    if rings.len() < 2 {
        return None;
    }
    // the ridge closes over half a layer: the top outline, drawn in
    let top = rings.last()?.clone();
    let c = centroid(&top.0);
    let drawn = |k: f64| -> Vec<[f64; 2]> {
        top.0
            .iter()
            .map(|p| [c[0] + k * (p[0] - c[0]), c[1] + k * (p[1] - c[1])])
            .collect()
    };
    // the ridge closes over half a layer in two steps, rounder than one
    rings.push((drawn(0.85), top.1 + LAYER_MM * 0.2));
    rings.push((drawn(0.5), top.1 + LAYER_MM * 0.5));
    // the foot: the first outline, straight down
    let foot = rings[0].0.clone();
    let m = foot.len();
    // the ground the layers follow: the seat, its slope limited
    let at = f.path[0];
    let z0 = seat(at[0], at[1]);
    let d = 0.3;
    let g = rings
        .iter()
        .flat_map(|(plan, _)| plan.iter())
        .map(|p| {
            let gx = (seat(p[0] + d, p[1]) - seat(p[0] - d, p[1])) / (2.0 * d);
            let gy = (seat(p[0], p[1] + d) - seat(p[0], p[1] - d)) / (2.0 * d);
            libm::hypot(gx, gy)
        })
        .fold(0.0, f64::max);
    let lambda = if g > DRAPE_SLOPE {
        DRAPE_SLOPE / g
    } else {
        1.0
    };
    // a hair of height of its own, so no two blades have coplanar layers
    let drape = |x: f64, y: f64| z0 + lambda * (seat(x, y) - z0) + jitter[2];
    let low = rings
        .iter()
        .flat_map(|(plan, _)| plan.iter())
        .map(|p| raw(p[0], p[1]).min(drape(p[0], p[1])))
        .fold(f64::INFINITY, f64::min);
    let floor = low - EMBED_MM;
    let mut positions: Vec<[f64; 3]> = Vec::new();
    positions.extend(foot.iter().map(|p| [p[0], p[1], floor]));
    for (plan, h) in &rings {
        positions.extend(plan.iter().map(|p| [p[0], p[1], drape(p[0], p[1]) + h]));
    }
    let nr = rings.len() + 1;
    let u = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for r in 0..nr - 1 {
        let (a, b) = (r * m, (r + 1) * m);
        for j in 0..m {
            let j1 = (j + 1) % m;
            tris.push([u(a + j), u(b + j), u(b + j1)]);
            tris.push([u(a + j), u(b + j1), u(a + j1)]);
        }
    }
    // caps: a fan from the centroid
    let cb = positions.len();
    let c0 = centroid(&foot);
    positions.push([c0[0], c0[1], floor]);
    for j in 0..m {
        tris.push([u(cb), u(j), u((j + 1) % m)]);
    }
    let ct = positions.len();
    let last = (nr - 1) * m;
    let c1 = centroid(&rings.last()?.0);
    let ztop = positions[last..last + m].iter().map(|p| p[2]).sum::<f64>() / m as f64;
    positions.push([c1[0], c1[1], ztop]);
    for j in 0..m {
        tris.push([u(ct), u(last + (j + 1) % m), u(last + j)]);
    }
    let mut mesh = TriMesh {
        positions,
        triangles: tris,
        ..TriMesh::default()
    };
    if dt_kit::signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    Some(mesh)
}

/// A closed mesh through `rings` (each a closed loop of the same length),
/// capped at both ends by a fan from the centroid, wound outward.
pub fn ring_mesh(rings: &[Vec<[f64; 3]>]) -> TriMesh {
    let m = rings[0].len();
    let u = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut positions: Vec<[f64; 3]> = rings.iter().flatten().copied().collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for r in 0..rings.len() - 1 {
        let (a, b) = (r * m, (r + 1) * m);
        for j in 0..m {
            let j1 = (j + 1) % m;
            tris.push([u(a + j), u(b + j), u(b + j1)]);
            tris.push([u(a + j), u(b + j1), u(a + j1)]);
        }
    }
    let mean = |ring: &[[f64; 3]]| {
        let n = ring.len() as f64;
        [0, 1, 2].map(|k| ring.iter().map(|p| p[k]).sum::<f64>() / n)
    };
    let cb = positions.len();
    positions.push(mean(&rings[0]));
    for j in 0..m {
        tris.push([u(cb), u(j), u((j + 1) % m)]);
    }
    let ct = positions.len();
    let last = (rings.len() - 1) * m;
    positions.push(mean(&rings[rings.len() - 1]));
    for j in 0..m {
        tris.push([u(ct), u(last + (j + 1) % m), u(last + j)]);
    }
    let mut mesh = TriMesh {
        positions,
        triangles: tris,
        ..TriMesh::default()
    };
    if dt_kit::signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    mesh
}

fn centroid(ring: &[[f64; 2]]) -> [f64; 2] {
    let n = ring.len() as f64;
    [
        ring.iter().map(|p| p[0]).sum::<f64>() / n,
        ring.iter().map(|p| p[1]).sum::<f64>() / n,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn frond(len: f64, h: f64, hw: f64, bend: f64) -> Frond {
        let path: Vec<[f64; 2]> = (0..80)
            .map(|i| {
                let u = i as f64 * TRACE_STEP_MM;
                [u, bend * u * u]
            })
            .collect();
        Frond {
            path,
            length: len,
            half_w: hw,
            height: h,
        }
    }

    fn closed(m: &TriMesh) -> Result<(), String> {
        let mut e: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &m.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *e.entry((a, b)).or_default() += 1;
            }
        }
        for (&(a, b), &c) in &e {
            if c != 1 || e.get(&(b, a)) != Some(&1) {
                return Err(format!("edge {a}-{b} x{c}, back {:?}", e.get(&(b, a))));
            }
        }
        Ok(())
    }

    #[test]
    fn blades_are_closed_manifolds() {
        for (len, h, hw, bend) in [
            (5.0, 1.8, 1.0, 0.0),
            (3.5, 1.5, 0.8, 0.02),
            (6.5, 2.0, 1.25, -0.03),
            (1.6, 0.6, 0.5, 0.0),
        ] {
            let f = frond(len, h, hw, bend);
            let m = mesh(&f, &|_, _| 3.0, &|_, _| 3.0, [0.0; 3]).unwrap();
            closed(&m).unwrap_or_else(|e| panic!("{len}: {e}"));
            assert!(dt_kit::signed_volume(&m) > 0.0);
            eprintln!(
                "{len}: {} verts {} faces",
                m.positions.len(),
                m.triangles.len()
            );
        }
    }
}
