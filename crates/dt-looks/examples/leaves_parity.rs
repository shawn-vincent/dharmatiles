//! P5 leaves: the Rust placer against full Python captures, and the
//! overhang audit of both leaf sets on the same clumps.
//!
//! ```text
//! for s in 1 2 3 4; do python tests/parity/capture/leaves.py out/leaves/seed$s $s; done
//! cargo run --release -p dt-looks --example leaves_parity -- out/leaves/seed1 [...]
//! ```
//!
//! Per capture: Python vs Rust leaf count, roots, failed builds, branch
//! culls, uncovered test points and time (Rust replaying Python's samples,
//! at all threads and at one; and with its own seeded samples); how many
//! leaves match Python's root and direction; then the kit audit (rockpile's
//! layer-reach rule at 55°, sub-gap voids filled) on wood + foliage +
//! leaves unioned in the kernel, with the overhang area split by part.
//! The foliage is tagged `Masonry` here only so the audit can tell it from
//! the leaves.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a report harness"
)]

#[path = "../tests/fixture/mod.rs"]
mod fixture;

use std::time::Instant;

use dt_kit::{Frame, Material, Printer, Scene, Seed, Tag, TriMesh, audit, form};
use dt_looks::tree::leaves::{LeafStats, place, place_with};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    let audit_on = !dirs.iter().any(|d| d == "--no-audit");
    for dir in dirs.iter().filter(|d| !d.starts_with("--")) {
        let bytes = std::fs::read(format!("{dir}/leaves.bin"))?;
        let c = fixture::read(&bytes)?;
        let foliage = c.mesh("foliage");
        let wood = c.mesh("wood");
        let shape = c.shape();
        let draws = c.draws();
        let [placed, roots, failed, culled, uncovered] = c.py_counts();
        let py_s = c.f64s("py/seconds")[0];

        // Best of three, all threads and one.
        let mut best = f64::INFINITY;
        let mut out = None;
        for _ in 0..3 {
            let t = Instant::now();
            let r = place_with(&foliage, &wood, &shape, &draws).map_err(|e| e.message)?;
            best = best.min(t.elapsed().as_secs_f64());
            out = Some(r);
        }
        let (leaves, stats) = out.ok_or("no run")?;
        let one = rayon::ThreadPoolBuilder::new().num_threads(1).build()?;
        let mut best1 = f64::INFINITY;
        for _ in 0..2 {
            let t = Instant::now();
            one.install(|| place_with(&foliage, &wood, &shape, &draws))
                .map_err(|e| e.message)?;
            best1 = best1.min(t.elapsed().as_secs_f64());
        }
        let printable = dt_looks::tree::leaves::LeafShape {
            overhang_deg: Some(55.0),
            ..shape
        };
        let (fixed, fs) = place_with(&foliage, &wood, &printable, &draws).map_err(|e| e.message)?;
        let (_, ns) = place(&foliage, &wood, &printable, Seed::new(1)).map_err(|e| e.message)?;

        let py: Vec<([f64; 3], [f64; 3])> = c
            .points("py/base")
            .into_iter()
            .zip(c.points("py/dir"))
            .collect();
        let matched = matches(&stats, &py);
        println!("== {dir}");
        println!(
            "python  placed {placed:5} roots {roots:5} failed {failed:4} culled {culled:4} uncovered {uncovered:3} time {py_s:.3}s"
        );
        row("rust/py", &stats, best);
        println!("rust/py 1 thread {best1:.3}s");
        row("rust/own", &ns, ns.times.total);
        row("fixed", &fs, fs.times.total);
        println!(
            "leaf-for-leaf: {matched} of {} Rust leaves at a Python root with Python's direction; count diff {:+.1}% (own draws {:+.1}%)",
            stats.placed,
            100.0 * (stats.placed as f64 / f64::from(placed) - 1.0),
            100.0 * (ns.placed as f64 / f64::from(placed) - 1.0),
        );
        println!("stages: {:?}", stats.times);

        if audit_on {
            let py_leaves: Vec<TriMesh> = c
                .py_leaf_positions()
                .into_iter()
                .map(|v| relayout(&v, &leaves))
                .collect();
            for (name, set, st) in [
                ("python", &py_leaves, None),
                ("rust", &leaves, Some(&stats)),
                ("fixed", &fixed, Some(&fs)),
            ] {
                let t = Instant::now();
                let r = overhang(&foliage, &wood, set, st)?;
                println!(
                    "audit {name:6}: overhang leaves {:7.1} mm²  foliage {:7.1}  wood {:6.1}  (watertight {}, {:.1}s)",
                    r.0,
                    r.1,
                    r.2,
                    r.3,
                    t.elapsed().as_secs_f64()
                );
            }
        }
    }
    Ok(())
}

