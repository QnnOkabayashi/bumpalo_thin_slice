// TODO(quinn): inline this for better formatting
#![doc = include_str!("../README.md")]
use bumpalo::Bump;
use core::{alloc::Layout, ptr::NonNull};

mod thin_slice;
mod thin_slice_mut;

pub use thin_slice::ThinSlice;
pub use thin_slice_mut::ThinSliceMut;

mod sealed {
    pub trait Sealed {}

    impl Sealed for bumpalo::Bump {}
}

pub trait BumpaloThinSliceExt: sealed::Sealed {
    fn alloc_thin_slice_clone<T>(&self, src: &[T]) -> ThinSliceMut<'_, T>
    where
        T: Clone;

    fn alloc_thin_slice_copy<T>(&self, src: &[T]) -> ThinSliceMut<'_, T>
    where
        T: Copy;

    fn alloc_thin_slice_fill_clone<T>(&self, len: usize, value: &T) -> ThinSliceMut<'_, T>
    where
        T: Clone;

    fn alloc_thin_slice_fill_copy<T>(&self, len: usize, value: T) -> ThinSliceMut<'_, T>
    where
        T: Copy;

    fn alloc_thin_slice_fill_default<T>(&self, len: usize) -> ThinSliceMut<'_, T>
    where
        T: Default;

    fn alloc_thin_slice_fill_iter<T, I>(&self, iter: I) -> ThinSliceMut<'_, T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator;

    fn alloc_thin_slice_fill_with<T, F>(&self, len: usize, f: F) -> ThinSliceMut<'_, T>
    where
        F: FnMut(usize) -> T;
}

impl BumpaloThinSliceExt for Bump {
    fn alloc_thin_slice_clone<T>(&self, src: &[T]) -> ThinSliceMut<'_, T>
    where
        T: Clone,
    {
        ThinSliceMut::new_clone(self, src)
    }

    fn alloc_thin_slice_copy<T>(&self, src: &[T]) -> ThinSliceMut<'_, T>
    where
        T: Copy,
    {
        ThinSliceMut::new_copy(self, src)
    }

    fn alloc_thin_slice_fill_clone<T>(&self, len: usize, value: &T) -> ThinSliceMut<'_, T>
    where
        T: Clone,
    {
        ThinSliceMut::from_fn(self, len, |_| value.clone())
    }

    fn alloc_thin_slice_fill_copy<T>(&self, len: usize, value: T) -> ThinSliceMut<'_, T>
    where
        T: Copy,
    {
        ThinSliceMut::from_fn(self, len, |_| value)
    }

    fn alloc_thin_slice_fill_default<T>(&self, len: usize) -> ThinSliceMut<'_, T>
    where
        T: Default,
    {
        ThinSliceMut::from_fn(self, len, |_| T::default())
    }

    fn alloc_thin_slice_fill_iter<T, I>(&self, iter: I) -> ThinSliceMut<'_, T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = iter.into_iter();
        ThinSliceMut::from_fn(self, iter.len(), |_| {
            iter.next().expect("Iterator supplied to few elements")
        })
    }

    fn alloc_thin_slice_fill_with<T, F>(&self, len: usize, f: F) -> ThinSliceMut<'_, T>
    where
        F: FnMut(usize) -> T,
    {
        ThinSliceMut::from_fn(self, len, f)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let bump = Bump::new();
        let slice1: ThinSliceMut<'_, i32> = bump.alloc_thin_slice_fill_iter(0..10);
        let slice2: ThinSliceMut<'_, i32> = bump.alloc_thin_slice_fill_iter(0..10);
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], slice1.as_slice());
        assert_eq!(slice1, slice2);
    }

    #[test]
    fn same_size_when_stored_in_option() {
        use core::mem::size_of;
        assert_eq!(
            size_of::<ThinSliceMut<'_, i32>>(),
            size_of::<Option<ThinSliceMut<'_, i32>>>()
        );
    }

    #[test]
    fn empty() {
        let bump = Bump::new();
        let slice1: ThinSliceMut<'_, i32> = bump.alloc_thin_slice_fill_iter(0..0);
        let slice2: ThinSliceMut<'_, i32> = ThinSliceMut::default();
        assert_eq!(&[] as &[i32], slice1.as_slice());
        assert_eq!(slice1, slice2);
    }

    #[test]
    fn same_empty_singleton() {
        let bump = Bump::new();
        let slice1: ThinSliceMut<'_, i32> = bump.alloc_thin_slice_fill_iter(0..0);
        let slice2: ThinSliceMut<'_, i32> = ThinSliceMut::default();
        assert_eq!(slice1.header, slice2.header);
    }

    #[test]
    fn same_empty_singleton_extreme() {
        let bump = Bump::new();
        (0..10)
            .map(|_| bump.alloc_thin_slice_fill_iter(0..0))
            .chain((0..10).map(|_| bump.alloc_thin_slice_fill_copy(0, 100)))
            .for_each(|thin_slice| {
                assert_eq!(ThinSliceMut::<i32>::default().header, thin_slice.header);
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
