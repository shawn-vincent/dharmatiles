//! `cargo run --release -p dt-eval --example trace -- lib:tiles/test/tree-1x1.kdl`:
//! print each build stage and look sub-progress with its time, to calibrate
//! the progress fractions inside a look.

use dt_eval::{Files, Options, Progress};
use dt_kit::{Event, Reporter};
use std::sync::Arc;
use std::time::Instant;
struct R(Instant);
impl Reporter for R {
    fn report(&self, e: &Event<'_>) {
        let t = self.0.elapsed().as_secs_f64();
        match e {
            Event::Sub {
                what,
                done,
                total,
                fraction,
            } if *total == 0 || done == total || *done == 1 => {
                println!("{t:6.2} sub {what} {done}/{total} {fraction:.2}")
            }
            Event::Sub { .. } => {}
            e => println!("{t:6.2} {e:?}"),
        }
    }
}
fn main() {
    let Some(p) = std::env::args().nth(1) else {
        return;
    };
    let pr = Progress::new(Arc::new(R(Instant::now())));
    let _ = dt_eval::build_with(&Files, &p, &Options::default(), &pr);
}
