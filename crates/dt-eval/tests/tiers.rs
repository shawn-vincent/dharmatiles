//! The tiers of a build: what each one is, what it costs and what it keeps.

#![allow(clippy::unwrap_used, clippy::print_stderr)]

use dt_eval::{Built, Library, Options, Tier, build_tier};

/// Six library tiles that between them hold every element a tier draws
/// differently: grass, soil, a wall, water, a tree and a cliff.
const SIX: &[&str] = &[
    "lib:tiles/ground/1x1-grass.dtile",
    "lib:tiles/ground/1x1-soil+grass.dtile",
    "lib:tiles/walls/1x1-wall-stone.dtile",
    "lib:tiles/water/1x1-water+grass.dtile",
    "lib:tiles/ground/1x1-grass-tree.dtile",
    "lib:tiles/cliffs/1x1-cliff.dtile",
];

fn made(path: &str, tier: Tier) -> (Built, f64) {
    let t = std::time::Instant::now();
    let b = build_tier(
        &Library,
        path,
        &Options::default(),
        tier,
        &dt_eval::Progress::none(),
        None,
        None,
    )
    .unwrap_or_else(|e| panic!("{path}: {e:?}"));
    (b, t.elapsed().as_secs_f64())
}

/// Not a check: `DT_TIER_TILE=lib:tiles/…` prints each stack entry's
/// triangles at the tier named by `DT_TIER` (default low), and the sweep of
/// the six tiles with `DT_TIER_SWEEP=1`.
#[test]
#[ignore = "a measurement: prints triangles per entry and the six tiles' times"]
fn what_each_tier_costs() {
    let tier = match std::env::var("DT_TIER").as_deref() {
        Ok("full") => Tier::Full,
        Ok("super-low") => Tier::SuperLow,
        _ => Tier::Low,
    };
    let tiles: Vec<String> = match std::env::var("DT_TIER_TILE") {
        Ok(t) => vec![t],
        Err(_) if std::env::var("DT_TIER_SWEEP").is_ok() => {
            SIX.iter().map(|s| (*s).to_owned()).collect()
        }
        Err(_) => return,
    };
    for tile in tiles {
        let (b, secs) = made(&tile, tier);
        eprintln!(
            "{tile} at {}: {} triangles in {secs:.2} s",
            tier.name(),
            b.export.mesh.triangles.len()
        );
        let mut per = vec![0usize; b.stack.len() + 1];
        for &e in &b.entries {
            let i = usize::from(e).min(b.stack.len());
            per[i] += 1;
        }
        for (i, n) in per.iter().enumerate() {
            let name = b.stack.get(i).map_or("(none)", |s| s.id.as_str());
            eprintln!("    {name}: {n}");
        }
    }
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dt-tiers-{}-{name}", std::process::id()));
    if d.exists() {
        std::fs::remove_dir_all(&d).unwrap();
    }
    d
}

const GRASS: &str = "lib:tiles/ground/1x1-soil+grass.dtile";

#[test]
fn only_a_full_build_is_checked_and_a_low_one_is_lighter() {
    let (full, _) = made(GRASS, Tier::Full);
    let (low, _) = made(GRASS, Tier::Low);
    let (tiny, _) = made(GRASS, Tier::SuperLow);
    assert!(full.checked && !low.checked && !tiny.checked);
    assert!(Tier::Full.checked() && !Tier::Low.checked() && !Tier::SuperLow.checked());
    let n = |b: &Built| b.export.mesh.triangles.len();
    assert!(n(&low) < n(&full) / 5, "{} against {}", n(&low), n(&full));
    assert!(n(&tiny) <= dt_eval::tiers::SUPER_LOW_BUDGET && n(&tiny) <= n(&low));
    // overhang is still painted by angle: every list is one per triangle
    assert_eq!(low.audit.overhang.len(), n(&low));
    assert_eq!(tiny.audit.overhang.len(), n(&tiny));
    // the same places
    assert_eq!(full.placed, low.placed);
    assert_eq!(full.placed, tiny.placed);
}

#[test]
fn the_three_tiers_are_three_entries() {
    let dir = temp("keys");
    let store = dt_eval::Store::new(&dir, "test-engine");
    let go = |tier| {
        build_tier(
            &Library,
            GRASS,
            &Options::default(),
            tier,
            &dt_eval::Progress::none(),
            None,
            Some(&store),
        )
        .unwrap()
    };
    use dt_eval::CacheState::{Hit, Miss};
    assert_eq!(go(Tier::SuperLow).cache.state, Miss);
    // the low it was cut from was not kept: the low is its own entry
    assert_eq!(go(Tier::Low).cache.state, Miss);
    assert_eq!(go(Tier::Low).cache.state, Hit);
    assert_eq!(go(Tier::Full).cache.state, Miss);
    assert_eq!(go(Tier::SuperLow).cache.state, Hit);
    assert_eq!(go(Tier::Full).cache.state, Hit);
    assert_eq!(store.stats().unwrap().entries, 3);
    let low = go(Tier::Low);
    assert!(!low.checked, "a low build read back is still not checked");
    assert!(go(Tier::Full).checked);
    std::fs::remove_dir_all(&dir).unwrap();
}
