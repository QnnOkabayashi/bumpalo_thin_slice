# bumpalo_thin_slice
[![github-img]][github-url] [![crates-img]][crates-url] [![docs-img]][docs-url]

[github-url]: https://github.com/QnnOkabayashi/bumpalo_thin_slice
[crates-url]: https://crates.io/crates/bumpalo_thin_slice
[docs-url]: https://docs.rs/bumpalo_thin_slice/latest/bumpalo_thin_slice
[github-img]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
[crates-img]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
[docs-img]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K

Thin slice type for `bumpalo`.

# Overview

`bumpalo_thin_slice` provides two items: `ThinSlice<'bump, T>` and `BumpaloThinSliceExt`.

`ThinSlice<'bump, T>` is a struct consisting of a single pointer into a `Bump` allocation where the length and data is stored contiguously.
This allows for storing slices in less space, which can be beneficial for reducing the size of nodes in tree structures stored in a `Bump` allocator.
It also dereferences to a regular slice, allowing for all the flexibility that slices in the standard library come with.

`BumpaloThinSliceExt` is a trait implemented for `bumpalo::Bump` that provides `.alloc_thin_slice_*()` methods for each corresponding `.alloc_slice_*()` method that `Bump` comes with.

Using these two items, you should be able to replace all `Bump`-allocated `&'bump [T]` and `&'bump mut [T]` in your program with `ThinSlice<'bump, T>`, and all `bump.alloc_slice_*()` methods with `bump.alloc_thin_slice_*()`.

# Getting started

TODO

```
use bumpalo_thin_slice::{BumpaloThinSliceExt, ThinSlice};
```

# Examples

TODO

