//! dharmatiles: `std::time::Instant` panics on wasm32-unknown-unknown (a
//! browser tab has no clock the standard library can reach), and the builders
//! read it only to fill the build-time statistics they are handed.  There it
//! is a clock that does not run; everywhere else it is std's.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        Self
    }

    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}
