//! Where a pattern=cover scatter's things land in a built tile.
//!
//! The scatter's own tests hold its dart sequence even; what matters is what
//! stands in the finished tile.  A cover of pebbles round one rock is built on
//! several seeds and read back from the mesh as it is seen from above: which
//! ground the pebbles cover and which is bare, and in each cell of a grid and in
//! each ring round the rock's outline the share that is bare.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "a test: a failed build panics with its diagnostics named"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "a test: pixel and cell indexes of mm values inside the tile"
)]

use std::collections::BTreeMap;

use dt_eval::{Built, Memory, Options};

use super::memo;

/// Seeds the cover is built on.
const SEEDS: std::ops::RangeInclusive<u64> = 1..=4;
/// The tile's edge, mm (one DungeonBlocks square).
const SIDE: f64 = 35.0;
/// The grid is laid over the tile without its edge band: pebbles keep off
/// the rim (the rim and a pebble's reach), so the band is bare by rule.
const BAND: f64 = 3.0;
/// Cells to a side of the grid.
const CELLS: usize = 4;
/// Pixels to a side of the raster the tile is seen in (0.25 mm each).
const PX: usize = 140;

/// A cover of pebbles round one rock, and nothing else: the pebble-cover test
/// tile without its clearings, its cover share and its `near`, so what is
/// measured is the landing and not the thinning.
fn cover(seed: u64) -> &'static Built {
    let text = "dharmatiles version=\"1.0.0\"\n\
        tile size=\"1x1\" seed=5 {\n\
        stack {\n\
            base\n\
            dirt\n\
            rock at=\"(0.5sq, 0.5sq)\" size=\"9dmm\"\n\
            scatter perSquare=300 pattern=cover { pebble size=\"1.2dmm .. 2.2dmm\" }\n\
        }\n\
        }\n";
    let src = Memory(BTreeMap::from([("t.dtile".to_owned(), text.to_owned())]));
    let opts = dt_testkit::seeded(&Options {
        seed: Some(seed),
        ..Options::default()
    });
    memo::build(&src, "t.dtile", &opts).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// Which pixels of the tile seen from above the triangles of the first stack
/// entry of the element `what` cover.
fn seen(b: &Built, what: &str) -> Vec<bool> {
    let entry = u16::try_from(b.report.stack.iter().position(|n| n.what == what).unwrap()).unwrap();
    let mesh = &b.export.mesh;
    let px = SIDE / PX as f64;
    let mut out = vec![false; PX * PX];
    for (t, tri) in mesh.triangles.iter().enumerate() {
        if b.report.entries[t] != entry {
            continue;
        }
        let c = tri.map(|i| {
            let v = mesh.positions[i as usize];
            [v[0] / px - 0.5, v[1] / px - 0.5]
        });
        let span =
            |k: usize, f: fn(f64, f64) -> f64, from: f64| c.iter().map(|p| p[k]).fold(from, f);
        let (x0, x1) = (span(0, f64::min, f64::MAX), span(0, f64::max, f64::MIN));
        let (y0, y1) = (span(1, f64::min, f64::MAX), span(1, f64::max, f64::MIN));
        for j in (y0.ceil().max(0.0) as usize)..=(y1.floor().max(-1.0) as usize).min(PX - 1) {
            for i in (x0.ceil().max(0.0) as usize)..=(x1.floor().max(-1.0) as usize).min(PX - 1) {
                let p = [i as f64, j as f64];
                let side = |a: [f64; 2], b: [f64; 2]| {
                    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
                };
                let s = [side(c[0], c[1]), side(c[1], c[2]), side(c[2], c[0])];
                if s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0) {
                    out[j * PX + i] = true;
                }
            }
        }
    }
    out
}

/// A point of the free ground: the cell of the grid it is in, how far it is
/// from the rock's outline (mm), and whether a pebble covers it.
struct Spot {
    cell: usize,
    from_rock: f64,
    covered: bool,
}

/// The free ground of the tile seen from above: every pixel off the rock and
/// inside the edge band, with what covers it.
fn ground(b: &Built) -> Vec<Spot> {
    let (rock, pebbles) = (seen(b, "rock"), seen(b, "scatter"));
    let px = SIDE / PX as f64;
    let rock_at: Vec<[f64; 2]> = (0..PX * PX)
        .filter(|q| rock[*q])
        .map(|q| [(q % PX) as f64, (q / PX) as f64])
        .collect();
    let span = SIDE - 2.0 * BAND;
    let mut out = Vec::new();
    for q in 0..PX * PX {
        let (i, j) = (q % PX, q / PX);
        let (x, y) = ((i as f64 + 0.5) * px - BAND, (j as f64 + 0.5) * px - BAND);
        if rock[q] || x < 0.0 || y < 0.0 || x >= span || y >= span {
            continue;
        }
        let side = span / CELLS as f64;
        let near = rock_at
            .iter()
            .map(|r| libm::hypot(r[0] - i as f64, r[1] - j as f64))
            .fold(f64::MAX, f64::min);
        out.push(Spot {
            cell: (y / side) as usize * CELLS + (x / side) as usize,
            from_rock: near * px,
            covered: pebbles[q],
        });
    }
    out
}