fn row(name: &str, s: &LeafStats, secs: f64) {
    println!(
        "{name:7} placed {:5} roots {:5} failed {:4} culled {:4} uncovered {:3} time {secs:.3}s  (oval {} skew {} neck {} error {}; tests {})",
        s.placed,
        s.roots,
        s.failed(),
        s.branch_culled,
        s.uncovered,
        s.failed_oval,
        s.failed_skew,
        s.failed_neck,
        s.failed_error,
        s.test_points,
    );
}

fn matches(s: &LeafStats, py: &[([f64; 3], [f64; 3])]) -> usize {
    let key = |p: [f64; 3]| p.map(|x| (x * 1e4).round() as i64);
    let map: std::collections::HashMap<[i64; 3], [f64; 3]> =
        py.iter().map(|(b, d)| (key(*b), *d)).collect();
    s.leaves
        .iter()
        .map(|l| (l.root, l.tangent))
        .filter(|(b, d)| {
            map.get(&key(*b))
                .is_some_and(|pd| d[0] * pd[0] + d[1] * pd[1] + d[2] * pd[2] > 0.999_99)
        })
        .count()
}

/// A Python leaf's vertices (blade then oval, the same layout) with the
/// Rust solid's triangles: Python's winding follows a per-topology cache
/// and is not always consistent, the geometry is the same.
fn relayout(v: &[[f64; 3]], any: &[TriMesh]) -> TriMesh {
    let mut m = TriMesh {
        positions: v.to_vec(),
        triangles: any.first().map(|m| m.triangles.clone()).unwrap_or_default(),
        ..TriMesh::default()
    };
    if dt_kit::signed_volume(&m) < 0.0 {
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
    }
    m
}

