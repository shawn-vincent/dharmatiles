//! The memory a build's stages reach.
//!
//! Natively a sampler thread reads the process's resident size (`memory-stats`: on macOS the task's
//! `resident_size`, on Linux `/proc/self/statm`) every few milliseconds while a build runs, and
//! [`Meter::stage_peak_mb`] hands back the highest reading since the last call: the peak of one stage.  It is the
//! whole process's, so builds running side by side in one process (the preview's workers, the tests) share it: it
//! is the number to read from a build alone (`dharmatiles build --json`).  In the browser a build is alone, the
//! module's linear memory never shrinks, and its size is the peak so far.
//!
//! Peaks vary from run to run: they are in the record and the build's message and never in a hash, a snapshot
//! or the disk cache's key.

const MB: f64 = 1_048_576.0;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread::JoinHandle;
    use std::time::Duration;

    /// How often the sampler reads the resident size.
    const EVERY: Duration = Duration::from_millis(2);

    fn resident() -> Option<u64> {
        memory_stats::memory_stats().map(|m| m.physical_mem as u64)
    }

    pub struct Meter {
        peak: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        sampler: Option<JoinHandle<()>>,
    }

    impl Meter {
        pub fn start() -> Self {
            let peak = Arc::new(AtomicU64::new(resident().unwrap_or(0)));
            let stop = Arc::new(AtomicBool::new(false));
            let (p, s) = (peak.clone(), stop.clone());
            let sampler = std::thread::Builder::new()
                .name("memory-meter".into())
                .spawn(move || {
                    while !s.load(Ordering::Relaxed) {
                        if let Some(r) = resident() {
                            p.fetch_max(r, Ordering::Relaxed);
                        }
                        std::thread::sleep(EVERY);
                    }
                })
                .expect("cannot start the memory meter's thread");
            Self {
                peak,
                stop,
                sampler: Some(sampler),
            }
        }

        pub fn stage_peak_bytes(&self) -> Option<u64> {
            let now = resident()?;
            Some(self.peak.swap(now, Ordering::Relaxed).max(now))
        }
    }

    impl Drop for Meter {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(h) = self.sampler.take() {
                // the sampler only reads and stores: a panic in it would only lose a reading
                let _ = h.join();
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod native {
    pub struct Meter;

    impl Meter {
        pub fn start() -> Self {
            Self
        }

        pub fn stage_peak_bytes(&self) -> Option<u64> {
            Some(core::arch::wasm32::memory_size::<0>() as u64 * 65_536)
        }
    }
}

/// Reads the highest memory a build's stages reach, one stage at a time.
pub struct Meter(native::Meter);

impl Meter {
    /// Start measuring (natively, the sampler thread, until this is dropped).
    #[must_use]
    pub fn start() -> Self {
        Self(native::Meter::start())
    }

    /// The highest resident size, MB, since the last call (or the start): the peak of the stage just ended.  None
    /// where the platform gives no reading.
    #[must_use]
    pub fn stage_peak_mb(&self) -> Option<f64> {
        self.0.stage_peak_bytes().map(|b| b as f64 / MB)
    }
}
