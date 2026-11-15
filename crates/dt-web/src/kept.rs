//! The one store of kept builds in the browser: [`KeptBuilds`], a [`dt_store::MemStore`] that a page's worker opens at its
//! start and hands to everything that builds (the editor's [`crate::service::Service`] and the site's cards' `build_tile`).
//!
//! Between visits the entries live in IndexedDB, which the worker's JavaScript (`web/preview/js/kept-idb.js`, the one file
//! that touches it) only reads and writes as opaque records: the key, the encoding of an entry (`dt_store`'s own bytes), the
//! size limit and the order of eviction are all decided here, by the memory store.  The worker loads the records into the
//! store before its first build ([`KeptBuilds::load`]) and then asks it to tell each change ([`KeptBuilds::persist`]), so
//! the records follow what the store keeps, and a build the store holds is read, not made.  IndexedDB is async and a
//! build is not, which is why the store is in front: nothing in a build ever waits for it.

use std::sync::Arc;

use dt_store::MemStore;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// What a worker keeps, in bytes (a browser tab holds the wasm's 4 GB, and a build is made in it as well).  The one limit,
/// for the editor's full builds and the site's drafts alike: the memory store holds to it and the records follow.
pub const KEPT_BYTES: usize = 256 << 20;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// The worker's keeper has `bytes` (copied by it before this returns) to keep under `id`, used at tick `used`.
    #[wasm_bindgen(js_namespace = globalThis, js_name = dtKeptPut)]
    fn js_kept_put(id: &str, bytes: &[u8], used: f64);

    /// The worker's keeper is to forget `id`.
    #[wasm_bindgen(js_namespace = globalThis, js_name = dtKeptDrop)]
    fn js_kept_drop(id: &str);
}

/// The builds a worker keeps, for the engine named `engine` (the page gives the engine's hash, so a mesh an older engine
/// made is never read).  Cheap to clone: every clone is the same store.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Clone)]
pub struct KeptBuilds {
    store: Arc<MemStore>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl KeptBuilds {
    /// An empty store for `engine`, bounded by [`KEPT_BYTES`].
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    #[must_use]
    pub fn new(engine: &str) -> Self {
        Self {
            store: Arc::new(MemStore::new(engine, KEPT_BYTES)),
        }
    }

    /// Take back one record a [`persist`](Self::persist)ed visit kept: its bytes and its use tick.  An entry of another
    /// engine is dropped (and its record is told dropped); a damaged one is dropped and answered in words, which the worker
    /// says on the console (an entry is a cache: the build is made again).  Empty when the entry was fine or was another
    /// engine's.
    #[must_use]
    pub fn load(&self, bytes: &[u8], used: f64) -> String {
        // a tick is a count of uses: whole and not negative
        #[expect(
            clippy::cast_sign_loss,
            reason = "a use tick counts up from zero, whole"
        )]
        let used = used as u64;
        self.store.load(vec![(bytes.to_vec(), used)]).join("; ")
    }

    /// How many builds are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// From now on tell the page's keeper every change: `globalThis.dtKeptPut(id, bytes, used)` for an entry kept and
    /// `globalThis.dtKeptDrop(id)` for one gone (kept-idb.js installs both).  A store never persisted (Node's parity run)
    /// is memory alone.
    #[cfg(target_arch = "wasm32")]
    pub fn persist(&self) {
        self.store.watch(Box::new(|change| match change {
            dt_store::Change::Put { id, bytes, used } => js_kept_put(id, bytes, used as f64),
            dt_store::Change::Drop { id } => js_kept_drop(id),
        }));
    }
}

impl KeptBuilds {
    /// The memory store, for what builds.
    #[must_use]
    pub fn store(&self) -> &Arc<MemStore> {
        &self.store
    }
}
