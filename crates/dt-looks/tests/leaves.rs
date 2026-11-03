//! The organic leaf placer against a small Python capture: two merged
//! clumps of the 1x1-grass-tree tile's tree (seed 1), placed without wood
//! (`tests/parity/capture/leaves.py out/leaves/small 1 --clumps 2
//! --compact`; the samples are stored as f32).

mod fixture;

use dt_kit::{Seed, TriMesh};
use dt_looks::tree::leaves::{LeafShape, LeafStats, place, place_with};

fn capture() -> fixture::Capture {
    let bytes = include_bytes!("fixtures/leaves-small.bin");
    fixture::read(bytes).expect("fixture reads")
}

fn hash(leaves: &[TriMesh], stats: &LeafStats) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |x: u64| {
        for b in x.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for m in leaves {
        for p in &m.positions {
            p.iter().for_each(|c| eat(c.to_bits()));
        }
        for t in &m.triangles {
            t.iter().for_each(|i| eat(u64::from(*i)));
        }
    }
    eat(stats.uncovered as u64);
    eat(stats.failed() as u64);
    h
}

#[test]
fn parity_with_python() {
    let c = capture();
    let foliage = c.mesh("foliage");
    let (leaves, stats) =
        place_with(&foliage, &TriMesh::default(), &c.shape(), &c.draws()).expect("places");
    let [placed, roots, failed, culled, uncovered] = c.py_counts();
    assert_eq!(culled, 0);
    // Same samples, same throw: the same roots.
    assert_eq!(stats.roots, roots as usize);
    assert_eq!(stats.uncovered, uncovered as usize);
    // Same leaves, within a build or two (the samples went through f32).
    let diff = (stats.placed as i64 - i64::from(placed)).abs();
    assert!(diff <= 2, "placed {} vs Python {placed}", stats.placed);
    assert!(stats.failed().abs_diff(failed as usize) <= 2);
    // Leaf by leaf: roots and directions.
    let py: Vec<([f64; 3], [f64; 3])> = c
        .points("py/base")
        .into_iter()
        .zip(c.points("py/dir"))
        .collect();
    let matched = stats
        .leaves
        .iter()
        .map(|l| (l.root, l.tangent))
        .filter(|(b, d)| {
            py.iter().any(|(pb, pd)| {
                (0..3).all(|k| (b[k] - pb[k]).abs() < 1e-5)
                    && d[0] * pd[0] + d[1] * pd[1] + d[2] * pd[2] > 0.999_99
            })
        })
        .count();
    assert!(
        matched + 2 >= stats.placed,
        "{matched} of {} match",
        stats.placed
    );
    assert_eq!(leaves.len(), stats.placed);
}

#[test]
fn every_leaf_is_closed() {
    let c = capture();
    let (leaves, stats) = place(
        &c.mesh("foliage"),
        &TriMesh::default(),
        &c.shape(),
        Seed::new(7),
    )
    .expect("places");
    assert!(stats.placed > 150, "{stats:?}");
    for m in &leaves {
        assert!(m.is_watertight());
        assert!(dt_kit::signed_volume(m) > 0.0);
    }
}

#[test]
fn complete_coverage() {
    let c = capture();
    let (_, stats) = place(
        &c.mesh("foliage"),
        &TriMesh::default(),
        &c.shape(),
        Seed::new(3),
    )
    .expect("places");
    // Maximal Poisson disk: every placeable test point is within a spacing
    // of a root, but for the few the normal-aware rule leaves at creases.
    assert!(stats.test_points > 500);
    assert!(
        stats.uncovered * 100 <= stats.test_points,
        "{} of {} uncovered",
        stats.uncovered,
        stats.test_points
    );
    // No root closer than the spacing to a like-facing one (no rows, no
    // piling up): spot-check the nearest neighbour distance.
    let r: Vec<([f64; 3], [f64; 3])> = stats.leaves.iter().map(|l| (l.root, l.tangent)).collect();
    for (i, (a, _)) in r.iter().enumerate().take(60) {
        let nearest = r
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, (b, _))| {
                ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(nearest > 0.2, "root {i} has a neighbour at {nearest} mm");
    }
}

#[test]
fn same_at_1_4_16_threads() {
    let c = capture();
    let foliage = c.mesh("foliage");
    let shape = LeafShape {
        overhang_deg: Some(55.0),
        ..c.shape()
    };
    let run = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("pool")
            .install(|| {
                let (l, s) =
                    place(&foliage, &TriMesh::default(), &shape, Seed::new(11)).expect("places");
                hash(&l, &s)
            })
    };
    let h1 = run(1);
    assert_eq!(h1, run(4));
    assert_eq!(h1, run(16));
}

#[test]
fn printable_undersides() {
    let c = capture();
    let foliage = c.mesh("foliage");
    let exact = c.shape();
    let printable = LeafShape {
        overhang_deg: Some(55.0),
        ..exact
    };
    let (_, e) = place_with(&foliage, &TriMesh::default(), &exact, &c.draws()).expect("places");
    let (leaves, p) =
        place_with(&foliage, &TriMesh::default(), &printable, &c.draws()).expect("places");
    // The printer limit lays the underside blades flat on the crown
    // (turning the few a crease would fold): the count stays within 5%.
    assert!(p.placed * 100 >= e.placed * 95 && p.placed * 100 <= e.placed * 105);
    assert!(p.leaves.iter().all(|l| l.normal[2] >= -0.75));
    for m in &leaves {
        assert!(m.is_watertight());
    }
    assert_eq!(LeafShape::default().overhang_deg, Some(55.0));
}