/// Overhang (mm²) of leaves, foliage and wood after the kernel union.
fn overhang(
    foliage: &TriMesh,
    wood: &TriMesh,
    leaves: &[TriMesh],
    detail: Option<&LeafStats>,
) -> Result<(f64, f64, f64, bool), Box<dyn std::error::Error>> {
    let frame = Frame::for_system("db", 1, 1);
    let mut scene = Scene::new(frame, Seed::new(0));
    let tag = |k: &'static str| Tag {
        kind: k,
        owner: "tree".into(),
    };
    let s = scene.slots(1, Material::Masonry)?;
    scene.add(tag("foliage"), form::solid(s.get(0), foliage)?);
    let bodies = split(wood);
    let s = scene.slots(bodies.len() as u32, Material::Wood)?;
    for (i, b) in bodies.iter().enumerate() {
        scene.add(tag("wood"), form::solid(s.get(i as u32), b)?);
    }
    let s = scene.slots(leaves.len() as u32, Material::Leaf)?;
    for (i, l) in leaves.iter().enumerate() {
        scene.add(tag("leaf"), form::solid(s.get(i as u32), l)?);
    }
    let (mesh, mats) = scene.union()?;
    let printer = Printer::new(0.45, 0.2, 55.0, 0.2);
    let (export, a) = audit(&mesh, frame.size_mm(), &printer);
    let mut by = [0.0; 3];
    let ids = mesh.triangle_original_ids().unwrap_or_default();
    let fol_id = mats
        .iter()
        .position(|m| *m == Material::Masonry)
        .map_or(0, |k| ids[k]);
    let nwood = bodies.len() as u32;
    // [part][zone]: part 0 blade top, 1 oval, 2 wall; zone 0 flush (< 0.5), 1 pitched.
    let mut cell = [[0.0; 2]; 3];
    let mut by_nz = [0.0; 8];
    let mut by_face = [0.0; 8];
    let mut by_ring = [[0.0_f64; 2]; 12];
    for (k, t) in export.mesh.triangles.iter().enumerate() {
        if !a.overhang[k] {
            continue;
        }
        let [p, q, r] = t.map(|i| export.mesh.positions[i as usize]);
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        let cx = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let area = 0.5 * (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
        match mats[export.source[k] as usize] {
            Material::Leaf => {
                by[0] += area;
                if let Some(st) = detail {
                    let li = (ids[export.source[k] as usize] - fol_id - 1 - nwood) as usize;
                    let c = [0, 1, 2].map(|d| (p[d] + q[d] + r[d]) / 3.0);
                    let (part, fk) = nearest_part(&leaves[li], c);
                    let lf = &st.leaves[li];
                    cell[part][usize::from(lf.zone >= 0.5)] += area;
                    let b = (((lf.normal[2] + 1.0) / 2.0 * 8.0) as usize).min(7);
                    by_nz[b] += area;
                    let fz = cx[2] / (2.0 * area).max(1e-12);
                    let fb = (((fz + 1.0) / 2.0 * 8.0) as usize).min(7);
                    by_face[fb] += area;
                    if part == 0 && lf.zone < 0.5 {
                        // blade face: ring 0 = base fan, 1..=10 body bands, 11 tip fan
                        let ring = if fk < 10 {
                            0
                        } else if fk >= 210 {
                            11
                        } else {
                            1 + (fk - 10) / 20
                        };
                        let edge = (10..210).contains(&fk) && matches!(((fk - 10) % 20) / 2, 0 | 9);
                        by_ring[ring][usize::from(edge)] += area;
                    }
                }
            }
            Material::Masonry => by[1] += area,
            _ => by[2] += area,
        }
    }
    if detail.is_some() {
        println!(
            "  leaf overhang by part (flush / pitched): blade {:.1} / {:.1}, oval {:.1} / {:.1}, wall {:.1} / {:.1}",
            cell[0][0], cell[0][1], cell[1][0], cell[1][1], cell[2][0], cell[2][1]
        );
        let s: Vec<String> = by_nz
            .iter()
            .enumerate()
            .map(|(b, a)| {
                format!(
                    "{:+.2}..{:+.2}: {a:.1}",
                    -1.0 + 0.25 * b as f64,
                    -0.75 + 0.25 * b as f64
                )
            })
            .collect();
        println!("  leaf overhang by root normal z: {}", s.join(", "));
        let s: Vec<String> = by_face
            .iter()
            .enumerate()
            .map(|(b, a)| {
                format!(
                    "{:+.2}..{:+.2}: {a:.1}",
                    -1.0 + 0.25 * b as f64,
                    -0.75 + 0.25 * b as f64
                )
            })
            .collect();
        println!("  leaf overhang by face normal z: {}", s.join(", "));
        let s: Vec<String> = by_ring
            .iter()
            .map(|r| format!("{:.0}/{:.0}", r[0], r[1]))
            .collect();
        println!(
            "  flush blade overhang by ring base→tip (inner/edge): {}",
            s.join(" ")
        );
    }
    Ok((by[0], by[1], by[2], a.watertight))
}

/// Which part of a leaf solid a point lies on: 0 blade, 1 oval, 2 wall,
/// and the nearest triangle.
fn nearest_part(leaf: &TriMesh, c: [f64; 3]) -> (usize, usize) {
    let mut best = (f64::INFINITY, 0);
    for (k, t) in leaf.triangles.iter().enumerate() {
        let m = [0, 1, 2].map(|d| {
            t.iter()
                .map(|&i| leaf.positions[i as usize][d])
                .sum::<f64>()
                / 3.0
        });
        let d = (0..3).map(|i| (m[i] - c[i]).powi(2)).sum::<f64>();
        if d < best.0 {
            best = (d, k);
        }
    }
    let part = match best.1 {
        k if k < 220 => 0,
        k if k < 440 => 1,
        _ => 2,
    };
    (part, best.1)
}

/// Connected bodies (each Python tube is one).
fn split(m: &TriMesh) -> Vec<TriMesh> {
    let n = m.positions.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for t in &m.triangles {
        let a = find(&mut parent, t[0] as usize);
        for &v in &t[1..] {
            let b = find(&mut parent, v as usize);
            parent[b] = a;
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<[u32; 3]>> = Default::default();
    for t in &m.triangles {
        groups
            .entry(find(&mut parent, t[0] as usize))
            .or_default()
            .push(*t);
    }
    groups
        .into_values()
        .map(|ts| {
            let mut map = std::collections::HashMap::new();
            let mut positions = Vec::new();
            let triangles = ts
                .iter()
                .map(|t| {
                    t.map(|i| {
                        *map.entry(i).or_insert_with(|| {
                            positions.push(m.positions[i as usize]);
                            (positions.len() - 1) as u32
                        })
                    })
                })
                .collect();
            TriMesh {
                positions,
                triangles,
                ..TriMesh::default()
            }
        })
        .collect()
}
