//! The canonical form of what a build reads: the one way a value becomes
//! the bytes a cache key is made of.
//!
//! Both caches key on it: the in-memory step cache ([`crate::cache`]) folds
//! the bytes into a 64-bit hash, and the disk store ([`crate::store`])
//! digests them with SHA-256.  Every value is written with a tag for its
//! kind and, where its size varies, its length, so two different values
//! never write the same bytes; a float is written by its bits (`-0.0` and
//! `0.0` are different keys, a NaN is its own bits), and nothing here reads
//! a span, a `Debug` text or a map's iteration order.
//!
//! A type is canonical by implementing [`Canonical`] with every field named
//! (a struct is taken apart without `..`, an enum is matched in full), so
//! adding a field is a compile error here until the key covers it.

use std::sync::Arc;

use dt_doc::place::Side;
use dt_elements::{Formula, Value};
use dt_kit::{Frame, Printer};
use dt_value::{Dist, Draw, Op, Weighting};

use crate::options::Profile;

/// The bytes of a canonical form, as it is written.
#[derive(Default)]
pub(crate) struct Canon {
    bytes: Vec<u8>,
}

/// A value that can write itself canonically.
pub(crate) trait Canonical {
    /// Write this value.
    fn canon(&self, c: &mut Canon);
}

impl Canon {
    /// An empty form.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The bytes written so far.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The form folded into 64 bits, for the in-memory cache.
    pub(crate) fn fnv(&self) -> u64 {
        crate::hash::bytes(crate::hash::OFFSET, &self.bytes)
    }

    /// A tag: one byte saying what comes next.
    pub(crate) fn tag(&mut self, t: u8) -> &mut Self {
        self.bytes.push(t);
        self
    }

    /// An unsigned number.
    pub(crate) fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes.push(b'u');
        self.bytes.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// A float, by its bits.
    pub(crate) fn f64(&mut self, v: f64) -> &mut Self {
        self.bytes.push(b'f');
        self.bytes.extend_from_slice(&v.to_bits().to_le_bytes());
        self
    }

    /// A flag.
    pub(crate) fn bool(&mut self, v: bool) -> &mut Self {
        self.bytes.push(if v { b'T' } else { b'F' });
        self
    }

    /// Text, with its length.
    pub(crate) fn str(&mut self, v: &str) -> &mut Self {
        self.bytes.push(b's');
        self.bytes
            .extend_from_slice(&(v.len() as u64).to_le_bytes());
        self.bytes.extend_from_slice(v.as_bytes());
        self
    }

    /// Any canonical value.
    pub(crate) fn put(&mut self, v: &(impl Canonical + ?Sized)) -> &mut Self {
        v.canon(self);
        self
    }

    /// A named field: the name, then the value.
    pub(crate) fn field(&mut self, name: &str, v: &(impl Canonical + ?Sized)) -> &mut Self {
        self.str(name);
        v.canon(self);
        self
    }
}

impl Canonical for f64 {
    fn canon(&self, c: &mut Canon) {
        c.f64(*self);
    }
}

impl Canonical for u64 {
    fn canon(&self, c: &mut Canon) {
        c.u64(*self);
    }
}

impl Canonical for u32 {
    fn canon(&self, c: &mut Canon) {
        c.u64(u64::from(*self));
    }
}

impl Canonical for usize {
    fn canon(&self, c: &mut Canon) {
        c.u64(*self as u64);
    }
}

impl Canonical for bool {
    fn canon(&self, c: &mut Canon) {
        c.bool(*self);
    }
}

impl Canonical for str {
    fn canon(&self, c: &mut Canon) {
        c.str(self);
    }
}

impl Canonical for String {
    fn canon(&self, c: &mut Canon) {
        c.str(self);
    }
}

impl<T: Canonical + ?Sized> Canonical for &T {
    fn canon(&self, c: &mut Canon) {
        (**self).canon(c);
    }
}

impl<T: Canonical> Canonical for Option<T> {
    fn canon(&self, c: &mut Canon) {
        match self {
            None => {
                c.tag(b'N');
            }
            Some(v) => {
                c.tag(b'S');
                v.canon(c);
            }
        }
    }
}

impl<T: Canonical> Canonical for [T] {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'[').u64(self.len() as u64);
        for v in self {
            v.canon(c);
        }
    }
}

impl<T: Canonical> Canonical for Vec<T> {
    fn canon(&self, c: &mut Canon) {
        self.as_slice().canon(c);
    }
}

impl<T: Canonical, const N: usize> Canonical for [T; N] {
    fn canon(&self, c: &mut Canon) {
        self.as_slice().canon(c);
    }
}

impl<T: Canonical> Canonical for Arc<[T]> {
    fn canon(&self, c: &mut Canon) {
        (**self).canon(c);
    }
}

impl<A: Canonical, B: Canonical> Canonical for (A, B) {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'(');
        self.0.canon(c);
        self.1.canon(c);
    }
}

impl<A: Canonical, B: Canonical, C: Canonical> Canonical for (A, B, C) {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'(');
        self.0.canon(c);
        self.1.canon(c);
        self.2.canon(c);
    }
}

impl Canonical for Frame {
    fn canon(&self, c: &mut Canon) {
        let Self {
            cols,
            rows,
            square_mm,
            cells_per_square,
        } = self;
        c.tag(b'F')
            .put(cols)
            .put(rows)
            .put(square_mm)
            .put(cells_per_square);
    }
}

