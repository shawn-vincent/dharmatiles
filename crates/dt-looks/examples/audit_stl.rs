//! Run the printability gate on binary STL files (a measuring tool for
//! the parity work: Python meshes through the Rust audit).
//!
//! `cargo run --release -p dt-looks --example audit_stl -- FILE.stl [W H]`
//! prints triangles, watertightness, overhang mm² and, with `--split Z`,
//! the overhang above and below `Z` mm.

use dt_kit::{Material, Printer, TriMesh, View, audit, paint, render};

fn read_stl(path: &str) -> Result<TriMesh, String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if b.len() < 84 {
        return Err(format!("{path}: not a binary STL"));
    }
    let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
    let mut positions = Vec::with_capacity(3 * n);
    let mut triangles = Vec::with_capacity(n);
    for t in 0..n {
        let o = 84 + 50 * t + 12;
        let f = |k: usize| {
            let i = o + 4 * k;
            f64::from(f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]))
        };
        let base = u32::try_from(positions.len()).map_err(|e| e.to_string())?;
        for v in 0..3 {
            positions.push([f(3 * v), f(3 * v + 1), f(3 * v + 2)]);
        }
        triangles.push([base, base + 1, base + 2]);
    }
    // merge identical positions so the audit sees a connected mesh
    let mut map = std::collections::HashMap::new();
    let mut uniq = Vec::new();
    let remap: Vec<u32> = positions
        .iter()
        .map(|p| {
            let key = p.map(f64::to_bits);
            *map.entry(key).or_insert_with(|| {
                uniq.push(*p);
                u32::try_from(uniq.len() - 1).unwrap_or(u32::MAX)
            })
        })
        .collect();
    Ok(TriMesh {
        positions: uniq,
        triangles: triangles
            .iter()
            .map(|t| t.map(|v| remap[v as usize]))
            .collect(),
        ..TriMesh::default()
    })
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split: Option<f64> = args
        .iter()
        .position(|a| a == "--split")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());
    let nums: Vec<f64> = args.iter().filter_map(|a| a.parse().ok()).collect();
    for path in args.iter().filter(|a| a.ends_with(".stl")) {
        let m = read_stl(path)?;
        let fp = if nums.len() >= 2 && split.is_none() {
            [nums[0], nums[1]]
        } else {
            [35.0, 35.0]
        };
        let (ex, a) = audit(&m, fp, &Printer::new(0.45, 0.2, 55.0, 0.2));
        let mut line = format!(
            "{path}: {} tris, watertight {}, bodies {}, overhang {:.1} mm²",
            a.triangles, a.watertight, a.bodies, a.overhang_mm2
        );
        if args.iter().any(|a| a == "--where") {
            let p65 = Printer {
                overhang_deg: 65.0,
                ..Printer::new(0.45, 0.2, 55.0, 0.2)
            };
            let (e65, a65) = audit(&m, fp, &p65);
            for (t, bad) in e65.mesh.triangles.iter().zip(&a65.overhang) {
                if *bad {
                    let p = t.map(|i| e65.mesh.positions[i as usize]);
                    println!("  >65 at {:.2?}", p[0]);
                }
            }
        }
        if let Some(z) = split {
            let (mut lo, mut hi) = (0.0, 0.0);
            for (t, bad) in ex.mesh.triangles.iter().zip(&a.overhang) {
                if !bad {
                    continue;
                }
                let p = t.map(|i| ex.mesh.positions[i as usize]);
                let u = [0, 1, 2].map(|k| p[1][k] - p[0][k]);
                let v = [0, 1, 2].map(|k| p[2][k] - p[0][k]);
                let c = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let area = 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                if (p[0][2] + p[1][2] + p[2][2]) / 3.0 > z {
                    hi += area;
                } else {
                    lo += area;
                }
            }
            line += &format!(" (below {z}: {lo:.1}, above: {hi:.1})");
        }
        println!("{line}");
        if args.iter().any(|a| a == "--red") {
            for (t, bad) in ex.mesh.triangles.iter().zip(&a.overhang) {
                if *bad {
                    let p = t.map(|i| ex.mesh.positions[i as usize]);
                    let c = [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0);
                    println!("red {:.2} {:.2} {:.2}", c[0], c[1], c[2]);
                }
            }
        }
        if args.iter().any(|a| a == "--png") {
            let mats = vec![Material::Leaf; ex.mesh.triangles.len()];
            let colours = paint(&mats, &a.overhang);
            for (name, elev, azim) in [
                ("side", 10.0, -120.0),
                ("below", -35.0, -120.0),
                ("above", 45.0, 60.0),
            ] {
                let v = View {
                    name,
                    elev,
                    azim,
                    close: false,
                };
                let img = render(&ex.mesh, &colours, &v, fp, (900, 760));
                let out = format!("{}-{name}.png", path.trim_end_matches(".stl"));
                std::fs::write(&out, img.png()?).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
