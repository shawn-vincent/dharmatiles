//! R3 spike: replay captured Embree ray batches through the pure-Rust
//! backends and measure speed, hit agreement and build determinism.
//!
//! Capture first (Python, writes to a scratch directory, never into git):
//!
//! ```text
//! python tests/parity/capture/rays.py tree  $DIR/tree
//! python tests/parity/capture/rays.py bench $DIR/tree     # Embree timings
//! cargo run --release -p dt-geom --example r3_rays -- $DIR/tree
//! ```
//!
//! Every scene is replayed in the frame Embree saw (trimesh scales each
//! mesh to a ~100-unit box), with the same float32 origins and directions.
//! Distances are reported in mm by dividing by that scale.
//!
//! Options: `--reps N` (best of N timings, default 3), `--only NAME`
//! (one backend), `--out FILE` (results as TSV).
#![allow(
    clippy::cast_possible_truncation,
    reason = "a benchmark harness: f64 timings and u32 ids from the capture"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_geom::{Hit, ObvhsRays, ParryRays, Ray, Rays};
use obvhs::BvhBuildParams;
use rayon::{ThreadPool, ThreadPoolBuilder};

type Res<T> = Result<T, Box<dyn Error>>;

/// Agreement threshold for a non-identical hit (mm).
const TOL_MM: f64 = 1e-4;
/// Thread counts the build must be identical across.
const DET_THREADS: [usize; 3] = [1, 4, 16];

struct Scene {
    scale: f64,
    pos: Vec<[f32; 3]>,
    tris: Vec<[u32; 3]>,
    rays: Vec<Ray>,
    /// Embree's answer per ray: primID (-1 = miss) and tfar.
    prim: Vec<i32>,
    tfar: Vec<f32>,
    /// Index into `Capture::labels`, per ray.
    label: Vec<u16>,
    embree_build_s: f64,
    embree_query_s: f64,
}

struct Capture {
    scenes: BTreeMap<u32, Scene>,
    labels: Vec<String>,
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n).ok_or("truncated capture")?;
        self.i += n;
        Ok(s)
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn f64(&mut self) -> Res<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn f32s(&mut self, n: usize) -> Res<Vec<f32>> {
        Ok(self
            .take(n * 4)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect())
    }
    fn magic(&mut self, m: &[u8; 4]) -> Res<()> {
        if self.take(4)? == m {
            Ok(())
        } else {
            Err("bad record magic".into())
        }
    }
    fn done(&self) -> bool {
        self.i >= self.b.len()
    }
}

fn triples<T: Copy>(v: &[T]) -> Vec<[T; 3]> {
    v.as_chunks::<3>().0.to_vec()
}

fn load(dir: &Path) -> Res<Capture> {
    let mut scenes = BTreeMap::new();
    let buf = std::fs::read(dir.join("scenes.bin"))?;
    let mut r = Reader { b: &buf, i: 0 };
    while !r.done() {
        r.magic(b"DTSC")?;
        let (id, nv, nf) = (r.u32()?, r.u32()? as usize, r.u32()? as usize);
        let scale = r.f64()?;
        let pos = triples(&r.f32s(nv * 3)?);
        let idx: Vec<u32> = r
            .take(nf * 12)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        scenes.insert(
            id,
            Scene {
                scale,
                pos,
                tris: triples(&idx),
                rays: vec![],
                prim: vec![],
                tfar: vec![],
                label: vec![],
                embree_build_s: 0.0,
                embree_query_s: 0.0,
            },
        );
    }
    let mut labels: Vec<String> = vec![];
    let buf = std::fs::read(dir.join("batches.bin"))?;
    let mut r = Reader { b: &buf, i: 0 };
    while !r.done() {
        r.magic(b"DTRB")?;
        let sid = r.u32()?;
        let len = r.u32()? as usize;
        let name = String::from_utf8(r.take(len)?.to_vec())?;
        let n = r.u32()? as usize;
        let o = triples(&r.f32s(n * 3)?);
        let d = triples(&r.f32s(n * 3)?);
        let prim: Vec<i32> = r
            .take(n * 4)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i32::from_le_bytes(*c))
            .collect();
        let tfar = r.f32s(n)?;
        let li = match labels.iter().position(|l| *l == name) {
            Some(i) => i,
            None => {
                labels.push(name);
                labels.len() - 1
            }
        };
        let s = scenes.get_mut(&sid).ok_or("batch for unknown scene")?;
        s.rays
            .extend(o.iter().zip(&d).map(|(o, d)| Ray::new(*o, *d)));
        s.prim.extend(prim);
        s.tfar.extend(tfar);
        s.label.extend(std::iter::repeat_n(u16::try_from(li)?, n));
    }
    let tsv = std::fs::read_to_string(dir.join("embree.tsv"))
        .map_err(|e| format!("{e}: run `rays.py bench` first"))?;
    for line in tsv.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if let [sid, _, _, b, q] = f[..]
            && let Some(s) = scenes.get_mut(&sid.parse()?)
        {
            s.embree_build_s = b.parse()?;
            s.embree_query_s = q.parse()?;
        }
    }
    scenes.retain(|_, s| !s.rays.is_empty());
    Ok(Capture { scenes, labels })
}

