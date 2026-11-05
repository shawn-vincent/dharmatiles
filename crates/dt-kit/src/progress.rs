//! A progress side channel: stage and sub-progress events for a build.
//!
//! A front end attaches a [`Reporter`]; the evaluator announces each stage
//! (an element, the union, the audit) and an element with a natural loop reports
//! how far through it is.  The channel never touches geometry or seeds, so
//! a build is bit-identical with or without a reporter, and with none
//! attached every call is a test of one `Option`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What a build reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event<'a> {
    /// A stage begins.  `key` identifies it for timings (`3:grass`,
    /// `union`); `label` is for people (`grass`).
    Begin { key: &'a str, label: &'a str },
    /// The stage `key` ends.
    End { key: &'a str },
    /// Inside the current stage: `fraction` (0..=1) of it is done; `done`
    /// of `total` `what`s, when it counts something.
    Sub {
        what: &'a str,
        done: usize,
        total: usize,
        fraction: f64,
    },
}

/// Receives events, possibly from many threads at once, so keep it cheap.
pub trait Reporter: Send + Sync {
    /// One event.
    fn report(&self, event: &Event<'_>);
}

/// The handle elements and the evaluator report through.  Cloning is cheap;
/// the default reports nothing.
#[derive(Clone, Default)]
pub struct Progress(Option<Arc<dyn Reporter>>);

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Progress({})",
            if self.0.is_some() { "on" } else { "off" }
        )
    }
}

impl Progress {
    /// Reports nothing.
    #[must_use]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Reports to `reporter`.
    #[must_use]
    pub fn new(reporter: Arc<dyn Reporter>) -> Self {
        Self(Some(reporter))
    }

    /// Is anything listening?
    #[must_use]
    pub fn is_on(&self) -> bool {
        self.0.is_some()
    }

    /// Send one event.
    pub fn send(&self, e: &Event<'_>) {
        if let Some(r) = &self.0 {
            r.report(e);
        }
    }

    /// A stage begins.
    pub fn begin(&self, key: &str, label: &str) {
        self.send(&Event::Begin { key, label });
    }

    /// A stage ends.
    pub fn end(&self, key: &str) {
        self.send(&Event::End { key });
    }

    /// `fraction` (0..=1) of the current stage is done, with no count.
    pub fn fraction(&self, what: &str, fraction: f64) {
        self.send(&Event::Sub {
            what,
            done: 0,
            total: 0,
            fraction: fraction.clamp(0.0, 1.0),
        });
    }

    /// A counter for a loop of `total` `what`s that is the part `lo..hi`
    /// (fractions of the stage) of the current stage.  Tick it from any
    /// thread.
    #[must_use]
    pub fn counter(&self, what: &'static str, total: usize, lo: f64, hi: f64) -> Counter {
        Counter {
            progress: self.clone(),
            what,
            total,
            done: AtomicUsize::new(0),
            lo,
            hi,
            // about 200 reports for the loop, however long it is
            step: (total / 200).max(1),
        }
    }
}

/// Counts a loop's iterations and reports them as part of the stage.
pub struct Counter {
    progress: Progress,
    what: &'static str,
    total: usize,
    done: AtomicUsize,
    lo: f64,
    hi: f64,
    step: usize,
}

impl Counter {
    /// One more done.  Safe from parallel iterators.
    #[allow(clippy::cast_precision_loss)]
    pub fn tick(&self) {
        if !self.progress.is_on() {
            return;
        }
        let n = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(self.step) || n == self.total {
            let f = if self.total == 0 {
                1.0
            } else {
                (n as f64 / self.total as f64).min(1.0)
            };
            self.progress.send(&Event::Sub {
                what: self.what,
                done: n,
                total: self.total,
                fraction: self.lo + (self.hi - self.lo) * f,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);
    impl Reporter for Log {
        fn report(&self, e: &Event<'_>) {
            self.0.lock().unwrap().push(format!("{e:?}"));
        }
    }

    #[test]
    fn a_counter_reports_in_range_and_ends_at_its_top() {
        let log = Arc::new(Log::default());
        let p = Progress::new(log.clone());
        let c = p.counter("blades", 1000, 0.0, 0.5);
        for _ in 0..1000 {
            c.tick();
        }
        let v = log.0.lock().unwrap();
        assert!(v.len() <= 205, "throttled by count: {}", v.len());
        assert!(
            v.last().unwrap().contains("fraction: 0.5"),
            "{:?}",
            v.last()
        );
    }

    #[test]
    fn nothing_attached_reports_nothing() {
        let p = Progress::none();
        let c = p.counter("x", 10, 0.0, 1.0);
        c.tick();
        p.begin("a", "a");
        assert!(!p.is_on());
    }
}
