//! Pin the small set of `zenlib` type aliases the crate uses internally,
//! so submodules can `use crate::zenlib_re_export::int64u` without each
//! file pulling in the full `zenlib` prelude.

#[allow(unused_imports)]
pub use zenlib::{float32, float64, float80, int128u, int16u, int32u, int64u, int8u};