/// Agreement with Embree, per label and in total.
#[derive(Default, Clone)]
struct Agree {
    rays: u64,
    /// Same triangle (or both miss).
    identical: u64,
    /// Different triangle, same distance within `TOL_MM` (shared edges).
    tie: u64,
    /// Both hit, distance off by more than `TOL_MM`.
    far: u64,
    /// One hit and one miss.
    hit_miss: u64,
    /// Max |Δt| (mm) over identical hits.
    max_same_mm: f64,
    /// Max |Δt| (mm) over non-identical hits that both hit.
    max_diff_mm: f64,
}

impl Agree {
    fn add(&mut self, o: &Agree) {
        self.rays += o.rays;
        self.identical += o.identical;
        self.tie += o.tie;
        self.far += o.far;
        self.hit_miss += o.hit_miss;
        self.max_same_mm = self.max_same_mm.max(o.max_same_mm);
        self.max_diff_mm = self.max_diff_mm.max(o.max_diff_mm);
    }
    fn pct(&self, n: u64) -> f64 {
        100.0 * n as f64 / self.rays.max(1) as f64
    }
}

/// Closest hit over every triangle (no BVH), with Embree's triangle test.
fn brute(s: &Scene, r: &Ray) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (i, t) in s.tris.iter().enumerate() {
        let tri = t.map(|k| s.pos[k as usize]);
        let q = Ray {
            t_max: best.map_or(r.t_max, |b| b.t),
            ..*r
        };
        if let Some(t) = dt_geom::rays::intersect_triangle(tri, &q)
            && best.is_none_or(|b| t < b.t)
        {
            best = Some(Hit {
                t,
                triangle: i as u32,
            });
        }
    }
    best
}

/// Who is right on a sample of disagreements: brute force (Embree's test,
/// no BVH) sides with Embree or with the backend; and is the ray stable, or
/// does its answer change when the origin moves by `TOL_MM`?
#[derive(Default, Clone, Copy)]
struct Referee {
    checked: u64,
    brute_is_embree: u64,
    brute_is_backend: u64,
    zero_dir_component: u64,
    unstable: u64,
}

impl Referee {
    fn check(&mut self, s: &Scene, r: &Ray, embree: Option<(i32, f32)>, ours: Option<Hit>) {
        self.checked += 1;
        let same = |a: Option<Hit>, b: Option<(u32, f32)>| match (a, b) {
            (None, None) => true,
            (Some(a), Some((tri, t))) => {
                a.triangle == tri || (f64::from(a.t - t).abs() / s.scale) <= TOL_MM
            }
            _ => false,
        };
        let b = brute(s, r);
        let e = embree.map(|(p, t)| (p as u32, t));
        self.brute_is_embree += u64::from(same(b, e));
        self.brute_is_backend += u64::from(same(b, ours.map(|h| (h.triangle, h.t))));
        self.zero_dir_component += u64::from(r.dir.contains(&0.0));
        let d = (TOL_MM * s.scale) as f32;
        let flips = (0..6).any(|k| {
            let mut q = *r;
            q.origin[k / 2] += if k % 2 == 0 { d } else { -d };
            !same(brute(s, &q), b.map(|h| (h.triangle, h.t)))
        });
        self.unstable += u64::from(flips);
    }
}

const REFEREE_SAMPLE: u64 = 200;

fn compare(
    s: &Scene,
    hits: &[Option<Hit>],
    per_label: &mut [Agree],
    referee: &mut Referee,
    far_ref: &mut Referee,
) {
    for (i, h) in hits.iter().enumerate() {
        let a = &mut per_label[s.label[i] as usize];
        a.rays += 1;
        let r = &s.rays[i];
        let len = f64::from(r.dir.iter().map(|c| c * c).sum::<f32>()).sqrt();
        let mm = |dt: f32| f64::from(dt).abs() * len / s.scale;
        match (h, s.prim[i]) {
            (None, p) if p < 0 => a.identical += 1,
            (Some(h), p) if p >= 0 => {
                let e = mm(h.t - s.tfar[i]);
                if i64::from(h.triangle) == i64::from(p) {
                    a.identical += 1;
                    a.max_same_mm = a.max_same_mm.max(e);
                } else {
                    a.max_diff_mm = a.max_diff_mm.max(e);
                    if e <= TOL_MM {
                        a.tie += 1;
                    } else {
                        a.far += 1;
                        if far_ref.checked < REFEREE_SAMPLE && a.far % 13 == 1 {
                            far_ref.check(s, r, Some((p, s.tfar[i])), Some(*h));
                        }
                    }
                }
            }
            _ => {
                a.hit_miss += 1;
                if referee.checked < REFEREE_SAMPLE && a.hit_miss % 97 == 1 {
                    let e = (s.prim[i] >= 0).then(|| (s.prim[i], s.tfar[i]));
                    referee.check(s, r, e, *h);
                }
            }
        }
    }
}

