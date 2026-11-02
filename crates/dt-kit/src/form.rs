//! Forms: the P5 operations that make stones, blocks and branches as
//! triangle meshes and turn them into solids (walls, floors, trees).
//!
//! A form is built as a [`TriMesh`] in its own frame (a level set of a
//! signed distance, a convex hull), finished there ([`relief`], the
//! common stone texture; any transform the look applies to positions),
//! and then becomes one leaf solid with [`solid`] under its slot. That is
//! the one allowed round trip through a triangle mesh (`dt_geom::source`
//! rule 2): every form is a single-source leaf before its material is
//! recorded. [`clip`] cuts a finished solid to a box, the cut faces under
//! a slot of the solid's own material.
//!
//! | op | what it does | used by |
//! |---|---|---|
//! | [`level_set`] | the solid where an SDF is positive, meshed | wall blocks, floor slabs, fieldstone, foliage |
//! | [`hull`] | the convex hull of points | rubble chips, mortar cores, wall stones |
//! | [`solid`] | a closed mesh → a leaf solid | every form |
//! | [`clip`] | a solid ∩ an axis-aligned box | tile-edge butt joins, floor bottoms |
//! | [`relief`] | the common stone relief, displaced along smoothed normals | wall blocks, floor slabs, fieldstone |
//! | [`relief_weighted`] | the relief, scaled per vertex (faded edges, patchy calm) | wall blocks, fieldstone |
//! | [`subdivide`] | split every triangle in four (midpoints) | fieldstone (relief needs the density) |
//! | [`fbm3`] | isotropic 3D value-noise fBm | relief, foliage noise, bark |

use dt_geom::{Bounds, Mesh, TriMesh};

use crate::KitError;
use crate::graph::dist;
use crate::scene::{Slot, Solid, leaf};

/// **level set**: the closed surface where `sdf(p) > 0` (positive inside)
/// within `[lo, hi]`, on a grid of `edge_mm`, as a triangle mesh in the
/// SDF's own frame. Manifold's marching tetrahedra: a plane field is met
/// exactly where it is linear, so flat faces stay flat.
///
/// # Errors
/// [`KitError::Shape`] if the bounds are empty or the grid is not
/// positive.
pub fn level_set(
    sdf: &(dyn Fn([f64; 3]) -> f64 + Sync),
    lo: [f64; 3],
    hi: [f64; 3],
    edge_mm: f64,
) -> Result<TriMesh, KitError> {
    let ok = edge_mm > 0.0 && (0..3).all(|k| hi[k] > lo[k]);
    if !ok {
        return Err(KitError::Shape("level set: empty bounds or grid".into()));
    }
    let s = dt_geom::Solid::level_set(sdf, Bounds { min: lo, max: hi }, edge_mm);
    s.status().map_err(KitError::Mesh)?;
    Ok(s.to_tri_mesh())
}

/// **hull**: the convex hull of `points`, outward wound.
///
/// # Errors
/// [`KitError::Shape`] for fewer than four points or a flat set.
pub fn hull(points: &[[f64; 3]]) -> Result<TriMesh, KitError> {
    if points.len() < 4 {
        return Err(KitError::Shape("hull: fewer than four points".into()));
    }
    // parry works in f32; centre first so the rounding is relative to the
    // form's size, not its place on the tile.
    let n = points.len() as f64;
    let c = [0, 1, 2].map(|k| points.iter().map(|p| p[k]).sum::<f64>() / n);
    #[allow(clippy::cast_possible_truncation)]
    let pts: Vec<parry3d::math::Vector> = points
        .iter()
        .map(|p| {
            parry3d::math::Vector::new(
                (p[0] - c[0]) as f32,
                (p[1] - c[1]) as f32,
                (p[2] - c[2]) as f32,
            )
        })
        .collect();
    let (v, t) = parry3d::transformation::convex_hull(&pts);
    if t.len() < 4 {
        return Err(KitError::Shape("hull: the points are flat".into()));
    }
    let mut mesh = TriMesh {
        positions: v
            .iter()
            .map(|p| {
                [
                    f64::from(p.x) + c[0],
                    f64::from(p.y) + c[1],
                    f64::from(p.z) + c[2],
                ]
            })
            .collect(),
        triangles: t,
        ..TriMesh::default()
    };
    if crate::signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    Ok(mesh)
}

/// **solid**: a closed, outward-wound triangle mesh as a leaf solid under
/// `slot`.
///
/// # Errors
/// [`KitError::Mesh`] if the mesh is not a closed 2-manifold.
pub fn solid(slot: Slot, mesh: &TriMesh) -> Result<Solid, KitError> {
    leaf(slot, mesh)
}

