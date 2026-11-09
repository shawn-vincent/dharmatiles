//! Where a tree stands and what it leaves behind: its place on the
//! ground, the footing it is stood on, and the crumbs and sealed pockets
//! its unions leave (shared with the wall, which stands and cleans its
//! stones the same way).

use dt_kit::{Material, Rng, Scene, Solid, TriMesh, form, graph, union};

use super::leaves;
use crate::{Ctx, LookError};

/// The smallest body [`crumbs`] keeps, mm³ (a printed line's bead,
/// 0.45 × 0.45 × 0.2 mm, is 0.04).
const CRUMB_MM3: f64 = 0.05;

/// `solid` less its crumbs: the slivers and sealed pockets a union or a
/// cut leaves (between noisy clumps, between bark-grooved tubes, at a
/// bevel's edge) are bodies under [`CRUMB_MM3`] or inside out. A sealed
/// pocket cannot print and the audit would drop it. The rebuilt solid
/// takes one fresh slot of `material`. Returns how many were dropped.
pub(super) fn crumbs(
    scene: &mut Scene,
    solid: Solid,
    material: Material,
) -> Result<(Solid, usize), LookError> {
    drop_crumbs(scene, solid, material, false)
}

/// [`crumbs`], keeping sealed pockets (inside-out bodies) when
/// `keep_voids`: only loose bodies under [`CRUMB_MM3`] go (what a wall's
/// union leaves where a tiny closer meets the core's rounding).
pub(crate) fn drop_crumbs(
    scene: &mut Scene,
    solid: Solid,
    material: Material,
    keep_voids: bool,
) -> Result<(Solid, usize), LookError> {
    let bodies = leaves::split_bodies(&solid.mesh());
    if bodies.len() <= 1 {
        return Ok((solid, 0));
    }
    let keep: Vec<&TriMesh> = bodies
        .iter()
        .filter(|b| {
            let v = dt_kit::signed_volume(b);
            v >= CRUMB_MM3 || (keep_voids && v < 0.0)
        })
        .collect();
    if keep.len() == bodies.len() {
        return Ok((solid, 0));
    }
    let mut m = TriMesh::default();
    for b in &keep {
        let base = u32::try_from(m.positions.len()).map_err(LookError::build)?;
        m.positions.extend_from_slice(&b.positions);
        m.triangles
            .extend(b.triangles.iter().map(|t| t.map(|v| v + base)));
    }
    let slot = scene.slots(1, material).map_err(LookError::build)?;
    let out = form::solid(slot.get(0), &m).map_err(LookError::build)?;
    Ok((out, bodies.len() - keep.len()))
}

/// Loose bodies under this, mm³, that the last union cut off the tree (a
/// knife-edge of a bevelled clump a leaf half covers) are plugged back on.
const ISLE_MM3: f64 = 0.01;

/// `solid` with the sealed air pockets its last union closed in (where a
/// bevelled clump meets a branch) filled: each pocket's convex hull, a
/// tenth larger, is unioned back as a plug (it is inside the wood and the
/// clump, so nothing shows). A tiny loose body it cut off (under
/// [`ISLE_MM3`]) is plugged the same way, so it fuses back to the tree it
/// touches instead of being dropped. Returns how many were filled.
pub(super) fn fill_pockets(scene: &mut Scene, solid: Solid) -> Result<(Solid, usize), LookError> {
    let bodies = leaves::split_bodies(&solid.mesh());
    let main = bodies.iter().map(dt_kit::signed_volume).fold(0.0, f64::max);
    let mut left_out = 0;
    let plugs: Vec<TriMesh> = bodies
        .into_iter()
        .filter(|b| {
            let v = dt_kit::signed_volume(b);
            v < 0.0 || (v < main && v < ISLE_MM3)
        })
        .filter_map(|b| {
            #[allow(clippy::cast_precision_loss)]
            let n = b.positions.len() as f64;
            let c = [0, 1, 2].map(|k| b.positions.iter().map(|p| p[k]).sum::<f64>() / n);
            let pts: Vec<[f64; 3]> = b
                .positions
                .iter()
                .map(|p| graph::add(c, graph::scale(graph::sub(*p, c), 1.1)))
                .collect();
            // a degenerate pocket: no plug, and counted
            form::hull(&pts).map_err(|_| left_out += 1).ok()
        })
        .collect();
    scene.left_out("tree", left_out);
    if plugs.is_empty() {
        return Ok((solid, 0));
    }
    let n = u32::try_from(plugs.len()).map_err(LookError::build)?;
    let slots = scene.slots(n, Material::Leaf).map_err(LookError::build)?;
    let mut parts = vec![solid];
    for (k, m) in plugs.iter().enumerate() {
        parts.push(
            form::solid(slots.get(u32::try_from(k).unwrap_or(0)), m).map_err(LookError::build)?,
        );
    }
    Ok((union(&parts).map_err(LookError::build)?, plugs.len()))
}

/// A box of half-width `r` round `at`, from `z0` to `z1`: the footing
/// [`dt_kit::stand`] stands the wood on.
pub(crate) fn ground_box(
    scene: &mut Scene,
    at: [f64; 3],
    r: f64,
    z0: f64,
    z1: f64,
) -> Result<Solid, LookError> {
    let positions = (0..8)
        .map(|i| {
            [
                at[0] + if i & 1 == 1 { r } else { -r },
                at[1] + if i & 2 == 2 { r } else { -r },
                if i & 4 == 4 { z1 } else { z0 },
            ]
        })
        .collect();
    let m = TriMesh {
        positions,
        triangles: vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ],
        ..TriMesh::default()
    };
    let slot = scene.slots(1, Material::Wood).map_err(LookError::build)?;
    form::solid(slot.get(0), &m).map_err(LookError::build)
}

