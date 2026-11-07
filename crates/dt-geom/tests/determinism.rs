//! Determinism, with material tracing: a small pipeline (noise, scatter,
//! EDT, then level-set rocks trimmed by planes and one batch union) run with
//! rayon at 1, 4 and 16 threads, twice each, with every stage hashed.
//! Go: byte-identical at every thread count, and every output triangle
//! traced to a material. `--nocapture` prints the hashes, so platforms can
//! be compared.
//!
//! Rules every pipeline follows: every random draw comes
//! from a keyed [`Seed`], never a shared RNG; parallel work writes to its
//! own slot and is collected in index order; source IDs follow
//! `dt_geom::source` (a reserved block, leaves relabelled by index, cuts
//! with IDs we give them, a material table keyed by original and face ID).
//! Manifold's own parallelism (TBB) is whatever the build has; the default
//! build has it on. Its thread count is not set here.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::float_cmp
)]

use dt_geom::{BoolOp, Bounds, Fnv, IdBlock, Materials, Seed, Solid, TriMesh, batch_in_order};
use rayon::prelude::*;

const N: usize = 384; // grid cells per side
const CELL: f64 = 0.125; // mm per cell (48 mm square)

fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn lattice(seed: Seed, ix: i64, iy: i64) -> f64 {
    unit(seed.index(ix as u64).index(iy as u64).value())
}

/// Value noise, arithmetic only (+, ×, floor): exact on every IEEE platform.
fn value_noise(seed: Seed, x: f64, y: f64) -> f64 {
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i64, fy as i64);
    let (tx, ty) = (x - fx, y - fy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let a = lattice(seed, ix, iy);
    let b = lattice(seed, ix + 1, iy);
    let c = lattice(seed, ix, iy + 1);
    let d = lattice(seed, ix + 1, iy + 1);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

fn fbm(seed: Seed, x: f64, y: f64) -> f64 {
    let (mut sum, mut amp, mut f) = (0.0, 0.5, 1.0 / 16.0);
    for o in 0..5 {
        sum += amp * value_noise(seed.index(o), x * f, y * f);
        amp *= 0.5;
        f *= 2.0;
    }
    sum
}

fn hash_f64s(v: &[f64]) -> u64 {
    let mut h = Fnv::default();
    v.iter().for_each(|x| h.u64(x.to_bits()));
    h.finish()
}

/// 1D squared distance transform (Felzenszwalb and Huttenlocher, 2012).
fn edt_1d(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let mut first = true;
    for q in 0..n {
        if f[q].is_infinite() {
            continue;
        }
        if first {
            v[0] = q;
            first = false;
            continue;
        }
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f64::INFINITY;
                break;
            }
        }
    }
    if first {
        out.fill(f64::INFINITY);
        return;
    }
    let mut j = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[j + 1] < q as f64 {
            j += 1;
        }
        let p = v[j];
        *o = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// Exact Euclidean distance (in cells) to the nearest `true` cell.