/// **clip**: `solid` ∩ the box `[lo, hi]`. The box is imported under
/// `slot` (give it the solid's material), so the cut faces are traced.
/// A solid already inside the box is returned as it is.
///
/// # Errors
/// The kernel's status.
pub fn clip(solid: &Solid, slot: Slot, lo: [f64; 3], hi: [f64; 3]) -> Result<Solid, KitError> {
    let Some(b) = solid.0.bounds() else {
        return Ok(solid.clone());
    };
    if (0..3).all(|k| b.min[k] >= lo[k] && b.max[k] <= hi[k]) {
        return Ok(solid.clone());
    }
    let corner = |i: usize| [0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] });
    let boxm = TriMesh {
        positions: (0..8).map(corner).collect(),
        triangles: dt_geom::BOX_TRIANGLES.to_vec(),
        ..TriMesh::default()
    };
    let cutter = leaf(slot, &boxm)?;
    let out = solid.0.intersection(&cutter.0);
    out.status().map_err(KitError::Mesh)?;
    Ok(Solid(out))
}

/// The common stone relief's settings (`stone/finish.py`'s
/// `stone_relief`, the project's one stone texture: a calm plateau carved
/// down into saturating worn recesses, a gentle dish and a fine tooth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Relief {
    /// Noise scale of the carve, mm.
    pub scale_mm: f64,
    /// Depth of the worn recesses, mm.
    pub carve_mm: f64,
    /// Where on the noise a recess starts.
    pub threshold: f64,
    /// How fast a recess saturates.
    pub band: f64,
    /// The gentle two-sided dish, mm.
    pub dish_mm: f64,
    /// The dish's noise scale, mm (`None`: the form's size / 1.6).
    pub dish_scale_mm: Option<f64>,
    /// Octaves of the carve noise.
    pub octaves: u32,
    /// Fade the displacement to zero this far above the form's lowest
    /// point (a buried base keeps its shape).
    pub base_fade_mm: Option<f64>,
    /// The fine tooth for the drybrush, mm.
    pub grain_mm: f64,
    /// The tooth's noise scale, mm.
    pub grain_scale_mm: f64,
}

impl Default for Relief {
    fn default() -> Self {
        Self {
            scale_mm: 7.0,
            carve_mm: 0.9,
            threshold: 0.05,
            band: 0.35,
            dish_mm: 0.35,
            dish_scale_mm: None,
            octaves: 4,
            base_fade_mm: None,
            grain_mm: 0.10,
            grain_scale_mm: 1.5,
        }
    }
}

/// **relief**: the common stone relief on `mesh`, in place. Vertices move
/// along Taubin-smoothed normals, damped where the surface is curved
/// (normal relief folds where it exceeds the local concave radius); the
/// noise domain is rotated at random so faces do not trace the lattice.
/// `seed` drives every draw. Port of `stone/finish.py:stone_relief`.
pub fn relief(mesh: &mut TriMesh, seed: u64, r: &Relief) {
    relief_weighted(mesh, seed, r, &|_, _| 1.0);
}

/// **relief weighted**: [`relief`] with the displacement of each vertex
/// scaled by `weight(position, smoothed normal)` (positions as given,
/// before any displacement). Walls use it to keep the relief off the
/// faces another stone covers (a carved recess under a joint is a void
/// whose roof cannot print) and for fieldstone's patchy calm/active
/// envelope.
pub fn relief_weighted(
    mesh: &mut TriMesh,
    seed: u64,
    r: &Relief,
    weight: &(dyn Fn([f64; 3], [f64; 3]) -> f64 + Sync),
) {
    relief_along(mesh, seed, r, weight, None);
}

/// **relief radial**: [`relief_weighted`], but every vertex moves along
/// the ray from `centre` instead of its normal. On a body star-shaped
/// about `centre` the skin cannot fold (each ray still meets it once),
/// however thin its triangles. Fieldstone's pebbles use it (their pole
/// caps pack latitude rings closer than the relief is deep).
pub fn relief_radial(
    mesh: &mut TriMesh,
    seed: u64,
    r: &Relief,
    centre: [f64; 3],
    weight: &(dyn Fn([f64; 3], [f64; 3]) -> f64 + Sync),
) {
    relief_along(mesh, seed, r, weight, Some(centre));
}