impl Canonical for Printer {
    fn canon(&self, c: &mut Canon) {
        let Self {
            overhang_deg,
            steepest_deg,
            layer_mm,
            line_mm,
            gap_mm,
            reach_mm,
            span_mm,
            grid_mm,
            overhang_budget_mm2,
        } = self;
        c.tag(b'P')
            .put(overhang_deg)
            .put(steepest_deg)
            .put(layer_mm)
            .put(line_mm)
            .put(gap_mm)
            .put(reach_mm)
            .put(span_mm)
            .put(grid_mm)
            .put(overhang_budget_mm2);
    }
}

/// A printer profile is its numbers; its name is only what it is called
/// (the store adds that apart, as it names the target).
impl Canonical for Profile {
    fn canon(&self, c: &mut Canon) {
        (**self).canon(c);
    }
}

impl Canonical for Side {
    fn canon(&self, c: &mut Canon) {
        c.tag(match self {
            Side::Left => b'l',
            Side::Right => b'r',
            Side::Centre => b'c',
        });
    }
}

impl Canonical for Op {
    fn canon(&self, c: &mut Canon) {
        c.tag(match self {
            Op::Add => b'+',
            Op::Sub => b'-',
            Op::Mul => b'*',
            Op::Div => b'/',
        });
    }
}

impl Canonical for Weighting {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'W');
        match self {
            Self::Flat => {
                c.tag(b'0');
            }
            Self::Low(r) => {
                c.tag(b'1').put(r);
            }
            Self::High(r) => {
                c.tag(b'2').put(r);
            }
            Self::Bell { spread, at } => {
                c.tag(b'3').put(spread).put(at);
            }
            Self::Peak { spread, at } => {
                c.tag(b'4').put(spread).put(at);
            }
            Self::Ends { k, at } => {
                c.tag(b'5').put(k).put(at);
            }
            Self::Steps(bins) => {
                c.tag(b'6').put(bins);
            }
            Self::List(items) => {
                c.tag(b'7').put(items);
            }
            Self::Mix(parts) => {
                c.tag(b'8').u64(parts.len() as u64);
                for (w, inner) in parts.iter() {
                    c.put(w).put(inner);
                }
            }
        }
    }
}

impl Canonical for Dist {
    fn canon(&self, c: &mut Canon) {
        let Self { lo, hi, weighting } = self;
        c.tag(b'D').put(lo).put(hi).put(weighting);
    }
}

impl Canonical for Draw {
    fn canon(&self, c: &mut Canon) {
        match self {
            Self::Num(v) => {
                c.tag(b'n').put(v);
            }
            Self::Range(d) => {
                c.tag(b'r').put(d);
            }
            Self::Neg(a) => {
                c.tag(b'~').put(&**a);
            }
            Self::Bin(op, a, b) => {
                c.tag(b'b').put(op).put(&**a).put(&**b);
            }
        }
    }
}

impl Canonical for Formula {
    fn canon(&self, c: &mut Canon) {
        let Self { owner, child, key } = self;
        c.tag(b'=').str(owner.as_str()).put(child).put(key);
    }
}

impl Canonical for Value {
    fn canon(&self, c: &mut Canon) {
        c.tag(b'V');
        match self {
            Self::Number(v) => {
                c.tag(b'n').put(v);
            }
            Self::Word(w) => {
                c.tag(b'w').put(w);
            }
            Self::Range(lo, hi) => {
                c.tag(b'r').put(lo).put(hi);
            }
            Self::Dist(d) => {
                c.tag(b'd').put(d);
            }
            Self::Draw(d) => {
                c.tag(b'x').put(d);
            }
            Self::Formula(f) => {
                c.tag(b'=').put(f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(f: impl Fn(&mut Canon)) -> Vec<u8> {
        let mut c = Canon::new();
        f(&mut c);
        c.bytes().to_vec()
    }

    #[test]
    fn floats_are_their_bits() {
        assert_ne!(
            of(|c| {
                c.put(&0.0_f64);
            }),
            of(|c| {
                c.put(&-0.0_f64);
            })
        );
        assert_eq!(
            of(|c| {
                c.put(&f64::NAN);
            }),
            of(|c| {
                c.put(&f64::NAN);
            })
        );
        assert_ne!(
            of(|c| {
                c.put(&0.1_f64);
            }),
            of(|c| {
                c.put(&0.1_f64.next_up());
            })
        );
    }

    #[test]
    fn a_boundary_is_part_of_the_form() {
        assert_ne!(
            of(|c| {
                c.put("ab").put("c");
            }),
            of(|c| {
                c.put("a").put("bc");
            })
        );
        assert_ne!(
            of(|c| {
                c.put(&vec![1.0_f64]).put(&vec![2.0_f64]);
            }),
            of(|c| {
                c.put(&vec![1.0_f64, 2.0]).put(&Vec::<f64>::new());
            })
        );
        assert_ne!(
            of(|c| {
                c.put(&None::<f64>);
            }),
            of(|c| {
                c.put(&Some(0.0_f64));
            })
        );
    }

    #[test]
    fn a_weighting_is_its_numbers_and_its_kind() {
        let a = Dist::new(1.0, 6.0, Weighting::Low(4.0));
        let b = Dist::new(1.0, 6.0, Weighting::High(4.0));
        let c = Dist::new(1.0, 6.0, Weighting::Low(4.5));
        let (fa, fb, fc) = (
            of(|k| {
                k.put(&a);
            }),
            of(|k| {
                k.put(&b);
            }),
            of(|k| {
                k.put(&c);
            }),
        );
        assert_ne!(fa, fb);
        assert_ne!(fa, fc);
        assert_eq!(
            fa,
            of(|k| {
                k.put(&a.clone());
            })
        );
    }
}