/// How far the ground under a disc of `r` round `at` falls below `at`.
pub(super) fn ground_drop(scene: &Scene, at: [f64; 3], r: f64) -> f64 {
    let Some(f) = scene.surface_field() else {
        return 0.0;
    };
    let mut lo = at[2];
    for (k, z) in f.z.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = ((k % f.nx) as f64 * f.step, (k / f.nx) as f64 * f.step);
        if libm::hypot(x - at[0], y - at[1]) <= r {
            lo = lo.min(f64::from(*z));
        }
    }
    at[2] - lo
}

/// Where the tree stands, world mm on the ground: `at=` (a range drawn
/// once), or anywhere in the area with the trunk `margin` from the tile
/// edge and clear of stamped solids (nudged toward the middle when
/// nothing qualifies; never clipped).
pub(super) fn place(
    scene: &Scene,
    ctx: &Ctx<'_>,
    margin: f64,
    clear: f64,
) -> Result<[f64; 3], LookError> {
    let frame = *scene.frame();
    let sq = frame.square_mm;
    let [w, h] = frame.size_mm();
    let mut rng = Rng::new(ctx.seed.key("place"));
    let a = ctx.area;
    let step = frame.step_mm();
    #[allow(clippy::cast_precision_loss)]
    let xy = |k: usize| ((k % a.nx) as f64 * step, (k / a.nx) as f64 * step);
    // the trunk a crown radius from the tile edge (all of the tile's
    // middle when the crown is wider than the tile)
    let m = margin.min(0.5 * w.min(h));
    let inside =
        |x: f64, y: f64| x >= m - 1e-9 && x <= w - m + 1e-9 && y >= m - 1e-9 && y <= h - m + 1e-9;
    // the area's node nearest the tile's middle
    let middle = || {
        (0..a.bits.len())
            .filter(|&k| a.bits[k])
            .min_by(|&p, &q| {
                let d = |k: usize| {
                    let (x, y) = xy(k);
                    libm::hypot(x - 0.5 * w, y - 0.5 * h)
                };
                d(p).total_cmp(&d(q))
            })
            .ok_or_else(|| LookError::build("the tree's area is empty"))
    };
    let (x, y) = if let Some([dx, dy]) = ctx.at {
        // a number is a range of no width and still takes its draw, so the
        // stream after it is where it always was
        let (u, v) = (dx.draw(&mut rng), dy.draw(&mut rng));
        // nudged in so the crown stays on the tile, never clipped
        let x = (u * sq).clamp(m, w - m);
        let y = (v * sq).clamp(m, h - m);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let node = |x: f64, y: f64| {
            let i = libm::round(x / step).clamp(0.0, (a.nx - 1) as f64) as usize;
            let j = libm::round(y / step).clamp(0.0, (a.ny - 1) as f64) as usize;
            j * a.nx + i
        };
        if a.bits[node(x, y)] {
            (x, y)
        } else {
            // outside its area (`at=` a region): nudged to the nearest area node
            // that keeps the crown on the tile, as the automatic path
            // keeps to its area
            let near = |k: usize| {
                let (px, py) = xy(k);
                libm::hypot(px - x, py - y)
            };
            let best = (0..a.bits.len())
                .filter(|&k| {
                    a.bits[k] && {
                        let (px, py) = xy(k);
                        inside(px, py)
                    }
                })
                .min_by(|&p, &q| near(p).total_cmp(&near(q)).then(p.cmp(&q)));
            xy(match best {
                Some(k) => k,
                None => middle()?,
            })
        }
    } else {
        let support = scene.support();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rr = libm::ceil(clear / step) as usize;
        let clear_of = |k: usize| -> bool {
            let Some(sup) = support else { return true };
            let (i, j) = (k % a.nx, k / a.nx);
            for jj in j.saturating_sub(rr)..=(j + rr).min(a.ny - 1) {
                for ii in i.saturating_sub(rr)..=(i + rr).min(a.nx - 1) {
                    #[allow(clippy::cast_precision_loss)]
                    let d =
                        libm::hypot((ii as f64 - i as f64) * step, (jj as f64 - j as f64) * step);
                    if d <= clear && sup.z[jj * a.nx + ii].is_finite() {
                        return false;
                    }
                }
            }
            true
        };
        let cands: Vec<usize> = (0..a.bits.len())
            .filter(|&k| a.bits[k])
            .filter(|&k| {
                let (x, y) = xy(k);
                inside(x, y)
            })
            .collect();
        let free: Vec<usize> = cands.iter().copied().filter(|&k| clear_of(k)).collect();
        let pick = if free.is_empty() { &cands } else { &free };
        let k = if pick.is_empty() {
            // nudged: the area's node nearest the tile's middle
            middle()?
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_possible_wrap
            )]
            let i = rng.int(0, (pick.len() - 1) as i64) as usize;
            pick[i]
        };
        xy(k)
    };
    Ok([x, y, ground_at(scene, x, y)])
}

/// The ground (or what a stamped solid stands up to) under `(x, y)`.
fn ground_at(scene: &Scene, x: f64, y: f64) -> f64 {
    let node = |f: &dt_kit::Grid| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let (i, j) = (
            libm::round(x / f.step).clamp(0.0, (f.nx - 1) as f64) as usize,
            libm::round(y / f.step).clamp(0.0, (f.ny - 1) as f64) as usize,
        );
        f.z[j * f.nx + i]
    };
    let g = scene.surface_field().map_or(0.0, |f| f64::from(node(f)));
    let s = scene
        .support()
        .map_or(f64::NEG_INFINITY, |f| f64::from(node(f)));
    g.max(s)
}
