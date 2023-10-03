use crate::{data, len, thin_slice::ThinSlice, Header};
use bumpalo::Bump;
use core::{
    alloc::Layout,
    cmp, fmt, hash,
    marker::PhantomData,
    ops,
    ptr::{self, NonNull},
    slice,
};

/// `ThinSliceMut<'a, T>` is exactly the same as `&'a mut [T]`, except that
/// both the length and data live in the buffer owned by the [`Bump`] allocator.
///
/// This makes the memory footprint of `ThinSliceMut`s lower. Being pointer-sized
/// also means it can be passed/stored in registers.
///
/// Properties of `&mut [T]` that are preserved:
/// * `size_of::<ThinSliceMut<'_, T>>()` == `size_of::<Option<ThinSliceMut<'_, T>>()`
/// * The empty `ThinSliceMut` points to a statically allocated singleton.
///
/// Note that this type is intentially not `Copy` or `Clone`. Use
/// [`ThinSliceMut::as_thin_slice`] to get a [`ThinSlice`] instead, which you
/// may then freely copy.
pub struct ThinSliceMut<'a, T> {
    pub(crate) header: NonNull<Header>,
    pub(crate) _marker: PhantomData<&'a mut [T]>,
}

impl<'a, T> ThinSliceMut<'a, T> {
    /// Allocate a new `ThinSliceMut` with a given length and initialization
    /// function that accepts a pointer to an uninitialized array of `len`
    /// elements.
    ///
    /// Note that you may prefer [`ThinSliceMut::from_fn`] for a safe
    /// constructor instead.
    ///
    /// # Safety
    ///
    /// This function is unsafe because `init` is responsible for ensuring
    /// that all elements are properly initialized before returning.
    ///
    /// # Examples
    ///
    /// Implementation of [`ThinSliceMut::new_copy`].
    /// ```
    /// # use bumpalo_thin_slice::ThinSliceMut;
    /// # use bumpalo::Bump;
    /// fn new_copy<'a, T: Copy>(
    ///     bump: &'a Bump,
    ///     src: &[T],
    /// ) -> ThinSliceMut<'a, T> {
    ///     unsafe {
    ///         ThinSliceMut::new(bump, src.len(), |dst: *mut T| {
    ///             core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
    ///         })
    ///     }
    /// }
    /// ```
    pub unsafe fn new<F>(bump: &'a Bump, len: usize, init: F) -> Self
    where
        F: FnOnce(*mut T),
    {
        if len == 0 {
            return ThinSliceMut::default();
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

        ThinSliceMut {
            header,
            _marker: PhantomData,
        }
    }

    /// Allocate a new `ThinSliceMut` whose elements are cloned from `src`.
    pub fn new_clone(bump: &'a Bump, src: &[T]) -> Self
    where
        T: Clone,
    {
        unsafe {
            ThinSliceMut::new(bump, src.len(), |dst: *mut T| {
                for (i, val) in src.iter().cloned().enumerate() {
                    // SAFETY: pointer points to a valid allocation
                    ptr::write(dst.add(i), val);
                }
            })
        }
    }

    /// Allocate a new `ThinSliceMut` whose elements are copied from `src`.
    pub fn new_copy(bump: &'a Bump, src: &[T]) -> Self
    where
        T: Copy,
    {
        unsafe {
            ThinSliceMut::new(bump, src.len(), |dst: *mut T| {
                ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
            })
        }
    }

    /// Allocate a new `ThinSliceMut` with a provided length given the closure
    /// `f`, which returns each element `T` using that element's index.
    pub fn from_fn<F>(bump: &'a Bump, len: usize, mut f: F) -> Self
    where
        F: FnMut(usize) -> T,
    {
        unsafe {
            ThinSliceMut::new(bump, len, |dst: *mut T| {
                for i in 0..len {
                    ptr::write(dst.add(i), f(i));
                }
            })
        }
    }

    /// Borrows `self` and returns a [`ThinSlice`] that lives as long as `self`
    /// is borrowed.
    pub fn as_thin_slice(&self) -> ThinSlice<'_, T> {
        ThinSlice {
            header: self.header,
            _marker: PhantomData,
        }
    }

    /// Consumes `self` and returns a [`ThinSlice`] that lives as long as the
    /// underlying bump allocator.
    pub fn into_thin_slice(self) -> ThinSlice<'a, T> {
        ThinSlice {
            header: self.header,
            _marker: PhantomData,
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

    /// Convert the `ThinSliceMut` into a mut slice containing the contents of
    /// `self`, whose lifetime is bound by `'a`.
    #[must_use]
    pub fn into_slice(self) -> &'a mut [T] {
        // Same as `as_mut_slice`, but lifetime is bound differently
        unsafe { slice::from_raw_parts_mut(data(self.header), len(self.header)) }
    }
}

impl<T> Default for ThinSliceMut<'_, T> {
    fn default() -> Self {
        static EMPTY: Header = 0;

        ThinSliceMut {
            header: NonNull::from(&EMPTY),
            _marker: PhantomData,
        }
    }
}

impl<T> ops::Deref for ThinSliceMut<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> ops::DerefMut for ThinSliceMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T> fmt::Debug for ThinSliceMut<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

impl<T> hash::Hash for ThinSliceMut<'_, T>
where
    T: hash::Hash,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<T> cmp::PartialEq for ThinSliceMut<'_, T>
where
    T: cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T> cmp::Eq for ThinSliceMut<'_, T> where T: cmp::Eq {}

impl<T> cmp::PartialOrd for ThinSliceMut<'_, T>
where
    T: cmp::PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<T> cmp::Ord for ThinSliceMut<'_, T>
where
    T: cmp::Ord,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<'a, T> IntoIterator for &'a ThinSliceMut<'_, T> {
    type Item = &'a T;

    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut ThinSliceMut<'_, T> {
    type Item = &'a mut T;

    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