/// The share of `spots` no pebble covers.
fn bare<'a>(spots: impl Iterator<Item = &'a Spot>) -> f64 {
    let (mut n, mut bare) = (0.0, 0.0);
    for s in spots {
        n += 1.0;
        bare += f64::from(u8::from(!s.covered));
    }
    bare / f64::max(n, 1.0)
}

/// How many of the cover's stones stand in each cell of the grid (a stone is
/// counted where the middle of what shows of it is), and each cell's share of
/// the free ground.
fn counts(b: &Built, spots: &[Spot]) -> (Vec<f64>, Vec<f64>) {
    let entry = u16::try_from(
        b.report
            .stack
            .iter()
            .position(|n| n.what == "scatter")
            .unwrap(),
    )
    .unwrap();
    let mesh = &b.export.mesh;
    let mut boxes: BTreeMap<u16, [f64; 4]> = BTreeMap::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        if b.report.entries[t] != entry {
            continue;
        }
        let e = boxes
            .entry(b.report.pieces[t])
            .or_insert([f64::MAX, f64::MAX, f64::MIN, f64::MIN]);
        for &v in tri {
            let p = mesh.positions[v as usize];
            e[0] = e[0].min(p[0]);
            e[1] = e[1].min(p[1]);
            e[2] = e[2].max(p[0]);
            e[3] = e[3].max(p[1]);
        }
    }
    let span = SIDE - 2.0 * BAND;
    let side = span / CELLS as f64;
    let mut count = vec![0.0; CELLS * CELLS];
    for e in boxes.values() {
        let (x, y) = ((e[0] + e[2]) / 2.0 - BAND, (e[1] + e[3]) / 2.0 - BAND);
        if x >= 0.0 && y >= 0.0 && x < span && y < span {
            count[(y / side) as usize * CELLS + (x / side) as usize] += 1.0;
        }
    }
    let share = (0..CELLS * CELLS)
        .map(|c| spots.iter().filter(|s| s.cell == c).count() as f64 / spots.len() as f64)
        .collect();
    (count, share)
}

/// What a build's ground looks like from above: the stones a cell holds, the bare share in the
/// worst cell, and in the rings round the rock (to 1 mm, 1 to 2.5, 2.5 to 4 and
/// 4 to 8 mm out).
struct Bare {
    worst_cell: f64,
    /// The fewest and the most stones a cell holds, as a multiple of its share.
    stones: (f64, f64),
    rings: [f64; 4],
}

fn bare_of(b: &Built) -> Bare {
    let spots = ground(b);
    let (count, share) = counts(b, &spots);
    let total: f64 = count.iter().sum();
    let ratios: Vec<f64> = count
        .iter()
        .zip(&share)
        .map(|(c, s)| c / (s * total))
        .collect();
    let ring = |lo: f64, hi: f64| {
        bare(
            spots
                .iter()
                .filter(|s| s.from_rock >= lo && s.from_rock < hi),
        )
    };
    Bare {
        stones: (
            ratios.iter().copied().fold(f64::MAX, f64::min),
            ratios.iter().copied().fold(0.0, f64::max),
        ),
        worst_cell: (0..CELLS * CELLS)
            .map(|c| bare(spots.iter().filter(|s| s.cell == c)))
            .fold(0.0, f64::max),
        rings: [
            ring(0.0, 1.0),
            ring(1.0, 2.5),
            ring(2.5, 4.0),
            ring(4.0, 8.0),
        ],
    }
}

/// A cover of pebbles round a rock is even where it lands, not only in its
/// darts: whatever the seed, every cell of a grid over the tile holds between
/// 0.55 and 1.75 times its share of the stones (the share is its free ground,
/// the rock's own not counted), no cell is bare over more than 65% of its
/// ground, and the pebbles lie against the rock's outline on every side: the
/// ground within 1 mm of it is no more than 95% bare, that 1 to 2.5 mm out no
/// more than 70% and that 2.5 to 4 mm out no more than 40% (a cover with a halo
/// round the rock leaves these nearly bare: on the rock's short side the
/// keep-clear was a disc of its bounds).
///
/// The limits are what the seeds measure, with room: a stone is a cluster's
/// member a quarter of the time, so counts are not the darts' even ones, and a
/// pebble touching a rock does so at one point.
#[test]
fn a_cover_lands_evenly_and_against_the_rock() {
    for seed in SEEDS {
        let m = bare_of(cover(seed));
        let (low, high) = m.stones;
        assert!(
            (0.55..=1.75).contains(&low) && (0.55..=1.75).contains(&high),
            "seed {seed}: a cell holds {low:.2} to {high:.2} times its share of the stones"
        );
        assert!(
            m.worst_cell <= 0.65,
            "seed {seed}: a cell is {:.2} bare",
            m.worst_cell
        );
        for (ring, (limit, name)) in m.rings.iter().zip([
            (0.95, "to 1 mm"),
            (0.70, "1 to 2.5 mm"),
            (0.40, "2.5 to 4 mm"),
        ]) {
            assert!(
                *ring <= limit,
                "seed {seed}: the ground {name} from the rock is {ring:.2} bare, over {limit}"
            );
        }
    }
}
