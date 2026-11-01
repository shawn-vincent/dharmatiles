//! R1 spike: replay every boolean captured from the Python labs
//! (`tests/parity/capture/booleans.py`) on each mesh backend, and compare
//! with what Python's manifold3d produced.
//!
//! ```text
//! cargo run --release -p dt-geom --features manifold-rust --example r1_replay -- \
//!     CAPTURE_DIR CASE [--backend ffi|port] [--limit N] [--csv OUT.csv]
//! ```
//!
//! Add `--features manifold-parallel` for Manifold's internal parallelism
//! (TBB / rayon). Each op's time covers the op plus forcing its result
//! (`triangle_count`), as the capture does; importing operands is untimed.
//! Per op it checks: status, watertightness, triangle count and volume
//! against Python, the result's original IDs and property count, and
//! whether the result is byte-identical to Python's.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::float_cmp
)]

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_geom::{BoolOp, Mesh, TriMesh};

fn read_dtm(path: &Path) -> TriMesh {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(&b[..8], b"DTMESH01", "{}", path.display());
    let mut o = 8;
    let u32_at = |o: &mut usize| {
        let v = u32::from_le_bytes(b[*o..*o + 4].try_into().unwrap());
        *o += 4;
        v
    };
    let num_prop = u32_at(&mut o) as usize;
    o += 4;
    let mut n = [0usize; 8];
    for x in &mut n {
        *x = u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as usize;
        o += 8;
    }
    o += 8; // tolerance (neither backend's import takes it)
    let [n_vert, n_tri, n_merge, n_ri, n_ro, n_rt, n_fi, _n_rf] = n;
    let f64s = |o: &mut usize, k: usize| -> Vec<f64> {
        let v = b[*o..*o + 8 * k]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| f64::from_le_bytes(*c))
            .collect();
        *o += 8 * k;
        v
    };
    let u32s = |o: &mut usize, k: usize| -> Vec<u32> {
        let v = b[*o..*o + 4 * k]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        *o += 4 * k;
        v
    };
    let props = f64s(&mut o, n_vert * num_prop);
    let tv = u32s(&mut o, n_tri * 3);
    let merge_from = u32s(&mut o, n_merge);
    let merge_to = u32s(&mut o, n_merge);
    let run_index = u32s(&mut o, n_ri);
    let run_original_id = u32s(&mut o, n_ro);
    let _run_transform = f64s(&mut o, n_rt);
    let face_id = u32s(&mut o, n_fi);
    let mut positions = Vec::with_capacity(n_vert);
    let mut extra = Vec::new();
    for row in props.chunks_exact(num_prop.max(3)) {
        positions.push([row[0], row[1], row[2]]);
        extra.extend_from_slice(&row[3..]);
    }
    TriMesh {
        positions,
        triangles: tv.as_chunks::<3>().0.to_vec(),
        extra,
        num_extra: num_prop.saturating_sub(3),
        merge_from,
        merge_to,
        run_index,
        run_original_id,
        face_id,
    }
}

/// A small LRU of imported meshes keyed by content hash.
struct Store {
    dir: PathBuf,
    cache: HashMap<String, TriMesh>,
    order: VecDeque<String>,
}

impl Store {
    fn get(&mut self, h: &str) -> TriMesh {
        if let Some(m) = self.cache.get(h) {
            return m.clone();
        }
        let m = read_dtm(&self.dir.join("meshes").join(format!("{h}.dtm")));
        self.cache.insert(h.to_owned(), m.clone());
        self.order.push_back(h.to_owned());
        if self.order.len() > 48 {
            let old = self.order.pop_front().unwrap();
            self.cache.remove(&old);
        }
        m
    }
}

struct Rec {
    id: u64,
    op: String,
    normal: [f64; 3],
    offset: f64,
    py_ms: f64,
    operands: Vec<String>,
    results: Vec<String>,
    result_tris: Vec<usize>,
    result_vol: Vec<f64>,
}

