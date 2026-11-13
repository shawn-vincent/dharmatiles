//! The stopwatch a build times its stages with.
//!
//! `std::time::Instant` panics on `wasm32-unknown-unknown` (a browser tab
//! has no clock the standard library can reach), so the one place a build
//! reads time is here: native builds use `Instant`; the browser build reads
//! the clock its host registers with [`set_clock`] (the page's
//! `performance.now()`), and refuses to run without one.  Timings only weigh
//! the progress bar and fill `stage_secs`: no geometry or hash depends on them.

#[cfg(not(target_arch = "wasm32"))]
pub struct Stopwatch(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl Stopwatch {
    pub fn start() -> Self {
        Self(std::time::Instant::now())
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

#[cfg(target_arch = "wasm32")]
static CLOCK: std::sync::OnceLock<fn() -> f64> = std::sync::OnceLock::new();

/// Register the browser's clock: seconds from any fixed start.  Only the
/// first call counts.
#[cfg(target_arch = "wasm32")]
pub fn set_clock(now_secs: fn() -> f64) {
    // a second registration is the same page's clock again: nothing to do
    let _ = CLOCK.set(now_secs);
}

#[cfg(target_arch = "wasm32")]
pub struct Stopwatch(f64);

#[cfg(target_arch = "wasm32")]
impl Stopwatch {
    fn now() -> f64 {
        let clock = CLOCK
            .get()
            .expect("the browser build needs a clock: call dt_eval::clock::set_clock first");
        clock()
    }

    pub fn start() -> Self {
        Self(Self::now())
    }

    pub fn elapsed_secs(&self) -> f64 {
        Self::now() - self.0
    }
}
