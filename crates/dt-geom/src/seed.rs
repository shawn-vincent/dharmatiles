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

/// A seeded random stream (SplitMix64), for draws that must follow one
/// another, such as dart throwing. Make one per keyed seed; never share
/// one across threads. Seeded maths uses `libm`.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// The stream for a seed.
    #[must_use]
    pub const fn new(seed: Seed) -> Self {
        Self(seed.0)
    }

    /// The next 64 random bits.
    pub const fn next_u64(&mut self) -> u64 {
        let r = splitmix64(self.0);
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        r
    }

    /// Uniform in [0, 1).
    pub fn random(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let u = (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
        u
    }

    /// Uniform in [lo, hi).
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.random()
    }

    /// A whole number from `lo` to `hi`, both included.
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        #[allow(clippy::cast_sign_loss)]
        let span = (hi - lo) as u64 + 1;
        #[allow(clippy::cast_possible_wrap)]
        let k = (self.next_u64() % span) as i64;
        lo + k
    }

    /// Normal with mean `mu` and standard deviation `sigma` (Box–Muller).
    pub fn normal(&mut self, mu: f64, sigma: f64) -> f64 {
        let u1 = 1.0 - self.random(); // (0, 1]
        let u2 = self.random();
        mu + sigma * libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(2.0 * core::f64::consts::PI * u2)
    }

    /// Poisson with mean `lambda`: Knuth's product method up to 30, the
    /// rounded normal approximation above.
    pub fn poisson(&mut self, lambda: f64) -> u64 {
        if lambda.is_nan() || lambda <= 0.0 {
            return 0;
        }
        if lambda > 30.0 {
            let v = libm::round(self.normal(lambda, libm::sqrt(lambda)));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return v.max(0.0) as u64;
        }
        let limit = libm::exp(-lambda);
        let (mut k, mut p) = (0u64, 1.0);
        loop {
            p *= self.random();
            if p <= limit {
                return k;
            }
            k += 1;
        }
    }
}

/// Domain tag keeping `index(i)` apart from `key(..)`.
const INDEX_TAG: u64 = 0x6474_2d69_6e64_6578; // "dt-index"

/// SplitMix64 finaliser (Steele, Lea and Flood, 2014).
pub(crate) const fn splitmix64(x: u64) -> u64 {
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
    fn rng_draws_are_in_range_and_keyed() {
        let mut r = Rng::new(Seed::new(9).key("a"));
        let mut s = Rng::new(Seed::new(9).key("a"));
        for _ in 0..1000 {
            let u = r.random();
            assert!((0.0..1.0).contains(&u));
            assert_eq!(u.to_bits(), s.random().to_bits());
            let k = r.int(4, 7);
            s.int(4, 7);
            assert!((4..=7).contains(&k));
        }
        let n: Vec<f64> = (0..20_000).map(|_| r.normal(1.0, 2.0)).collect();
        #[allow(clippy::cast_precision_loss)]
        let m = n.iter().sum::<f64>() / n.len() as f64;
        assert!((m - 1.0).abs() < 0.05);
        #[allow(clippy::cast_precision_loss)]
        let p = (0..4000).map(|_| r.poisson(3.5)).sum::<u64>() as f64 / 4000.0;
        assert!((p - 3.5).abs() < 0.1, "{p}");
        assert_eq!(r.poisson(0.0), 0);
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