#[derive(Default)]
struct Run {
    build_1t: f64,
    build_nt: f64,
    query_1t: f64,
    query_nt: f64,
    deterministic_build: bool,
    deterministic_query: bool,
    per_label: Vec<Agree>,
    referee: Referee,
    far_ref: Referee,
}

fn best<T>(reps: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = None;
    let mut best = f64::INFINITY;
    for _ in 0..reps.max(1) {
        let t0 = Instant::now();
        let v = f();
        best = best.min(t0.elapsed().as_secs_f64());
        out = Some(v);
    }
    match out {
        Some(v) => (best, v),
        None => unreachable!("reps.max(1) runs at least once"),
    }
}

fn run<B: Rays + Send + Sync>(
    cap: &Capture,
    pools: &Pools,
    reps: usize,
    build: impl Fn(&[[f32; 3]], &[[u32; 3]]) -> B + Sync,
) -> Run {
    let mut out = Run {
        deterministic_build: true,
        deterministic_query: true,
        per_label: vec![Agree::default(); cap.labels.len()],
        ..Run::default()
    };
    for s in cap.scenes.values() {
        let (b1, bvh) = best(reps, || pools.one.install(|| build(&s.pos, &s.tris)));
        let (bn, _) = best(reps, || pools.all.install(|| build(&s.pos, &s.tris)));
        out.build_1t += b1;
        out.build_nt += bn;
        let h = bvh.structure_hash();
        for p in &pools.det {
            let again = p.install(|| build(&s.pos, &s.tris));
            out.deterministic_build &= again.structure_hash() == h;
        }
        let (q1, hits) = best(reps, || pools.one.install(|| bvh.intersect(&s.rays)));
        let (qn, hits_n) = best(reps, || pools.all.install(|| bvh.intersect(&s.rays)));
        out.query_1t += q1;
        out.query_nt += qn;
        out.deterministic_query &= hits == hits_n;
        compare(
            s,
            &hits,
            &mut out.per_label,
            &mut out.referee,
            &mut out.far_ref,
        );
    }
    out
}

struct Pools {
    one: ThreadPool,
    all: ThreadPool,
    det: Vec<ThreadPool>,
    n_all: usize,
}

fn pool(n: usize) -> Res<ThreadPool> {
    Ok(ThreadPoolBuilder::new().num_threads(n).build()?)
}

