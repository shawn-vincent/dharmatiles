use std::sync::Arc;
use std::time::Instant;
use dt_eval::{Files, Options, Progress};
use dt_kit::{Event, Reporter};
struct R(Instant);
impl Reporter for R {
    fn report(&self, e: &Event<'_>) {
        let t = self.0.elapsed().as_secs_f64();
        match e {
            Event::Sub { what, done, total, fraction } if *total == 0 || done == total || *done == 1 => println!("{t:6.2} sub {what} {done}/{total} {fraction:.2}"),
            Event::Sub { .. } => {}
            e => println!("{t:6.2} {e:?}"),
        }
    }
}
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let pr = Progress::new(Arc::new(R(Instant::now())));
    let _ = dt_eval::build_with(&Files, &p, &Options::default(), &pr);
}