fn relief_along(
    mesh: &mut TriMesh,
    seed: u64,
    r: &Relief,
    weight: &(dyn Fn([f64; 3], [f64; 3]) -> f64 + Sync),
    centre: Option<[f64; 3]>,
) {
    if mesh.positions.is_empty() {
        return;
    }
    let p = mesh.positions.clone();
    let relaxed = taubin(&p, &mesh.triangles, 10);
    let vn = vertex_normals(&relaxed, &mesh.triangles);
    let mut rng = dt_geom::Rng::new(dt_geom::Seed::new(seed));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let nseed = (rng.random() * 2_147_483_648.0) as u64;
    let rot = random_rotation(&mut rng);
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for q in &p {
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let foot = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    let dsc = r.dish_scale_mm.unwrap_or(foot / 1.6).max(1e-6);
    let zmin = lo[2];
    for (i, q) in p.iter().enumerate() {
        let pr = mat3(&rot, *q);
        let n = fbm3(pr, nseed, r.scale_mm, r.octaves);
        let t = ((n - r.threshold) / r.band).clamp(0.0, 1.0);
        let mut carve = -r.carve_mm * t * t * (3.0 - 2.0 * t);
        if r.dish_mm > 0.0 {
            carve += r.dish_mm * fbm3(pr, nseed.wrapping_add(7919), dsc, 2);
        }
        if r.grain_mm > 0.0 {
            carve += r.grain_mm * fbm3(pr, nseed.wrapping_add(31337), r.grain_scale_mm, 2);
        }
        if let Some(f) = r.base_fade_mm {
            carve *= ((q[2] - zmin) / f).clamp(0.0, 1.0);
        }
        let cd = dist(*q, relaxed[i]);
        let damp = 1.0 / (1.0 + (cd / 0.35) * (cd / 0.35));
        let mut d = carve * damp * weight(*q, vn[i]);
        let dir = match centre {
            Some(c) => {
                let v = [0, 1, 2].map(|k| q[k] - c[k]);
                let l = dist(*q, c).max(1e-9);
                // never through the centre
                d = d.max(-0.5 * l);
                v.map(|x| x / l)
            }
            None => vn[i],
        };
        mesh.positions[i] = [0, 1, 2].map(|k| q[k] + dir[k] * d);
    }
}

/// **subdivide**: every triangle split in four at its edge midpoints
/// (shared midpoints, so a closed mesh stays closed). Port of trimesh's
/// `subdivide`, which `stone_relief(refine=1)` runs first.
#[must_use]
pub fn subdivide(mesh: &TriMesh) -> TriMesh {
    let mut positions = mesh.positions.clone();
    let mut mid: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    let mut triangles = Vec::with_capacity(4 * mesh.triangles.len());
    let mut midpoint = |a: u32, b: u32, positions: &mut Vec<[f64; 3]>| -> u32 {
        let key = (a.min(b), a.max(b));
        *mid.entry(key).or_insert_with(|| {
            let (p, q) = (positions[a as usize], positions[b as usize]);
            positions.push([0, 1, 2].map(|k| 0.5 * (p[k] + q[k])));
            u32::try_from(positions.len() - 1).unwrap_or(u32::MAX)
        })
    };
    for t in &mesh.triangles {
        let [a, b, c] = *t;
        let ab = midpoint(a, b, &mut positions);
        let bc = midpoint(b, c, &mut positions);
        let ca = midpoint(c, a, &mut positions);
        triangles.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
    }
    TriMesh {
        positions,
        triangles,
        ..TriMesh::default()
    }
}

/// A uniformly random rotation (from a random unit quaternion).
fn random_rotation(rng: &mut dt_geom::Rng) -> [[f64; 3]; 3] {
    let mut q = [0.0; 4];
    for v in &mut q {
        *v = rng.normal(0.0, 1.0);
    }
    let l = q.iter().map(|x| x * x).sum::<f64>().sqrt() + 1e-12;
    let [w, x, y, z] = q.map(|v| v / l);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn mat3(m: &[[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2])
}

/// Area-weighted vertex normals (unit; zero for an isolated vertex).
#[must_use]
pub fn vertex_normals(p: &[[f64; 3]], tris: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let mut n = vec![[0.0; 3]; p.len()];
    for t in tris {
        let [a, b, c] = t.map(|i| p[i as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let f = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in t {
            for k in 0..3 {
                n[i as usize][k] += f[k];
            }
        }
    }
    for v in &mut n {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 {
            *v = v.map(|x| x / l);
        }
    }
    n
}

/// Taubin smoothing (λ = 0.5, μ = −0.53, umbrella weights), as trimesh's
/// `filter_taubin` defaults.
fn taubin(p: &[[f64; 3]], tris: &[[u32; 3]], iterations: usize) -> Vec<[f64; 3]> {
    let mut nb: Vec<Vec<u32>> = vec![Vec::new(); p.len()];
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            nb[a as usize].push(b);
            nb[b as usize].push(a);
        }
    }
    for l in &mut nb {
        l.sort_unstable();
        l.dedup();
    }
    let mut cur = p.to_vec();
    let step = |cur: &[[f64; 3]], f: f64| -> Vec<[f64; 3]> {
        cur.iter()
            .enumerate()
            .map(|(i, q)| {
                let l = &nb[i];
                if l.is_empty() {
                    return *q;
                }
                #[allow(clippy::cast_precision_loss)]
                let w = 1.0 / l.len() as f64;
                let mut avg = [0.0; 3];
                for &j in l {
                    for k in 0..3 {
                        avg[k] += cur[j as usize][k] * w;
                    }
                }
                [0, 1, 2].map(|k| q[k] + f * (avg[k] - q[k]))
            })
            .collect()
    };
    for it in 0..iterations {
        cur = step(&cur, if it % 2 == 0 { 0.5 } else { -0.53 });
    }
    cur
}

/// **fbm3**: isotropic 3D value-noise fBm at `scale_mm`, about −1..1
/// (`stone/noise.py:fbm`: smoothstep-interpolated lattice values, gain ½,
/// lacunarity 2). Position-keyed, so it is the same at every resolution.
#[must_use]
pub fn fbm3(p: [f64; 3], seed: u64, scale_mm: f64, octaves: u32) -> f64 {
    let (mut total, mut amp, mut sc, mut norm) = (0.0, 1.0, scale_mm, 0.0);
    for o in 0..octaves {
        total += amp * value3(p, seed.wrapping_add(101 * u64::from(o)), sc);
        norm += amp;
        amp *= 0.5;
        sc /= 2.0;
    }
    if norm > 0.0 { total / norm } else { 0.0 }
}

fn value3(p: [f64; 3], seed: u64, scale: f64) -> f64 {
    let q = p.map(|v| v / scale);
    let i = q.map(libm::floor);
    let f = [0, 1, 2].map(|k| q[k] - i[k]);
    let u = f.map(|t| t * t * (3.0 - 2.0 * t));
    #[allow(clippy::cast_possible_truncation)]
    let ii = i.map(|v| v as i64);
    let c = |dx: i64, dy: i64, dz: i64| lattice3(ii[0] + dx, ii[1] + dy, ii[2] + dz, seed);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), u[0]);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), u[0]);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), u[0]);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), u[0]);
    lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2])
}