fn edt(mask: &[bool]) -> Vec<f64> {
    let mut g: Vec<f64> = mask
        .iter()
        .map(|&m| if m { 0.0 } else { f64::INFINITY })
        .collect();
    // rows, then columns: each line independent, so parallel and exact
    g.par_chunks_mut(N).for_each(|row| {
        let f = row.to_vec();
        edt_1d(&f, row);
    });
    let cols: Vec<Vec<f64>> = (0..N)
        .into_par_iter()
        .map(|x| {
            let f: Vec<f64> = (0..N).map(|y| g[y * N + x]).collect();
            let mut o = vec![0.0; N];
            edt_1d(&f, &mut o);
            o
        })
        .collect();
    let mut out = vec![0.0; N * N];
    for (x, col) in cols.iter().enumerate() {
        for (y, &d) in col.iter().enumerate() {
            out[y * N + x] = d.sqrt();
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Hashes {
    noise: u64,
    noise_libm: u64,
    scatter: u64,
    edt: u64,
    rocks: u64,
    union: u64,
    union_tris: usize,
    materials: u64,
    traced: [usize; 4],
}

const GROUND: u32 = 0;
const STONE: u32 = 1;
const MOSS: u32 = 2;

/// A rock's own material field: moss above `c.z + 0.3 r`, stone below.
fn rock_material(c: [f64; 3], r: f64, p: [f64; 3]) -> u32 {
    if p[2] > c[2] + 0.3 * r { MOSS } else { STONE }
}

fn centroid(m: &TriMesh, t: usize) -> [f64; 3] {
    let [a, b, c] = m.triangles[t].map(|i| m.positions[i as usize]);
    [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
}

fn pipeline(root: Seed) -> Hashes {
    // 1. noise: a height field, every cell independent
    let ns = root.key("noise");
    let height: Vec<f64> = (0..N * N)
        .into_par_iter()
        .map(|i| fbm(ns, (i % N) as f64, (i / N) as f64))
        .collect();
    // the same with libm calls (sin, exp): the cross-platform risk the plan names
    let libm: Vec<f64> = height
        .par_iter()
        .enumerate()
        .map(|(i, &h)| (h * 7.3 + i as f64 * 1e-3).sin() * (-h).exp())
        .collect();

    // 2. scatter: one keyed candidate per 16×16-cell block, kept where the
    // noise is high enough; collected in block order
    let ss = root.key("scatter");
    let blocks = N / 16;
    let points: Vec<[f64; 2]> = (0..blocks * blocks)
        .into_par_iter()
        .filter_map(|b| {
            let s = ss.index(b as u64);
            let (bx, by) = ((b % blocks) * 16, (b / blocks) * 16);
            let x = bx as f64 + 2.0 + 12.0 * unit(s.key("x").value());
            let y = by as f64 + 2.0 + 12.0 * unit(s.key("y").value());
            let h = height[(y as usize) * N + x as usize];
            (h > 0.45).then_some([x, y])
        })
        .collect();
    let mut sh = Fnv::default();
    points.iter().flatten().for_each(|v| sh.u64(v.to_bits()));

    // 3. EDT from the scatter points
    let mut mask = vec![false; N * N];
    for p in &points {
        mask[(p[1] as usize) * N + p[0] as usize] = true;
    }
    let dist = edt(&mask);

    // 4. boolean batch: one level-set rock per point (radius from the EDT
    // to its neighbours), cut flat underneath (into the ground) and on top
    // (a sawn face that stays visible), in parallel, then one batch union
    // with the ground slab. Source IDs: the slab is slot 0, rock i owns
    // slots 1 + 3i (body), 2 + 3i (bottom cut), 3 + 3i (top cut).
    let ground_z = 0.0;
    let n_rocks = points.len().min(24);
    let block = IdBlock::reserve(1 + 3 * n_rocks as u32);
    let built: Vec<(Solid, Materials)> = points[..n_rocks]
        .par_iter()
        .enumerate()
        .map(|(i, p)| {
            let s = root.key("rock").index(i as u64);
            let c = [p[0] * CELL, p[1] * CELL, ground_z + 0.5];
            let r = 1.2 + 1.3 * unit(s.value());
            let w = 0.25 * unit(s.key("w").value());
            let sdf = move |q: [f64; 3]| {
                let d =
                    ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2) + (q[2] - c[2]).powi(2)).sqrt();
                r - d + w * (q[0] - c[0]) * (q[1] - c[1])
            };
            let pad = r + 1.0;
            let b = Bounds {
                min: [c[0] - pad, c[1] - pad, c[2] - pad],
                max: [c[0] + pad, c[1] + pad, c[2] + pad],
            };
            let slot = 1 + 3 * i as u32;
            let (body, bottom, top) = (block.id(slot), block.id(slot + 1), block.id(slot + 2));
            // rule 2: relabel the leaf, then record its materials from it
            let leaf = Solid::level_set(&sdf, b, 0.3)
                .with_original_id(body, block.face_base(slot))
                .unwrap();
            let lm = leaf.to_tri_mesh();
            let mut mats = Materials::default();
            mats.set_faces(&lm, |t| rock_material(c, r, centroid(&lm, t)));
            // rule 3 + the cut-face rule: cuts take the rock's base material
            mats.set_id(bottom, STONE);
            mats.set_id(top, STONE);
            let rock = leaf
                .trim_by_plane_with_id([0.0, 0.0, 1.0], ground_z - 0.2, bottom)
                .unwrap()
                .trim_by_plane_with_id([0.0, 0.0, -1.0], -(c[2] + 0.6 * r), top)
                .unwrap();
            (rock, mats)
        })
        .collect();
    let mut rh = Fnv::default();
    for (r, _) in &built {
        rh.u64(r.to_tri_mesh().content_hash());
    }
    let side = N as f64 * CELL;
    let slab = TriMesh {
        positions: (0..8)
            .map(|i| {
                [
                    if i & 1 == 0 { 0.0 } else { side },
                    if i & 2 == 0 { 0.0 } else { side },
                    if i & 4 == 0 { -2.0 } else { ground_z },
                ]
            })
            .collect(),
        triangles: dt_geom::BOX_TRIANGLES.to_vec(),
        run_index: vec![0, 36],
        run_original_id: vec![block.id(0)],
        ..Default::default()
    };
    let mut mats = Materials::default();
    mats.set_id(block.id(0), GROUND);
    let mut parts = vec![Solid::from_tri_mesh(&slab).unwrap()];
    for (rock, m) in built {
        parts.push(rock);
        mats.merge(m);
    }
    // rule 5: combine in index order (checked in debug builds)
    let all = batch_in_order(&parts, BoolOp::Union);
    let um = all.to_tri_mesh();
    assert!(um.is_watertight());

    // every output triangle traces to a material, and the material is
    // right: a rock body's agrees with a fresh sample of its field (within
    // a grid cell of the moss line), a cut face lies on its plane.
    let traced = mats.trace(&um);
    let ids = um.triangle_original_ids().unwrap();
    let (mut untraced, mut body, mut body_agree, mut cut_off_plane) = (0, 0, 0, 0);
    let mut mh = Fnv::default();
    for (t, m) in traced.iter().enumerate() {
        let Some(m) = *m else {
            untraced += 1;
            continue;
        };
        mh.u64(u64::from(m));
        let Some(slot) = block.slot(ids[t]) else {
            untraced += 1;
            continue;
        };
        if slot == 0 {
            continue;
        }
        let i = ((slot - 1) / 3) as usize;
        let p = points[i];
        let rs = root.key("rock").index(i as u64);
        let c = [p[0] * CELL, p[1] * CELL, ground_z + 0.5];
        let r = 1.2 + 1.3 * unit(rs.value());
        let q = centroid(&um, t);
        match (slot - 1) % 3 {
            0 => {
                body += 1;
                let line = c[2] + 0.3 * r;
                body_agree += usize::from(m == rock_material(c, r, q) || (q[2] - line).abs() < 0.3);
            }
            1 => cut_off_plane += usize::from((q[2] - (ground_z - 0.2)).abs() > 1e-9 || m != STONE),
            _ => cut_off_plane += usize::from((q[2] - (c[2] + 0.6 * r)).abs() > 1e-9 || m != STONE),
        }
    }
    let top_cut = ids
        .iter()
        .filter(|&&id| block.slot(id).is_some_and(|s| s > 0 && (s - 1) % 3 == 2))
        .count();
    assert_eq!(untraced, 0, "untraced output triangles");
    assert_eq!(body_agree, body, "rock triangles with the wrong material");
    assert_eq!(
        cut_off_plane, 0,
        "cut-face triangles off their plane or not stone"
    );
    assert!(top_cut > 0, "no top cut face survived the union");
    assert_eq!(mats.conflicts, 0, "a face sampled with two materials");

    Hashes {
        noise: hash_f64s(&height),
        noise_libm: hash_f64s(&libm),
        scatter: sh.finish(),
        edt: hash_f64s(&dist),
        rocks: rh.finish(),
        union: um.content_hash(),
        union_tris: um.triangle_count(),
        materials: mh.finish(),
        traced: [um.triangle_count(), body, top_cut, untraced],
    }
}

fn check(backend: &str) {
    let mut first: Option<Hashes> = None;
    for threads in [1, 4, 16] {
        for run in 0..2 {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            // Shift the kernel's ID counter by a different amount each run:
            // only the order of source IDs may matter, never their values.
            let _ = Solid::reserve_ids(1 + 997 * (threads + run) as u32);
            let h = pool.install(|| pipeline(Seed::new(20_261_101)));
            println!(
                "determinism {backend} {}-{} threads={threads:>2} run={run} noise={:016x} noise_libm={:016x} scatter={:016x} edt={:016x} rocks={:016x} union={:016x} union_tris={} materials={:016x} traced={}/{} body={} top_cut={}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                h.noise,
                h.noise_libm,
                h.scatter,
                h.edt,
                h.rocks,
                h.union,
                h.union_tris,
                h.materials,
                h.traced[0] - h.traced[3],
                h.traced[0],
                h.traced[1],
                h.traced[2]
            );
            match &first {
                None => first = Some(h),
                Some(f) => assert_eq!(f, &h, "{backend}: threads={threads} run={run} differs"),
            }
        }
    }
}

#[test]
fn deterministic() {
    check("manifold");
}
