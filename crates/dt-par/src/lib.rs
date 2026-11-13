//! Parallel iteration for the engine, with the threads decided in one place.
//!
//! The engine crates depend on this crate under the name `rayon`
//! (`rayon = { package = "dt-par", path = "../dt-par" }`), so their code
//! (`use rayon::prelude::*`, `rayon::join`) is the same on every target.
//! Everywhere but the browser this is rayon.  WebAssembly in a browser tab has
//! no threads and rayon's pool cannot start there, so on wasm32 the same calls
//! run in order on the calling thread, in the order rayon's indexed iterators
//! collect in: results match a pool of one thread.  This is the one place that
//! says "sequential".

#[cfg(not(target_arch = "wasm32"))]
pub use rayon::*;

#[cfg(target_arch = "wasm32")]
pub use seq::*;

#[cfg(target_arch = "wasm32")]
mod seq {
    pub mod prelude {
        pub use crate::{
            IntoParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator,
            ParallelSlice, ParallelSliceMut,
        };
    }

    /// Run both closures, one after the other.
    pub fn join<A, B, RA, RB>(a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA,
        B: FnOnce() -> RB,
    {
        (a(), b())
    }

    /// A "parallel" iterator: an ordinary iterator under rayon's method names.
    pub struct Par<I>(pub I);

    impl<I: Iterator> Par<I> {
        pub fn map<R>(self, f: impl FnMut(I::Item) -> R) -> Par<impl Iterator<Item = R>> {
            Par(self.0.map(f))
        }
        pub fn filter(self, f: impl FnMut(&I::Item) -> bool) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.filter(f))
        }
        pub fn filter_map<R>(
            self,
            f: impl FnMut(I::Item) -> Option<R>,
        ) -> Par<impl Iterator<Item = R>> {
            Par(self.0.filter_map(f))
        }
        pub fn flat_map<U: IntoIterator>(
            self,
            f: impl FnMut(I::Item) -> U,
        ) -> Par<impl Iterator<Item = U::Item>> {
            Par(self.0.flat_map(f))
        }
        pub fn flat_map_iter<U: IntoIterator>(
            self,
            f: impl FnMut(I::Item) -> U,
        ) -> Par<impl Iterator<Item = U::Item>> {
            Par(self.0.flat_map(f))
        }
        pub fn flatten(self) -> Par<impl Iterator<Item = <I::Item as IntoIterator>::Item>>
        where
            I::Item: IntoIterator,
        {
            Par(self.0.flatten())
        }
        pub fn enumerate(self) -> Par<impl Iterator<Item = (usize, I::Item)>> {
            Par(self.0.enumerate())
        }
        pub fn skip(self, n: usize) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.skip(n))
        }
        pub fn take(self, n: usize) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.take(n))
        }
        pub fn step_by(self, n: usize) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.step_by(n))
        }
        pub fn inspect(self, f: impl FnMut(&I::Item)) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.inspect(f))
        }
        pub fn max(self) -> Option<I::Item>
        where
            I::Item: Ord,
        {
            self.0.max()
        }
        pub fn min(self) -> Option<I::Item>
        where
            I::Item: Ord,
        {
            self.0.min()
        }
        pub fn zip<J: IntoParallelIterator>(
            self,
            other: J,
        ) -> Par<impl Iterator<Item = (I::Item, J::Item)>> {
            Par(self.0.zip(other.into_par_iter().0))
        }
        pub fn chain<J: IntoParallelIterator<Item = I::Item>>(
            self,
            other: J,
        ) -> Par<impl Iterator<Item = I::Item>> {
            Par(self.0.chain(other.into_par_iter().0))
        }
        pub fn cloned<'a, T: 'a + Clone>(self) -> Par<impl Iterator<Item = T>>
        where
            I: Iterator<Item = &'a T>,
        {
            Par(self.0.cloned())
        }
        pub fn copied<'a, T: 'a + Copy>(self) -> Par<impl Iterator<Item = T>>
        where
            I: Iterator<Item = &'a T>,
        {
            Par(self.0.copied())
        }
        #[must_use]
        pub fn with_min_len(self, _n: usize) -> Self {
            self
        }
        #[must_use]
        pub fn with_max_len(self, _n: usize) -> Self {
            self
        }
        pub fn collect<C: FromIterator<I::Item>>(self) -> C {
            self.0.collect()
        }
        pub fn for_each(self, f: impl FnMut(I::Item)) {
            self.0.for_each(f);
        }
        pub fn sum<S: std::iter::Sum<I::Item>>(self) -> S {
            self.0.sum()
        }
        pub fn count(self) -> usize {
            self.0.count()
        }
        pub fn any(mut self, f: impl FnMut(I::Item) -> bool) -> bool {
            self.0.any(f)
        }
        pub fn all(mut self, f: impl FnMut(I::Item) -> bool) -> bool {
            self.0.all(f)
        }
        pub fn find_any(mut self, f: impl FnMut(&I::Item) -> bool) -> Option<I::Item> {
            self.0.find(f)
        }
        pub fn min_by_key<K: Ord>(self, f: impl FnMut(&I::Item) -> K) -> Option<I::Item> {
            self.0.min_by_key(f)
        }
        pub fn max_by_key<K: Ord>(self, f: impl FnMut(&I::Item) -> K) -> Option<I::Item> {
            self.0.max_by_key(f)
        }
        pub fn min_by(
            self,
            f: impl FnMut(&I::Item, &I::Item) -> std::cmp::Ordering,
        ) -> Option<I::Item> {
            self.0.min_by(f)
        }
        pub fn max_by(
            self,
            f: impl FnMut(&I::Item, &I::Item) -> std::cmp::Ordering,
        ) -> Option<I::Item> {
            self.0.max_by(f)
        }
        /// Rayon's `reduce`: an identity and an operation.
        pub fn reduce(
            self,
            identity: impl Fn() -> I::Item,
            op: impl FnMut(I::Item, I::Item) -> I::Item,
        ) -> I::Item {
            self.0.fold(identity(), op)
        }
        /// Rayon's `fold`: one accumulator here, where rayon has one per split.
        pub fn fold<T>(
            self,
            identity: impl Fn() -> T,
            op: impl FnMut(T, I::Item) -> T,
        ) -> Par<std::option::IntoIter<T>> {
            Par(Some(self.0.fold(identity(), op)).into_iter())
        }
        pub fn unzip<A, B, FA: Default + Extend<A>, FB: Default + Extend<B>>(self) -> (FA, FB)
        where
            I: Iterator<Item = (A, B)>,
        {
            self.0.unzip()
        }
    }

    pub trait IntoParallelIterator {
        type Item;
        type Iter: Iterator<Item = Self::Item>;
        fn into_par_iter(self) -> Par<Self::Iter>;
    }

    impl<T: IntoIterator> IntoParallelIterator for T {
        type Item = T::Item;
        type Iter = T::IntoIter;
        fn into_par_iter(self) -> Par<Self::Iter> {
            Par(self.into_iter())
        }
    }

    impl<I: Iterator> IntoIterator for Par<I> {
        type Item = I::Item;
        type IntoIter = I;
        fn into_iter(self) -> I {
            self.0
        }
    }

    pub trait IntoParallelRefIterator<'a> {
        type Item: 'a;
        type Iter: Iterator<Item = Self::Item>;
        fn par_iter(&'a self) -> Par<Self::Iter>;
    }

    impl<'a, T: 'a> IntoParallelRefIterator<'a> for [T] {
        type Item = &'a T;
        type Iter = std::slice::Iter<'a, T>;
        fn par_iter(&'a self) -> Par<Self::Iter> {
            Par(self.iter())
        }
    }

    impl<'a, T: 'a> IntoParallelRefIterator<'a> for Vec<T> {
        type Item = &'a T;
        type Iter = std::slice::Iter<'a, T>;
        fn par_iter(&'a self) -> Par<Self::Iter> {
            Par(self.iter())
        }
    }

    pub trait IntoParallelRefMutIterator<'a> {
        type Item: 'a;
        type Iter: Iterator<Item = Self::Item>;
        fn par_iter_mut(&'a mut self) -> Par<Self::Iter>;
    }

    impl<'a, T: 'a> IntoParallelRefMutIterator<'a> for [T] {
        type Item = &'a mut T;
        type Iter = std::slice::IterMut<'a, T>;
        fn par_iter_mut(&'a mut self) -> Par<Self::Iter> {
            Par(self.iter_mut())
        }
    }

    impl<'a, T: 'a> IntoParallelRefMutIterator<'a> for Vec<T> {
        type Item = &'a mut T;
        type Iter = std::slice::IterMut<'a, T>;
        fn par_iter_mut(&'a mut self) -> Par<Self::Iter> {
            Par(self.iter_mut())
        }
    }

    pub trait ParallelSlice<T> {
        fn par_chunks(&self, n: usize) -> Par<std::slice::Chunks<'_, T>>;
    }

    impl<T> ParallelSlice<T> for [T] {
        fn par_chunks(&self, n: usize) -> Par<std::slice::Chunks<'_, T>> {
            Par(self.chunks(n))
        }
    }

    pub trait ParallelSliceMut<T> {
        fn par_chunks_mut(&mut self, n: usize) -> Par<std::slice::ChunksMut<'_, T>>;
    }

    impl<T> ParallelSliceMut<T> for [T] {
        fn par_chunks_mut(&mut self, n: usize) -> Par<std::slice::ChunksMut<'_, T>> {
            Par(self.chunks_mut(n))
        }
    }
}
