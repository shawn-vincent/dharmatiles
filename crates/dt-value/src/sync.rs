//! A lock that survives a panic elsewhere, in one place.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// `lock()` for data a panic cannot leave half-written (caches, queues, last-message slots): a thread that
/// panicked while holding the lock has already been reported at its own boundary (the build worker, the preview
/// route), so the next reader takes the data as it is instead of panicking a second time.
pub trait LockExt<T> {
    /// The guard, whether or not a holder panicked.
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(g) => g,
            Err(poisoned) => PoisonError::into_inner(poisoned),
        }
    }
}
