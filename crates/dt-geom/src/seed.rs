//! Keyed seeds: every random stream is derived from the document seed and a
//! path of keys (`tile/region/meadow/grass/3`), never from a shared mutable
//! RNG. The same path always gives the same seed, regardless of thread count
//! or evaluation order.
//!
//! Derivation: each key is hashed with 64-bit FNV-1a, xored into the parent
//! state and finalised with the SplitMix64 mixer. The constants are frozen:
//! changing them changes every tile, which the golden hashes would catch.

/// A deterministic seed at some point in the document's key path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seed(u64);

impl Seed {
    /// The root seed of a document.
    #[must_use]
    pub const fn new(root: u64) -> Self {
        Self(splitmix64(root))
    }

    /// The seed for a named child key.
    #[must_use]
    pub const fn key(self, key: &str) -> Self {
        Self(splitmix64(self.0 ^ fnv1a(key.as_bytes())))
    }

    /// The seed for the `i`-th member of a collection.
    #[must_use]
    pub const fn index(self, i: u64) -> Self {
        Self(splitmix64(self.0 ^ splitmix64(i ^ INDEX_TAG)))
    }

    /// The seed at a `/`-separated path below this one. Empty segments are
    /// skipped, so `"a//b"` equals `"a/b"`.
    #[must_use]
    pub fn path(self, path: &str) -> Self {
        path.split('/')
            .filter(|s| !s.is_empty())
            .fold(self, Self::key)
    }

    /// The raw 64-bit value, for seeding a generator.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Domain tag keeping `index(i)` apart from `key(..)`.
const INDEX_TAG: u64 = 0x6474_2d69_6e64_6578; // "dt-index"

/// SplitMix64 finaliser (Steele, Lea and Flood, 2014).
const fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 64-bit FNV-1a over bytes.
const fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_same_seed() {
        let a = Seed::new(1).path("tile/meadow/grass").index(3);
        let b = Seed::new(1).key("tile").key("meadow").key("grass").index(3);
        assert_eq!(a, b);
        assert_eq!(Seed::new(1).path("a//b/"), Seed::new(1).path("a/b"));
    }

    #[test]
    fn different_paths_differ() {
        let root = Seed::new(1);
        assert_ne!(root.key("rocks"), root.key("grass"));
        assert_ne!(root.path("a/b"), root.path("b/a"));
        assert_ne!(root.index(0), root.index(1));
        assert_ne!(Seed::new(1).key("x"), Seed::new(2).key("x"));
        assert_ne!(root.key("0"), root.index(0));
    }

    #[test]
    fn frozen_values() {
        // Golden values: if these change, every tile changes.
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(
            Seed::new(1).path("tile/meadow").value(),
            Seed::new(1).key("tile").key("meadow").value()
        );
    }

    #[test]
    fn same_across_threads() {
        let expected: Vec<u64> = (0..64)
            .map(|i| Seed::new(7).path("t/r").index(i).value())
            .collect();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..64)
                        .map(|i| Seed::new(7).path("t/r").index(i).value())
                        .collect::<Vec<u64>>()
                })
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), expected);
        }
    }
}
