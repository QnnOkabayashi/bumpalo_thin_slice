// TODO(quinn): inline this for better formatting
#![doc = include_str!("../README.md")]
use bumpalo::Bump;
use core::{
    alloc::Layout,
    cmp, fmt, hash,
    marker::PhantomData,
    mem::MaybeUninit,
    ops,
    ptr::{self, NonNull},
    slice,
};

mod sealed {
    pub trait Sealed {}

    impl Sealed for bumpalo::Bump {}
}

pub trait BumpaloThinSliceExt: sealed::Sealed {
    fn alloc_thin_slice_clone<T>(&self, src: &[T]) -> ThinSlice<'_, T>
    where
        T: Clone;

    fn alloc_thin_slice_copy<T>(&self, src: &[T]) -> ThinSlice<'_, T>
    where
        T: Copy;

    fn alloc_thin_slice_fill_clone<T>(&self, len: usize, value: &T) -> ThinSlice<'_, T>
    where
        T: Clone;

    fn alloc_thin_slice_fill_copy<T>(&self, len: usize, value: T) -> ThinSlice<'_, T>
    where
        T: Copy;

    fn alloc_thin_slice_fill_default<T>(&self, len: usize) -> ThinSlice<'_, T>
    where
        T: Default;

    fn alloc_thin_slice_fill_iter<T, I>(&self, iter: I) -> ThinSlice<'_, T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator;

    fn alloc_thin_slice_fill_with<T, F>(&self, len: usize, f: F) -> ThinSlice<'_, T>
    where
        F: FnMut(usize) -> T;
}

impl BumpaloThinSliceExt for Bump {
    fn alloc_thin_slice_clone<T>(&self, src: &[T]) -> ThinSlice<'_, T>
    where
        T: Clone,
    {
        ThinSlice::new_clone(self, src)
    }

    fn alloc_thin_slice_copy<T>(&self, src: &[T]) -> ThinSlice<'_, T>
    where
        T: Copy,
    {
        ThinSlice::new_copy(self, src)
    }

    fn alloc_thin_slice_fill_clone<T>(&self, len: usize, value: &T) -> ThinSlice<'_, T>
    where
        T: Clone,
    {
        ThinSlice::from_fn(self, len, |_| value.clone())
    }

    fn alloc_thin_slice_fill_copy<T>(&self, len: usize, value: T) -> ThinSlice<'_, T>
    where
        T: Copy,
    {
        ThinSlice::from_fn(self, len, |_| value)
    }

    fn alloc_thin_slice_fill_default<T>(&self, len: usize) -> ThinSlice<'_, T>
    where
        T: Default,
    {
        ThinSlice::from_fn(self, len, |_| T::default())
    }

    fn alloc_thin_slice_fill_iter<T, I>(&self, iter: I) -> ThinSlice<'_, T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = iter.into_iter();
        ThinSlice::from_fn(self, iter.len(), |_| {
            iter.next().expect("Iterator supplied to few elements")
        })
    }

    fn alloc_thin_slice_fill_with<T, F>(&self, len: usize, f: F) -> ThinSlice<'_, T>
    where
        F: FnMut(usize) -> T,
    {
        ThinSlice::from_fn(self, len, f)
    }
}

type Header = usize;

/// Returns the length stored in the header.
///
/// # Safety
///
/// Same safety invariants as [`NonNull::as_ref`].
unsafe fn len(header: NonNull<Header>) -> usize {
    *header.as_ref()
}

/// Returns the data following a header.
///
/// # Safety
///
/// Must point to a valid allocation of `Header`, e.g. cannot dangle.
unsafe fn data<T>(header: NonNull<Header>) -> *mut T {
    // This gets constant-folded even at opt-level=1
    let header_layout = Layout::new::<Header>();
    let array_layout = Layout::new::<T>();
    let offset = header_layout.extend(array_layout).unwrap().1;

    // SAFETY: `offset` cannot be larger than `isize::MAX`, and
    // the validity of the allocation is upheld by the caller.
    unsafe { header.as_ptr().cast::<u8>().add(offset).cast::<T>() }
}

