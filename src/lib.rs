use bumpalo::Bump;
use core::{
    alloc::{Layout, LayoutError},
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
        unsafe {
            ThinSlice::new(self, src.len(), |dst: *mut T| {
                for (i, val) in src.iter().cloned().enumerate() {
                    // SAFETY: pointer points to a valid allocation
                    ptr::write(dst.add(i), val);
                }
            })
        }
    }

    fn alloc_thin_slice_copy<T>(&self, src: &[T]) -> ThinSlice<'_, T>
    where
        T: Copy,
    {
        unsafe {
            ThinSlice::new(self, src.len(), |dst| {
                ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut T, src.len());
            })
        }
    }

    fn alloc_thin_slice_fill_clone<T>(&self, len: usize, value: &T) -> ThinSlice<'_, T>
    where
        T: Clone,
    {
        self.alloc_thin_slice_fill_with(len, |_| value.clone())
    }

    fn alloc_thin_slice_fill_copy<T>(&self, len: usize, value: T) -> ThinSlice<'_, T>
    where
        T: Copy,
    {
        self.alloc_thin_slice_fill_with(len, |_| value)
    }

    fn alloc_thin_slice_fill_default<T>(&self, len: usize) -> ThinSlice<'_, T>
    where
        T: Default,
    {
        self.alloc_thin_slice_fill_with(len, |_| T::default())
    }

    fn alloc_thin_slice_fill_iter<T, I>(&self, iter: I) -> ThinSlice<'_, T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = iter.into_iter();
        self.alloc_thin_slice_fill_with(iter.len(), |_| {
            iter.next().expect("Iterator supplied to few elements")
        })
    }

    fn alloc_thin_slice_fill_with<T, F>(&self, len: usize, mut f: F) -> ThinSlice<'_, T>
    where
        F: FnMut(usize) -> T,
    {
        unsafe {
            ThinSlice::new(self, len, |dst: *mut T| {
                for i in 0..len {
                    ptr::write(dst.add(i), f(i));
                }
            })
        }
    }
}

#[repr(transparent)]
struct Header {
    len: usize,
}

impl Header {
    const fn new(len: usize) -> Self {
        Header { len }
    }

    fn empty() -> NonNull<Self> {
        static EMPTY: Header = Header::new(0);

        NonNull::from(&EMPTY)
    }

    fn array_layout<T>(len: usize) -> Result<Layout, LayoutError> {
        // This gets constant-folded even at opt-level=1
        let size_layout = Layout::new::<usize>();
        let array_layout = Layout::array::<T>(len)?;
        let layout = size_layout.extend(array_layout)?.0;

        Ok(layout)
    }

    fn data<T>(this: NonNull<Self>) -> *mut T {
        // This gets constant-folded even at opt-level=1
        let size_layout = Layout::new::<usize>();
        let array_layout = Layout::new::<T>();
        let offset = size_layout.extend(array_layout).unwrap().1;

        unsafe { this.as_ptr().cast::<u8>().add(offset).cast::<T>() }
    }
}

/// `ThinSlice<'bump, T>` is exactly the same as `&'bump mut [T]`, except that
/// both the length and data live in a [`Bump`] allocator.
///
/// This makes the memory footprint of `ThinSlice`s lower. Being pointer-sized
/// also means it can be passed/stored in registers.
///
/// Properties of `&[T]` that are preserved:
/// * `size_of::<ThinSlice<'_, T>>()` == `size_of::<Option<ThinSlice<'_, T>>()`
/// * The empty `ThinSlice` points to a statically allocated singleton.
///
/// Note that this type is intentially not [`Copy`] or [`Clone`]. This is
/// because if you cloned it, you would have two pointers to the same
/// allocation which could then be converted into mutable slices, violating
/// Rust's uniqueness invariants. Use [`ThinSlice::as_slice`] to get a slice
/// instead, which you may then freely copy.
pub struct ThinSlice<'bump, T> {
    header: NonNull<Header>,
    _marker: PhantomData<&'bump mut [T]>,
}

impl<'bump, T> ThinSlice<'bump, T> {
    /// # Safety
    ///
    /// `init` must properly initialize the array of length `len` pointed
    /// to by the input to proper `T` values.
    pub unsafe fn new<F>(bump: &'bump Bump, len: usize, init: F) -> Self
    where
        F: FnOnce(*mut T),
    {
        if len == 0 {
            return ThinSlice::default();
        }

        // Create the layout of the allocation
        let layout = Header::array_layout::<T>(len).expect("array size is too large");

        // Allocate
        let header = bump.alloc_layout(layout).cast::<Header>();

        unsafe {
            // Initialize header
            ptr::write(header.as_ptr(), Header::new(len));

            // Initialize values
            init(Header::data(header));
        }

        ThinSlice {
            header,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying slice with the lifetime of the bump allocator.
    pub fn as_slice(&self) -> &'bump [T] {
        unsafe {
            let len = self.header.as_ref().len;
            let data = Header::data(self.header);
            slice::from_raw_parts(data, len)
        }
    }

    /// Returns the underlying mutable slice with the lifetime of the bump allocator.
    pub fn as_mut_slice(&mut self) -> &'bump mut [T] {
        unsafe {
            let len = self.header.as_ref().len;
            let data = Header::data(self.header);
            slice::from_raw_parts_mut(data, len)
        }
    }
}

impl<T> Default for ThinSlice<'_, T> {
    fn default() -> Self {
        ThinSlice {
            header: Header::empty(),
            _marker: PhantomData,
        }
    }
}

impl<'bump, T> ThinSlice<'bump, MaybeUninit<T>> {
    /// # Safety
    ///
    /// The entire slice must be properly initialized. The easiest way
    /// to do this is to iterate through with [`TinySlice::as_mut_slice`]
    /// and populate each element directly.
    pub unsafe fn assume_init(self) -> ThinSlice<'bump, T> {
        ThinSlice {
            header: self.header.cast(),
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
        self.as_slice().hash(state)
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
}