fn parse(line: &str) -> Rec {
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let strs = |k: &str| -> Vec<String> {
        v[k].as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect()
    };
    let p = &v["params"];
    let mut normal = [0.0; 3];
    if let Some(n) = p["normal"].as_array() {
        for (i, x) in n.iter().enumerate() {
            normal[i] = x.as_f64().unwrap();
        }
    }
    Rec {
        id: v["id"].as_u64().unwrap(),
        op: v["op"].as_str().unwrap().to_owned(),
        normal,
        offset: p["offset"].as_f64().unwrap_or(0.0),
        py_ms: v["py_ms"].as_f64().unwrap(),
        operands: strs("operands"),
        results: strs("results"),
        result_tris: v["result_tris"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as usize)
            .collect(),
        result_vol: v["result_vol"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect(),
    }
}

#[derive(Default)]
struct Totals {
    ops: usize,
    failed_import: usize,
    status_err: usize,
    not_watertight: usize,
    tri_mismatch: usize,
    vol_over_1e6: usize,
    max_rel_vol: f64,
    ids_mismatch: usize,
    props_mismatch: usize,
    identical: usize,
    ms: f64,
    py_ms: f64,
    /// Hash of every result's bytes (absolute original IDs excluded), to
    /// compare runs of the same build for determinism (R2).
    hash: dt_geom::Fnv,
}

fn run<S: Mesh>(name: &str, store: &mut Store, recs: &[Rec], csv: &mut dyn Write) -> Totals {
    let mut t = Totals::default();
    for r in recs {
        let ins: Vec<TriMesh> = r.operands.iter().map(|h| store.get(h)).collect();
        let solids: Result<Vec<S>, _> = ins.iter().map(S::from_tri_mesh).collect();
        if let Err(e) = &solids
            && t.failed_import < 3
        {
            eprintln!("import op {}: {e}", r.id);
        }
        let Ok(solids) = solids else {
            t.failed_import += 1;
            writeln!(csv, "{name},{},{},import-failed", r.id, r.op).unwrap();
            continue;
        };
        for s in &solids {
            let _ = s.triangle_count();
        }
        let t0 = Instant::now();
        let outs: Vec<S> = match r.op.as_str() {
            "add" => vec![solids[0].union(&solids[1])],
            "subtract" => vec![solids[0].difference(&solids[1])],
            "intersect" => vec![solids[0].intersection(&solids[1])],
            "batch_add" => vec![S::batch(&solids, BoolOp::Union)],
            "batch_subtract" => vec![S::batch(&solids, BoolOp::Difference)],
            "batch_intersect" => vec![S::batch(&solids, BoolOp::Intersection)],
            "trim_by_plane" => vec![solids[0].trim_by_plane(r.normal, r.offset)],
            "split" => {
                let (a, b) = solids[0].split(&solids[1]);
                vec![a, b]
            }
            "split_by_plane" => {
                let (a, b) = solids[0].split_by_plane(r.normal, r.offset);
                vec![a, b]
            }
            other => panic!("unknown op {other}"),
        };
        let tris: Vec<usize> = outs.iter().map(Mesh::triangle_count).collect();
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        t.ops += 1;
        t.ms += ms;
        t.py_ms += r.py_ms;
        for (k, o) in outs.iter().enumerate() {
            let ok = o.status().is_ok();
            let tm = o.to_tri_mesh();
            t.hash.u64(tm.content_hash());
            let wt = tm.is_watertight();
            let vol = o.volume();
            let pv = r.result_vol[k];
            let rel = if pv.abs() > 1e-9 {
                (vol - pv).abs() / pv.abs()
            } else {
                vol.abs()
            };
            let py = store.get(&r.results[k]);
            // Original IDs carried from the operands must match Python's;
            // IDs a plane op mints for its half-space come from a
            // process-wide counter, so only their number is compared.
            let ids = |m: &TriMesh| m.run_original_id.iter().copied().collect::<BTreeSet<u32>>();
            let carried: BTreeSet<u32> = ins.iter().flat_map(ids).collect();
            let (ours, theirs) = (ids(&tm), ids(&py));
            let ids_ok = ours
                .intersection(&carried)
                .eq(theirs.intersection(&carried))
                && ours.difference(&carried).count() == theirs.difference(&carried).count();
            let props_ok = tm.num_extra == py.num_extra;
            let same = tm.positions == py.positions && tm.triangles == py.triangles;
            t.status_err += usize::from(!ok);
            t.not_watertight += usize::from(!wt);
            t.tri_mismatch += usize::from(tris[k] != r.result_tris[k]);
            t.vol_over_1e6 += usize::from(rel > 1e-6);
            t.max_rel_vol = t.max_rel_vol.max(rel);
            t.ids_mismatch += usize::from(!ids_ok);
            t.props_mismatch += usize::from(!props_ok);
            t.identical += usize::from(same);
            writeln!(
                csv,
                "{name},{},{},{k},{ms:.4},{:.4},{},{},{vol:.9e},{pv:.9e},{rel:.3e},{wt},{ok},{ids_ok},{props_ok},{same}",
                r.id, r.op, r.py_ms, tris[k], r.result_tris[k]
            )
            .unwrap();
        }
    }
    t
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let case = &args[1];
    let flag = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .map(|i| args[i + 1].clone())
    };
    let backend = flag("--backend").unwrap_or_else(|| "ffi".into());
    let limit: usize = flag("--limit").map_or(usize::MAX, |s| s.parse().unwrap());
    let csv_path = flag("--csv").unwrap_or_else(|| format!("{case}.{backend}.csv"));
    let f = std::fs::File::open(dir.join(format!("{case}.jsonl"))).unwrap();
    let recs: Vec<Rec> = std::io::BufReader::new(f)
        .lines()
        .take(limit)
        .map(|l| parse(&l.unwrap()))
        .collect();
    let mut store = Store {
        dir: dir.clone(),
        cache: HashMap::new(),
        order: VecDeque::new(),
    };
    let mut csv = std::io::BufWriter::new(std::fs::File::create(&csv_path).unwrap());
    writeln!(csv, "backend,id,op,k,ms,py_ms,tris,py_tris,vol,py_vol,rel_vol,watertight,status_ok,ids_ok,props_ok,identical").unwrap();
    let par = cfg!(feature = "manifold-parallel");
    let wall = Instant::now();
    let t = match backend.as_str() {
        #[cfg(feature = "manifold-ffi")]
        "ffi" => run::<dt_geom::backend::ffi::FfiSolid>("ffi", &mut store, &recs, &mut csv),
        #[cfg(feature = "manifold-rust")]
        "port" => run::<dt_geom::backend::port::PortSolid>("port", &mut store, &recs, &mut csv),
        other => panic!("backend {other} not built"),
    };
    println!(
        "{case} backend={backend} parallel={par} ops={} import_fail={} status_err={} not_watertight={} \
         tri_mismatch={} vol>1e-6={} max_rel_vol={:.2e} ids_mismatch={} props_mismatch={} identical={} \
         op_s={:.3} py_op_s={:.3} ratio={:.3} wall_s={:.1} results_hash={:016x}",
        t.ops,
        t.failed_import,
        t.status_err,
        t.not_watertight,
        t.tri_mismatch,
        t.vol_over_1e6,
        t.max_rel_vol,
        t.ids_mismatch,
        t.props_mismatch,
        t.identical,
        t.ms / 1e3,
        t.py_ms / 1e3,
        t.ms / t.py_ms,
        wall.elapsed().as_secs_f64(),
        t.hash.finish()
    );
}