/// `ThinSlice<'bump, T>` is exactly the same as `&'bump mut [T]`, except that
/// both the length and data live in the buffer owned by the [`Bump`] allocator.
///
/// This makes the memory footprint of `ThinSlice`s lower. Being pointer-sized
/// also means it can be passed/stored in registers.
///
/// Properties of `&[T]` that are preserved:
/// * `size_of::<ThinSlice<'_, T>>()` == `size_of::<Option<ThinSlice<'_, T>>()`
/// * The empty `ThinSlice` points to a statically allocated singleton.
///
/// Note that this type is intentially not `Copy` or `Clone`. This is
/// because if you cloned it, you would have two pointers to the same
/// allocation which could then be converted into mutable slices, violating
/// Rust's uniqueness invariants. Use [`ThinSlice::as_slice`] to get a slice
/// instead, which you may then freely copy.
pub struct ThinSlice<'bump, T> {
    header: NonNull<Header>,
    _marker: PhantomData<&'bump mut [T]>,
}

impl<'bump, T> ThinSlice<'bump, T> {
    /// Allocate a new `ThinSlice` with a given length and initialization
    /// function that accepts a pointer to an uninitialized array of `len`
    /// elements.
    ///
    /// # Safety
    ///
    /// This function is unsafe because `init` is responsible for ensuring
    /// that all elements are properly initialized before returning.
    ///
    /// # Examples
    ///
    /// Implementation of [`ThinSlice::new_copy`].
    /// ```
    /// # use bumpalo_thin_slice::ThinSlice;
    /// # use bumpalo::Bump;
    /// fn new_copy<'bump, T: Copy>(
    ///     bump: &'bump Bump,
    ///     src: &[T],
    /// ) -> ThinSlice<'bump, T> {
    ///     unsafe {
    ///         ThinSlice::new(bump, src.len(), |dst: *mut T| {
    ///             core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
    ///         })
    ///     }
    /// }
    /// ```
    pub unsafe fn new<F>(bump: &'bump Bump, len: usize, init: F) -> Self
    where
        F: FnOnce(*mut T),
    {
        if len == 0 {
            return ThinSlice::default();
        }

        let layout = {
            let header_layout = Layout::new::<Header>();
            let array_layout = Layout::array::<T>(len).expect("array size is too large");
            header_layout
                .extend(array_layout)
                .expect("array size is too large")
                .0
        };

        let header = bump.alloc_layout(layout).cast::<Header>();

        unsafe {
            ptr::write(header.as_ptr(), len);
            init(data(header));
        }

        ThinSlice {
            header,
            _marker: PhantomData,
        }
    }

    /// Allocate a new `ThinSlice` whose elements are cloned from `src`.
    pub fn new_clone(bump: &'bump Bump, src: &[T]) -> Self
    where
        T: Clone,
    {
        unsafe {
            ThinSlice::new(bump, src.len(), |dst: *mut T| {
                for (i, val) in src.iter().cloned().enumerate() {
                    // SAFETY: pointer points to a valid allocation
                    ptr::write(dst.add(i), val);
                }
            })
        }
    }

    /// Allocate a new `ThinSlice` whose elements are copied from `src`.
    pub fn new_copy(bump: &'bump Bump, src: &[T]) -> Self
    where
        T: Copy,
    {
        unsafe {
            ThinSlice::new(bump, src.len(), |dst: *mut T| {
                ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
            })
        }
    }

    /// Allocate a new `ThinSlice` with a provided length given the closure
    /// `f`, which returns each element `T` using that element's index.
    pub fn from_fn<F>(bump: &'bump Bump, len: usize, mut f: F) -> Self
    where
        F: FnMut(usize) -> T,
    {
        unsafe {
            ThinSlice::new(bump, len, |dst: *mut T| {
                for i in 0..len {
                    ptr::write(dst.add(i), f(i));
                }
            })
        }
    }

    /// Returns a slice containing the contents of `self`.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(data(self.header), len(self.header)) }
    }

    /// Returns a mut slice containing the contents of `self`.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(data(self.header), len(self.header)) }
    }

    /// Convert the `ThinSlice` into a mut slice containing the contents of
    /// `self`, whose lifetime is bound by `'bump`.
    #[must_use]
    pub fn into_slice(self) -> &'bump mut [T] {
        // Same as `as_mut_slice`, but lifetime is bound differently
        unsafe { slice::from_raw_parts_mut(data(self.header), len(self.header)) }
    }
}

impl<T> Default for ThinSlice<'_, T> {
    fn default() -> Self {
        static EMPTY: Header = 0;

        ThinSlice {
            header: NonNull::from(&EMPTY),
            _marker: PhantomData,
        }
    }
}