/// `_hash_lattice`: an integer-lattice hash to −1..1.
fn lattice3(ix: i64, iy: i64, iz: i64, seed: u64) -> f64 {
    #[allow(clippy::cast_possible_wrap)]
    let mut n = ix.wrapping_mul(73_856_093)
        ^ iy.wrapping_mul(19_349_663)
        ^ iz.wrapping_mul(83_492_791)
        ^ (seed as i64);
    n &= 0x7FFF_FFFF_FFFF_FFFF;
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n &= 0x7FFF_FFFF_FFFF_FFFF;
    n ^= n >> 16;
    #[allow(clippy::cast_precision_loss)]
    let v = (n & 0xFF_FFFF) as f64 / f64::from(0x7F_FFFF_u32);
    v - 1.0
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_box_level_set_is_closed_and_flat_faced() {
        let m = level_set(
            &|p| {
                let d = [p[0].abs() - 2.0, p[1].abs() - 1.5, p[2].abs() - 1.0];
                -d.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            },
            [-3.0; 3],
            [3.0; 3],
            0.25,
        )
        .unwrap();
        assert!(m.is_watertight());
        let v = crate::signed_volume(&m);
        assert!((v - 24.0).abs() < 1.0, "{v}");
    }

    #[test]
    fn a_hull_is_outward_and_closed() {
        let pts: Vec<[f64; 3]> = (0..8)
            .map(|i| [0, 1, 2].map(|k| f64::from(i >> k & 1) * 2.0 + 10.0))
            .collect();
        let m = hull(&pts).unwrap();
        assert!(m.is_watertight());
        assert!((crate::signed_volume(&m) - 8.0).abs() < 1e-4);
    }

    #[test]
    fn noise_is_bounded_and_position_keyed() {
        for i in 0..200 {
            let p = [f64::from(i) * 0.37, 1.3, -2.0];
            let v = fbm3(p, 7, 3.0, 4);
            assert!((-1.0..=1.0).contains(&v));
            assert_eq!(v.to_bits(), fbm3(p, 7, 3.0, 4).to_bits());
        }
    }
}
