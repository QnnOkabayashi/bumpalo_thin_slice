use crate::{data, len, Header};
use core::{cmp, fmt, hash, marker::PhantomData, ops, ptr::NonNull, slice};

/// `ThinSlice<'a, T>` is exactly the same as `&'a [T]`, except that
/// both the length and data live in the buffer owned by the [`Bump`] allocator.
///
/// This makes the memory footprint of `ThinSlice`s lower. Being pointer-sized
/// also means it can be passed/stored in registers.
///
/// Properties of `&[T]` that are preserved:
/// * `size_of::<ThinSlice<'_, T>>()` == `size_of::<Option<ThinSlice<'_, T>>()`
/// * The empty `ThinSlice` points to a statically allocated singleton.
#[derive(Copy, Clone)]
pub struct ThinSlice<'a, T> {
    pub(crate) header: NonNull<Header>,
    pub(crate) _marker: PhantomData<&'a [T]>,
}

impl<'a, T> ThinSlice<'a, T> {
    /// Returns a slice containing the contents of `self`.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(data(self.header), len(self.header)) }
    }

    /// Convert the `ThinSlice` into a mut slice containing the contents of
    /// `self`, whose lifetime is bound by `'a`.
    #[must_use]
    pub fn into_slice(self) -> &'a [T] {
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

impl<T> ops::Deref for ThinSlice<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
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
