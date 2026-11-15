//! Time, as the host gives it: wasm has no `Instant::now` or `SystemTime::now`, so the service asks a [`Clock`].

use std::time::Duration;

/// A source of time.
pub trait Clock: Send + Sync {
    /// Time since an arbitrary start that only grows: for the length of a job.
    fn elapsed(&self) -> Duration;
}

/// A clock that only the host can wind: a worker passes the time it has (`performance.now()`), a test sets it.
#[derive(Default)]
pub struct ManualClock {
    now: std::sync::Mutex<Duration>,
}

impl ManualClock {
    /// Set the time.
    pub fn set(&self, to: Duration) {
        *dt_value::LockExt::lock_unpoisoned(&self.now) = to;
    }
}

impl Clock for ManualClock {
    fn elapsed(&self) -> Duration {
        *dt_value::LockExt::lock_unpoisoned(&self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_says_what_it_was_set_to() {
        let c = ManualClock::default();
        assert_eq!(c.elapsed(), Duration::ZERO);
        c.set(Duration::from_secs(3));
        assert_eq!(c.elapsed(), Duration::from_secs(3));
    }
}