impl<'bump, T> ThinSlice<'bump, MaybeUninit<T>> {
    /// Allocate a new `ThinSlice<'_, MaybeUninit<T>>` using a provided length.
    pub fn new_uninit(bump: &'bump Bump, len: usize) -> Self {
        unsafe {
            ThinSlice::new(bump, len, |_dst: *mut MaybeUninit<T>| {
                // Do nothing. Values of type `MaybeUninit` are already
                // initialized on uninitialized memory by definition.
            })
        }
    }

    /// Convert a `ThinSlice<'_, MaybeUninit<T>>` to `ThinSlice<'_, T>`.
    ///
    /// # Safety
    ///
    /// The entire slice must be properly initialized. The easiest way
    /// to do this is to iterate through with [`ThinSlice::as_mut_slice`]
    /// and populate each element directly.
    #[must_use]
    pub unsafe fn assume_init(self) -> ThinSlice<'bump, T> {
        ThinSlice {
            header: self.header,
            _marker: PhantomData,
        }
    }
}

impl<T> ops::Deref for ThinSlice<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> ops::DerefMut for ThinSlice<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T> fmt::Debug for ThinSlice<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

impl<T> hash::Hash for ThinSlice<'_, T>
where
    T: hash::Hash,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<T> cmp::PartialEq for ThinSlice<'_, T>
where
    T: cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T> cmp::Eq for ThinSlice<'_, T> where T: cmp::Eq {}

impl<T> cmp::PartialOrd for ThinSlice<'_, T>
where
    T: cmp::PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<T> cmp::Ord for ThinSlice<'_, T>
where
    T: cmp::Ord,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<'a, T> IntoIterator for &'a ThinSlice<'_, T> {
    type Item = &'a T;

    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut ThinSlice<'_, T> {
    type Item = &'a mut T;

    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let bump = Bump::new();
        let slice1: ThinSlice<'_, i32> = bump.alloc_thin_slice_fill_iter(0..10);
        let slice2: ThinSlice<'_, i32> = bump.alloc_thin_slice_fill_iter(0..10);
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], slice1.as_slice());
        assert_eq!(slice1, slice2);
    }

    #[test]
    fn same_size_when_stored_in_option() {
        use core::mem::size_of;
        assert_eq!(
            size_of::<ThinSlice<'_, i32>>(),
            size_of::<Option<ThinSlice<'_, i32>>>()
        );
    }

    #[test]
    fn empty() {
        let bump = Bump::new();
        let slice1: ThinSlice<'_, i32> = bump.alloc_thin_slice_fill_iter(0..0);
        let slice2: ThinSlice<'_, i32> = ThinSlice::default();
        assert_eq!(&[] as &[i32], slice1.as_slice());
        assert_eq!(slice1, slice2);
    }

    #[test]
    fn same_empty_singleton() {
        let bump = Bump::new();
        let slice1: ThinSlice<'_, i32> = bump.alloc_thin_slice_fill_iter(0..0);
        let slice2: ThinSlice<'_, i32> = ThinSlice::default();
        assert_eq!(slice1.header, slice2.header);
    }

    #[test]
    fn same_empty_singleton_extreme() {
        let bump = Bump::new();
        (0..10)
            .map(|_| bump.alloc_thin_slice_fill_iter(0..0))
            .chain((0..10).map(|_| bump.alloc_thin_slice_fill_copy(0, 100)))
            .for_each(|thin_slice| {
                assert_eq!(ThinSlice::<i32>::default().header, thin_slice.header);
            });
    }

    #[test]
    fn alloc_thin_slice_clone() {
        let bump = Bump::new();
        let slice = &[1, 2, 3, 4];
        let thin = bump.alloc_thin_slice_clone(slice);
        assert_eq!(slice, thin.as_slice());
    }

    #[test]
    fn alloc_thin_slice_copy() {
        let bump = Bump::new();
        let slice = &[1, 2, 3, 4];
        let thin = bump.alloc_thin_slice_copy(slice);
        assert_eq!(slice, thin.as_slice());
    }

    #[test]
    fn deref() {
        let bump = Bump::new();
        let slice = bump.alloc_thin_slice_copy(&[1, 2, 3]);
        assert_eq!(&slice[..], &[1, 2, 3]);
    }

    #[test]
    fn into_iter() {
        let bump = Bump::new();
        let slice = bump.alloc_thin_slice_copy(&[1, 2, 3]);
        for (a, b) in [1, 2, 3].iter().zip(&slice) {
            assert_eq!(a, b);
        }
    }
}