fn main() -> Res<()> {
    let mut args = std::env::args().skip(1);
    let mut dir = None;
    let (mut reps, mut only, mut out_path) = (3usize, None::<String>, None::<PathBuf>);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--reps" => reps = args.next().ok_or("--reps N")?.parse()?,
            "--only" => only = args.next(),
            "--out" => out_path = args.next().map(PathBuf::from),
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let dir = dir.ok_or("usage: r3_rays CAPTURE_DIR [--reps N] [--only NAME] [--out FILE]")?;
    let t0 = Instant::now();
    let cap = load(&dir)?;
    let n_rays: usize = cap.scenes.values().map(|s| s.rays.len()).sum();
    let n_tris: usize = cap.scenes.values().map(|s| s.tris.len()).sum();
    let e_build: f64 = cap.scenes.values().map(|s| s.embree_build_s).sum();
    let e_query: f64 = cap.scenes.values().map(|s| s.embree_query_s).sum();
    let n_all = std::thread::available_parallelism()?.get();
    let pools = Pools {
        one: pool(1)?,
        all: pool(n_all)?,
        det: DET_THREADS.iter().map(|&n| pool(n)).collect::<Res<_>>()?,
        n_all,
    };
    println!(
        "{}: {} scenes, {} triangles, {} rays, loaded in {:.1}s",
        dir.display(),
        cap.scenes.len(),
        n_tris,
        n_rays,
        t0.elapsed().as_secs_f64()
    );
    println!(
        "embree (embreex 4.4): build {:.1} ms (TBB threads), query {:.1} ms (1 thread)",
        e_build * 1e3,
        e_query * 1e3
    );

    let presets: [(&str, BvhBuildParams); 3] = [
        ("obvhs-very-fast", BvhBuildParams::very_fast_build()),
        ("obvhs-fast", BvhBuildParams::fast_build()),
        ("obvhs-medium", BvhBuildParams::medium_build()),
    ];
    let mut results: Vec<(String, Run)> = vec![];
    for (name, p) in presets {
        if only.as_deref().is_none_or(|o| o == name) {
            let r = run(&cap, &pools, reps, |v, t| ObvhsRays::build_with(v, t, p));
            results.push((name.to_string(), r));
        }
    }
    if only.as_deref().is_none_or(|o| o == "parry") {
        results.push(("parry".into(), run(&cap, &pools, reps, ParryRays::build)));
    }

    let mut tsv = String::from(
        "backend\tbuild_1t_ms\tbuild_nt_ms\tquery_1t_ms\tquery_nt_ms\tembree_build_ms\t\
         embree_query_ms\tbuild_ratio\tquery_ratio\tidentical_pct\ttie_pct\tfar\thit_miss\t\
         max_same_mm\tmax_diff_mm\tdet_build\tdet_query\n",
    );
    println!(
        "\n{:<16} {:>9} {:>9} {:>9} {:>9} {:>7} {:>7} {:>9} {:>7} {:>6} {:>6} {:>9} {:>9} {:>4} {:>4}",
        "backend",
        "build1 ms",
        &format!("build{n_all} ms", n_all = pools.n_all),
        "query1 ms",
        &format!("query{} ms", pools.n_all),
        "build×",
        "query×",
        "ident %",
        "tie %",
        "far",
        "h/miss",
        "maxsame",
        "maxdiff",
        "detB",
        "detQ"
    );
    for (name, r) in &results {
        let mut tot = Agree::default();
        r.per_label.iter().for_each(|a| tot.add(a));
        // Build: Embree builds with all threads. Query: embreex is 1 thread.
        let (bx, qx) = (r.build_nt / e_build, r.query_1t / e_query);
        println!(
            "{:<16} {:>9.1} {:>9.1} {:>9.1} {:>9.1} {:>7.2} {:>7.2} {:>9.4} {:>7.4} {:>6} {:>6} {:>9.2e} {:>9.2e} {:>4} {:>4}",
            name,
            r.build_1t * 1e3,
            r.build_nt * 1e3,
            r.query_1t * 1e3,
            r.query_nt * 1e3,
            bx,
            qx,
            tot.pct(tot.identical),
            tot.pct(tot.tie),
            tot.far,
            tot.hit_miss,
            tot.max_same_mm,
            tot.max_diff_mm,
            r.deterministic_build,
            r.deterministic_query
        );
        let _ = writeln!(
            tsv,
            "{name}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{bx:.3}\t{qx:.3}\t{:.5}\t{:.5}\t{}\t{}\t{:.3e}\t{:.3e}\t{}\t{}",
            r.build_1t * 1e3,
            r.build_nt * 1e3,
            r.query_1t * 1e3,
            r.query_nt * 1e3,
            e_build * 1e3,
            e_query * 1e3,
            tot.pct(tot.identical),
            tot.pct(tot.tie),
            tot.far,
            tot.hit_miss,
            tot.max_same_mm,
            tot.max_diff_mm,
            r.deterministic_build,
            r.deterministic_query
        );
        for (kind, rf) in [("hit/miss", r.referee), ("far", r.far_ref)] {
            if rf.checked > 0 {
                println!(
                    "    referee, {} sampled {kind} rays: brute force sides with embree {}, with {name} {}; zero dir component {}; answer flips under a {TOL_MM:e} mm nudge {}",
                    rf.checked,
                    rf.brute_is_embree,
                    rf.brute_is_backend,
                    rf.zero_dir_component,
                    rf.unstable
                );
                let _ = writeln!(
                    tsv,
                    "#referee\t{name}\t{kind}\t{}\t{}\t{}\t{}\t{}",
                    rf.checked,
                    rf.brute_is_embree,
                    rf.brute_is_backend,
                    rf.zero_dir_component,
                    rf.unstable
                );
            }
        }
        for (li, a) in r.per_label.iter().enumerate() {
            if a.rays > 0 && cap.labels.len() > 1 {
                println!(
                    "    {:<40} {:>9} rays  ident {:>8.4}%  tie {:>7.4}%  far {:>5}  hit/miss {:>5}  maxdiff {:.2e} mm",
                    cap.labels[li],
                    a.rays,
                    a.pct(a.identical),
                    a.pct(a.tie),
                    a.far,
                    a.hit_miss,
                    a.max_diff_mm
                );
            }
        }
    }
    if let Some(p) = out_path {
        std::fs::write(&p, tsv)?;
        println!("\nwrote {}", p.display());
    }
    Ok(())
}
