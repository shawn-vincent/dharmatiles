#![allow(clippy::unwrap_used)]
use dt_eval::{Library, Options, build};
#[test]
#[ignore = "probe"]
fn probe() {
    let want = std::env::var("PROBE").unwrap();
    let seeds: Vec<Option<u64>> = if std::env::var("PROBE_ONE").is_ok() {
        vec![None]
    } else {
        vec![None, Some(9001), Some(9002), Some(9003)]
    };
    for name in want.split(',') {
        let path = format!("lib:tiles/cliffs/{name}.dtile");
        let mut worst = [0usize; 5];
        let mut tri = 0;
        for s in &seeds {
            let o = Options {
                seed: *s,
                ..Options::default()
            };
            let b = build(&Library, &path, &o).unwrap();
            let a = &b.audit;
            tri = a.triangles;
            let c = [
                a.voids.len(),
                a.pinches_moved,
                a.slivers.len(),
                a.collapsed,
                a.dropped.len(),
            ];
            for i in 0..5 {
                worst[i] = worst[i].max(c[i]);
            }
        }
        println!("PROBE {name} tri {tri} voids/pinch/sliv/coll/drop worst {worst:?}");
    }
}
