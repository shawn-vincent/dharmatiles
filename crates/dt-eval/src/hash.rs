//! The one hash of the evaluator: FNV-1a, for cache keys, plan stages and
//! the document's text.

/// FNV-1a's offset basis: where a hash starts.
pub(crate) const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// One word folded into `h`.
pub(crate) fn step(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x0000_0100_0000_01b3)
}

/// Every byte folded into `h`, one at a time.
pub(crate) fn bytes(h: u64, b: &[u8]) -> u64 {
    b.iter().fold(h, |h, &b| step(h, u64::from(b)))
}

/// `b` folded into `h`, then its length (so `"ab","c"` is not `"a","bc"`).
pub(crate) fn field(h: u64, b: &[u8]) -> u64 {
    step(bytes(h, b), b.len() as u64)
}
